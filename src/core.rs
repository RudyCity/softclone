use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::{Child, Command},
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloneRecord {
    pub id: Uuid,
    pub name: String,
    pub source: PathBuf,
    pub root: PathBuf,
    pub entrypoint: PathBuf,
    pub isolated_profile: bool,
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

fn checked_copy_dir(source: &Path, destination: &Path) -> Result<u64> {
    let mut bytes = 0u64;
    for item in WalkDir::new(source).follow_links(false) {
        let item = item.map_err(std::io::Error::other)?;
        let relative = item
            .path()
            .strip_prefix(source)
            .map_err(|e| CloneError::Invalid(e.to_string()))?;
        let to = destination.join(relative);
        if item.file_type().is_symlink() {
            return Err(CloneError::Link(item.path().display().to_string()));
        }
        if item.file_type().is_dir() {
            fs::create_dir_all(&to)?;
        } else if item.file_type().is_file() {
            bytes = bytes.saturating_add(fs::copy(item.path(), to)?);
        } else {
            return Err(CloneError::Invalid(format!(
                "Unsupported special file: {}",
                item.path().display()
            )));
        }
    }
    Ok(bytes)
}

/// Clones portable application files into a fresh directory. Does not copy profiles.
pub fn create_clone(prepared: &Prepared, library_dir: &Path) -> Result<CloneRecord> {
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
    let outcome = (|| -> Result<u64> {
        let payload = staging.join("payload");
        fs::create_dir(&payload)?;
        let bytes = if is_app_bundle(&prepared.source) {
            let bundle = prepared
                .source
                .file_name()
                .ok_or_else(|| CloneError::Invalid("Missing app bundle name".into()))?;
            checked_copy_dir(&prepared.source, &payload.join(bundle))?
        } else if prepared.source.is_dir() {
            checked_copy_dir(&prepared.source, &payload)?
        } else {
            let name = prepared
                .source
                .file_name()
                .ok_or_else(|| CloneError::Invalid("Invalid filename".into()))?;
            fs::copy(&prepared.source, payload.join(name))?
        };
        if prepared.isolated_profile {
            for sub in ["config", "data", "cache", "roaming", "local"] {
                fs::create_dir_all(staging.join("profile").join(sub))?;
            }
        }
        fs::rename(&staging, &root)?;
        Ok(bytes)
    })();
    let copied_bytes = match outcome {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    Ok(CloneRecord {
        id,
        name: prepared.name.clone(),
        source: prepared.source.clone(),
        root,
        entrypoint: prepared.entrypoint.clone(),
        isolated_profile: prepared.isolated_profile,
        created_epoch_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        copied_bytes,
    })
}

/// Launches by direct process API, never via an interpolated command string.
pub fn launch(record: &CloneRecord) -> Result<Child> {
    let payload = fs::canonicalize(record.root.join("payload"))?;
    let target = fs::canonicalize(payload.join(&record.entrypoint))?;
    if !target.starts_with(&payload) || !can_launch(&target) {
        return Err(CloneError::Invalid(
            "Cloned executable is unavailable or outside its payload.".into(),
        ));
    }
    let mut command = if is_app_bundle(&target) {
        let mut cmd = Command::new("open");
        cmd.args(["-n", "-a"]).arg(&target);
        cmd
    } else {
        Command::new(&target)
    };
    command.current_dir(&payload);
    if record.isolated_profile {
        let profile = record.root.join("profile");
        for sub in ["config", "data", "cache", "roaming", "local"] {
            fs::create_dir_all(profile.join(sub))?;
        }
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
