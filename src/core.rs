use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum CloneError {
    #[error("{0}")]
    Invalid(String),
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot copy symbolic links: {0}")]
    Link(String),
    #[error("Clone cancelled; incomplete files were discarded")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, CloneError>;

#[derive(Clone, Debug)]
pub struct CloneRequest {
    pub source: PathBuf,
    pub name: String,
    /// Relative to source directory; empty means auto-detect.
    pub entrypoint: String,
    /// Environment-level isolation only. This is not an OS security sandbox.
    pub isolated_profile: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    #[default]
    Copy,
    ProfileOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloneRecord {
    pub id: Uuid,
    pub name: String,
    pub source: PathBuf,
    pub root: PathBuf,
    pub entrypoint: PathBuf,
    pub isolated_profile: bool,
    #[serde(default)]
    pub run_mode: RunMode,
    pub created_epoch_secs: u64,
    pub copied_bytes: u64,
}

#[derive(Debug)]
pub struct Prepared {
    pub source: PathBuf,
    pub name: String,
    pub entrypoint: PathBuf,
    pub isolated_profile: bool,
}

pub fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.chars().count() > 64
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
    {
        return Err(CloneError::Invalid(
            "Use 1–64 characters without path separators or reserved characters.".into(),
        ));
    }
    Ok(name.to_owned())
}

fn safe_relative(entry: &str) -> Result<PathBuf> {
    let path = PathBuf::from(entry);
    if entry.is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(CloneError::Invalid(
            "Entrypoint must be a relative path within the source folder.".into(),
        ));
    }
    Ok(path)
}

fn is_app_bundle(path: &Path) -> bool {
    cfg!(target_os = "macos")
        && path.is_dir()
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
}

fn can_launch(path: &Path) -> bool {
    if is_app_bundle(path) {
        return true;
    }
    if !path.is_file() {
        return false;
    }
    if cfg!(target_os = "windows") {
        return path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[allow(unreachable_code)]
    false
}

fn auto_entry(source: &Path) -> Result<PathBuf> {
    if is_app_bundle(source) {
        return Ok(PathBuf::from("."));
    }
    let mut candidates = Vec::new();
    for item in fs::read_dir(source)? {
        let item = item?;
        if can_launch(&item.path()) {
            candidates.push(PathBuf::from(item.file_name()));
        }
    }
    candidates.sort();
    match candidates.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(CloneError::Invalid(
            "No top-level executable found. Enter its relative path.".into(),
        )),
        _ => Err(CloneError::Invalid(
            "Multiple executables found. Specify the relative entrypoint.".into(),
        )),
    }
}

/// Checks name, source and executable path without modifying the source.
pub fn prepare(request: &CloneRequest) -> Result<Prepared> {
    let name = validate_name(&request.name)?;
    let top = fs::symlink_metadata(&request.source)?;
    if top.file_type().is_symlink() {
        return Err(CloneError::Link(request.source.display().to_string()));
    }
    let source = fs::canonicalize(&request.source)?;
    if !(top.is_file() || top.is_dir()) {
        return Err(CloneError::Invalid(
            "Source must be a file or directory.".into(),
        ));
    }
    let entrypoint = if top.is_file() {
        if !request.entrypoint.trim().is_empty() {
            return Err(CloneError::Invalid(
                "Entrypoint is only used with a source directory.".into(),
            ));
        }
        if !can_launch(&source) {
            return Err(CloneError::Invalid(
                "File is not executable on this operating system.".into(),
            ));
        }
        PathBuf::from(
            source
                .file_name()
                .ok_or_else(|| CloneError::Invalid("Missing filename".into()))?,
        )
    } else if is_app_bundle(&source) {
        if !request.entrypoint.trim().is_empty() {
            return Err(CloneError::Invalid(
                "A macOS app bundle needs no entrypoint.".into(),
            ));
        }
        PathBuf::from(
            source
                .file_name()
                .ok_or_else(|| CloneError::Invalid("Missing app bundle name".into()))?,
        )
    } else {
        let entry = if request.entrypoint.trim().is_empty() {
            auto_entry(&source)?
        } else {
            safe_relative(request.entrypoint.trim())?
        };
        let target = fs::canonicalize(source.join(&entry))?;
        if !target.starts_with(&source) || !can_launch(&target) {
            return Err(CloneError::Invalid(
                "Entrypoint must be an executable inside the source directory.".into(),
            ));
        }
        entry
    };
    Ok(Prepared {
        source,
        name,
        entrypoint,
        isolated_profile: request.isolated_profile,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CopyPlan {
    pub total_bytes: u64,
    pub total_files: u64,
}

#[derive(Clone, Debug)]
pub struct CopyProgress {
    pub copied_bytes: u64,
    pub total_bytes: u64,
    pub completed_files: u64,
    pub total_files: u64,
    pub current_file: String,
}

fn abort_if_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(CloneError::Cancelled)
    } else {
        Ok(())
    }
}

fn validate_source_link(source: &Path, path: &Path) -> Result<()> {
    let link = fs::read_link(path)?;
    let resolved = fs::canonicalize(path)?;
    if link.is_absolute() || !resolved.starts_with(source) {
        return Err(CloneError::Link(path.display().to_string()));
    }
    #[cfg(not(unix))]
    return Err(CloneError::Link(path.display().to_string()));
    #[cfg(unix)]
    Ok(())
}

/// Inspects source files without modifying the source or destination.
/// Estimates byte count and validates internal symlinks, failing closed on
/// special files and paths that escape the source root.
pub fn analyze(prepared: &Prepared) -> Result<CopyPlan> {
    if prepared.source.is_file() {
        return Ok(CopyPlan {
            total_bytes: fs::metadata(&prepared.source)?.len(),
            total_files: 1,
        });
    }
    let mut plan = CopyPlan::default();
    for item in WalkDir::new(&prepared.source).follow_links(false) {
        let item = item.map_err(std::io::Error::other)?;
        if item.file_type().is_symlink() {
            validate_source_link(&prepared.source, item.path())?;
        } else if item.file_type().is_file() {
            plan.total_bytes = plan
                .total_bytes
                .saturating_add(fs::metadata(item.path())?.len());
            plan.total_files = plan.total_files.saturating_add(1);
        } else if !item.file_type().is_dir() {
            return Err(CloneError::Invalid(format!(
                "Unsupported special file: {}",
                item.path().display()
            )));
        }
    }
    Ok(plan)
}

fn copy_file_with_progress<F>(
    source: &Path,
    to: &Path,
    cancel: &AtomicBool,
    plan: CopyPlan,
    copied_bytes: &mut u64,
    completed_files: &mut u64,
    progress: &mut F,
) -> Result<()>
where
    F: FnMut(CopyProgress),
{
    abort_if_cancelled(cancel)?;
    if !fs::symlink_metadata(source)?.file_type().is_file() {
        return Err(CloneError::Invalid(format!(
            "Source changed during copy: {}",
            source.display()
        )));
    }
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    let mut buffer = [0u8; 256 * 1024];
    let current_file = source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut since_report = 0u64;
    loop {
        abort_if_cancelled(cancel)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        *copied_bytes = copied_bytes.saturating_add(count as u64);
        since_report += count as u64;
        if since_report >= 2 * 1024 * 1024 {
            progress(CopyProgress {
                copied_bytes: *copied_bytes,
                total_bytes: plan.total_bytes,
                completed_files: *completed_files,
                total_files: plan.total_files,
                current_file: current_file.clone(),
            });
            since_report = 0;
        }
    }
    output.flush()?;
    fs::set_permissions(to, fs::metadata(source)?.permissions())?;
    *completed_files = completed_files.saturating_add(1);
    progress(CopyProgress {
        copied_bytes: *copied_bytes,
        total_bytes: plan.total_bytes,
        completed_files: *completed_files,
        total_files: plan.total_files,
        current_file,
    });
    Ok(())
}

fn checked_copy_dir<F>(
    source: &Path,
    destination: &Path,
    cancel: &AtomicBool,
    plan: CopyPlan,
    copied_bytes: &mut u64,
    completed_files: &mut u64,
    progress: &mut F,
) -> Result<()>
where
    F: FnMut(CopyProgress),
{
    for item in WalkDir::new(source).follow_links(false) {
        abort_if_cancelled(cancel)?;
        let item = item.map_err(std::io::Error::other)?;
        let relative = item
            .path()
            .strip_prefix(source)
            .map_err(|e| CloneError::Invalid(e.to_string()))?;
        let to = destination.join(relative);
        if item.file_type().is_symlink() {
            validate_source_link(source, item.path())?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(item.path())?, &to)?;
            #[cfg(not(unix))]
            return Err(CloneError::Link(item.path().display().to_string()));
        } else if item.file_type().is_dir() {
            fs::create_dir_all(&to)?;
        } else if item.file_type().is_file() {
            copy_file_with_progress(
                item.path(),
                &to,
                cancel,
                plan,
                copied_bytes,
                completed_files,
                progress,
            )?;
        } else {
            return Err(CloneError::Invalid(format!(
                "Unsupported special file: {}",
                item.path().display()
            )));
        }
    }
    Ok(())
}

/// Backward-compatible copy API; uses the same staged, validated engine.
pub fn create_clone(prepared: &Prepared, library_dir: &Path) -> Result<CloneRecord> {
    create_clone_with_progress(prepared, library_dir, &AtomicBool::new(false), |_| {})
}

/// A cancellable copy that reports real byte/file progress. Temporary staging
/// files are deleted on any error or cancellation; no registry record is added.
pub fn create_clone_with_progress<F>(
    prepared: &Prepared,
    library_dir: &Path,
    cancel: &AtomicBool,
    mut progress: F,
) -> Result<CloneRecord>
where
    F: FnMut(CopyProgress),
{
    abort_if_cancelled(cancel)?;
    // Guard before creating directories so a nested destination never mutates the source.
    let mut ancestor = library_dir;
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| CloneError::Invalid("Library has no existing ancestor.".into()))?;
    }
    if fs::canonicalize(ancestor)?.starts_with(&prepared.source) {
        return Err(CloneError::Invalid(
            "Library must be outside the source directory.".into(),
        ));
    }
    fs::create_dir_all(library_dir)?;
    let library_dir = fs::canonicalize(library_dir)?;
    if library_dir.starts_with(&prepared.source) {
        return Err(CloneError::Invalid(
            "Library must be outside the source directory.".into(),
        ));
    }
    if prepared.source.starts_with(&library_dir) {
        return Err(CloneError::Invalid(
            "Source must be outside managed clone library.".into(),
        ));
    }
    let plan = analyze(prepared)?;
    abort_if_cancelled(cancel)?;
    let id = Uuid::new_v4();
    let slug: String = prepared
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "app" } else { slug };
    let root = library_dir.join(format!("{slug}-{id}"));
    let staging = library_dir.join(format!(".{slug}-{id}.partial"));
    fs::create_dir(&staging)?;
    let mut copied_bytes = 0u64;
    let mut completed_files = 0u64;
    let outcome = (|| -> Result<()> {
        abort_if_cancelled(cancel)?;
        let payload = staging.join("payload");
        fs::create_dir(&payload)?;
        if is_app_bundle(&prepared.source) {
            let bundle = prepared
                .source
                .file_name()
                .ok_or_else(|| CloneError::Invalid("Missing app bundle name".into()))?;
            checked_copy_dir(
                &prepared.source,
                &payload.join(bundle),
                cancel,
                plan,
                &mut copied_bytes,
                &mut completed_files,
                &mut progress,
            )?;
        } else if prepared.source.is_dir() {
            checked_copy_dir(
                &prepared.source,
                &payload,
                cancel,
                plan,
                &mut copied_bytes,
                &mut completed_files,
                &mut progress,
            )?;
        } else {
            let name = prepared
                .source
                .file_name()
                .ok_or_else(|| CloneError::Invalid("Invalid filename".into()))?;
            copy_file_with_progress(
                &prepared.source,
                &payload.join(name),
                cancel,
                plan,
                &mut copied_bytes,
                &mut completed_files,
                &mut progress,
            )?;
        }
        abort_if_cancelled(cancel)?;
        if prepared.isolated_profile {
            for sub in ["config", "data", "cache", "roaming", "local"] {
                fs::create_dir_all(staging.join("profile").join(sub))?;
            }
        }
        abort_if_cancelled(cancel)?;
        fs::rename(&staging, &root)?;
        Ok(())
    })();
    if let Err(error) = outcome {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(CloneRecord {
        id,
        name: prepared.name.clone(),
        source: prepared.source.clone(),
        root,
        entrypoint: prepared.entrypoint.clone(),
        isolated_profile: prepared.isolated_profile,
        run_mode: RunMode::Copy,
        created_epoch_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        copied_bytes,
    })
}

/// Creates a separate profile without duplicating installed application binaries.
/// The original application will be launched; its files are never modified here.
pub fn create_profile_instance(prepared: &Prepared, library_dir: &Path) -> Result<CloneRecord> {
    fs::create_dir_all(library_dir)?;
    let library = fs::canonicalize(library_dir)?;
    let id = Uuid::new_v4();
    let root = library.join(format!("profile-{id}"));
    fs::create_dir(&root)?;
    let result = (|| -> Result<()> {
        for sub in [
            "config", "data", "cache", "roaming", "local", "browser", "firefox",
        ] {
            fs::create_dir_all(root.join("profile").join(sub))?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&root);
        return Err(error);
    }
    Ok(CloneRecord {
        id,
        name: prepared.name.clone(),
        source: prepared.source.clone(),
        root,
        entrypoint: prepared.entrypoint.clone(),
        isolated_profile: true,
        run_mode: RunMode::ProfileOnly,
        created_epoch_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        copied_bytes: 0,
    })
}

/// Launches a copied app or a managed profile of an installed app.
/// Profiles are best-effort; this is not a sandbox or license bypass.
pub fn launch(record: &CloneRecord) -> Result<Child> {
    let (target, working_dir) = match record.run_mode {
        RunMode::Copy => {
            let payload = fs::canonicalize(record.root.join("payload"))?;
            let target = fs::canonicalize(payload.join(&record.entrypoint))?;
            if !target.starts_with(&payload) {
                return Err(CloneError::Invalid(
                    "Cloned executable is outside its payload.".into(),
                ));
            }
            (target, payload)
        }
        RunMode::ProfileOnly => {
            let source = fs::canonicalize(&record.source)?;
            if !source.is_file() && !source.is_dir() {
                return Err(CloneError::Invalid(
                    "Original app is no longer available.".into(),
                ));
            }
            let (target, directory) = if source.is_dir() {
                let target = if is_app_bundle(&source) {
                    source.clone()
                } else {
                    let relative = safe_relative(record.entrypoint.to_str().ok_or_else(|| {
                        CloneError::Invalid("Entrypoint contains invalid characters.".into())
                    })?)?;
                    fs::canonicalize(source.join(relative))?
                };
                if !target.starts_with(&source) {
                    return Err(CloneError::Invalid(
                        "Entrypoint escaped original application folder.".into(),
                    ));
                }
                (target, source)
            } else {
                let directory = source
                    .parent()
                    .ok_or_else(|| CloneError::Invalid("Executable has no parent.".into()))?
                    .to_path_buf();
                (source, directory)
            };
            (target, directory)
        }
    };
    if !can_launch(&target) {
        return Err(CloneError::Invalid(
            "The executable is missing or cannot launch on this OS.".into(),
        ));
    }

    let profile = record.root.join("profile");
    let arguments = if record.isolated_profile {
        for sub in [
            "config", "data", "cache", "roaming", "local", "browser", "firefox",
        ] {
            fs::create_dir_all(profile.join(sub))?;
        }
        crate::universal::adapter_args(&target, &profile)
    } else {
        Vec::new()
    };

    let mut command = if is_app_bundle(&target) {
        let mut cmd = Command::new("open");
        cmd.args(["-n", "-a"]).arg(&target);
        if !arguments.is_empty() {
            cmd.arg("--args").args(&arguments);
        }
        cmd
    } else {
        let mut cmd = Command::new(&target);
        cmd.args(&arguments);
        cmd
    };
    command.current_dir(&working_dir);
    if record.isolated_profile {
        command.env("SOFTCLONE_PROFILE_DIR", &profile);
        command.env("XDG_CONFIG_HOME", profile.join("config"));
        command.env("XDG_DATA_HOME", profile.join("data"));
        command.env("XDG_CACHE_HOME", profile.join("cache"));
        command.env("APPDATA", profile.join("roaming"));
        command.env("LOCALAPPDATA", profile.join("local"));
    }
    Ok(command.spawn()?)
}

pub fn open_folder(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    command.arg(path).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_names() {
        for name in ["", "..", "test/a", "C:\\thing", "tab\nhere"] {
            assert!(validate_name(name).is_err());
        }
        assert_eq!(validate_name(" My App ").unwrap(), "My App");
    }
    #[test]
    fn rejects_traversal() {
        for path in ["../evil", "/usr/bin/ls", "x/../y", "."] {
            assert!(safe_relative(path).is_err());
        }
    }
}
