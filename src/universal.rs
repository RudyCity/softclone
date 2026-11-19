//! User-space app discovery and known profile adapters.
//! Discovery is read-only. A discovered app is never launched automatically.
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::env;
#[cfg(not(target_os = "windows"))]
use std::fs;
use std::{
    collections::HashSet,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    ProfileAdapter,
    CopyCandidate,
    ReviewRequired,
}

impl Support {
    pub fn label(self) -> &'static str {
        match self {
            Self::ProfileAdapter => "Independent profile",
            Self::CopyCandidate => "Portable copy candidate",
            Self::ReviewRequired => "Requires review",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiscoveredApp {
    pub name: String,
    pub path: PathBuf,
    pub support: Support,
    pub note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    Chromium,
    Firefox,
}

/// Only recognize apps with known, documented per-instance command-line flags.
/// These are hints, not an OS sandbox or a license bypass.
pub fn adapter_for(path: &Path) -> Option<Adapter> {
    let name = path.file_stem()?.to_string_lossy().to_lowercase();
    let chromium = [
        "chrome",
        "google chrome",
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "microsoft edge",
        "msedge",
        "brave browser",
        "brave",
        "brave-browser",
        "vivaldi",
        "vivaldi-stable",
    ];
    if chromium.contains(&name.as_str()) {
        return Some(Adapter::Chromium);
    }
    if ["firefox", "firefox developer edition", "firefox-esr"].contains(&name.as_str()) {
        return Some(Adapter::Firefox);
    }
    None
}

pub fn adapter_args(target: &Path, profile_dir: &Path) -> Vec<OsString> {
    match adapter_for(target) {
        Some(Adapter::Chromium) => {
            let mut arg = OsString::from("--user-data-dir=");
            arg.push(profile_dir.join("browser").as_os_str());
            vec![arg, OsString::from("--no-first-run")]
        }
        Some(Adapter::Firefox) => vec![
            OsString::from("-no-remote"),
            OsString::from("-profile"),
            profile_dir.join("firefox").into_os_string(),
        ],
        None => Vec::new(),
    }
}

fn candidate(name: String, path: PathBuf, installed: bool) -> DiscoveredApp {
    let support = if adapter_for(&path).is_some() {
        Support::ProfileAdapter
    } else if installed {
        Support::ReviewRequired
    } else {
        Support::CopyCandidate
    };
    let note = match support {
        Support::ProfileAdapter => "Known browser supports dedicated profile flags; app files can stay installed.",
        Support::CopyCandidate => "Copying app files may work if all dependencies are portable.",
        Support::ReviewRequired => "Installed app may require system services, registry, sandbox, or a license. Profile-only mode is experimental.",
    };
    DiscoveredApp {
        name,
        path,
        support,
        note: note.into(),
    }
}

fn add(items: &mut Vec<DiscoveredApp>, seen: &mut HashSet<String>, item: DiscoveredApp) {
    if items.len() >= 300 {
        return;
    }
    let identity = item.path.to_string_lossy().to_lowercase();
    if seen.insert(identity) {
        items.push(item);
    }
}

#[cfg(target_os = "macos")]
fn macos_apps(items: &mut Vec<DiscoveredApp>, seen: &mut HashSet<String>) {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    for root in roots {
        let Ok(children) = fs::read_dir(&root) else {
            continue;
        };
        for child in children.flatten() {
            let path = child.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_dir() || meta.file_type().is_symlink() {
                continue;
            }
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
            {
                let title = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                add(items, seen, candidate(title, path, true));
            } else if path.file_name().is_some_and(|n| n == "Utilities") {
                if let Ok(nested) = fs::read_dir(path) {
                    for entry in nested.flatten() {
                        let app = entry.path();
                        if app.is_dir()
                            && app
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("app"))
                        {
                            let title = app
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            add(items, seen, candidate(title, app, true));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_apps(items: &mut Vec<DiscoveredApp>, seen: &mut HashSet<String>) {
    let mut roots = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = env::var_os(key) {
            roots.push(PathBuf::from(root));
        }
    }
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs"));
    }

    // Installed browsers are frequently nested, for example:
    // Program Files/Google/Chrome/Application/chrome.exe.
    // Walk a bounded number of entries without following junctions/symlinks.
    let mut known = Vec::new();
    let mut likely = Vec::new();
    let mut others = Vec::new();
    for root in roots {
        for entry in walkdir::WalkDir::new(root)
            .max_depth(5)
            .follow_links(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .take(12000)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
            {
                continue;
            }
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let lowercase = name.to_lowercase();
            if [
                "uninstall",
                "setup",
                "update",
                "helper",
                "crash",
                "service",
                "install",
                "broker",
            ]
            .iter()
            .any(|word| lowercase.contains(word))
            {
                continue;
            }
            let parent_name = path
                .parent()
                .and_then(|parent| parent.file_name())
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let parent_name =
                if ["application", "bin", "x64"].contains(&parent_name.to_lowercase().as_str()) {
                    path.parent()
                        .and_then(|p| p.parent())
                        .and_then(|p| p.file_name())
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                } else {
                    parent_name
                };
            let app_name = if parent_name.is_empty() {
                name.clone()
            } else {
                parent_name
            };
            let strong_match = app_name.eq_ignore_ascii_case(&name);
            let app = candidate(app_name, path.to_path_buf(), true);
            if app.support == Support::ProfileAdapter {
                known.push(app);
            } else if strong_match {
                likely.push(app);
            } else {
                others.push(app);
            }
        }
    }
    for app in known.into_iter().chain(likely).chain(others).take(300) {
        add(items, seen, app);
    }
}

/// A non-executing parser for desktop entry Exec tokens.
/// Returns None for shell syntax or command wrappers; never evaluates them.
pub fn first_exec_token(exec_line: &str) -> Option<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;
    for c in exec_line.chars() {
        if escape {
            token.push(c);
            escape = false;
            continue;
        }
        match c {
            '\\' => escape = true,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            c if quote == Some(c) => quote = None,
            c if c.is_whitespace() && quote.is_none() => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
            }
            _ => token.push(c),
        }
    }
    if escape || quote.is_some() {
        return None;
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    if tokens.is_empty() {
        return None;
    }
    let mut offset = 0;
    if tokens[0] == "env" {
        offset = 1;
        while offset < tokens.len() && tokens[offset].contains('=') {
            offset += 1;
        }
    }
    let executable = tokens.get(offset)?;
    if executable
        .chars()
        .any(|c| matches!(c, '$' | ';' | '|' | '&' | '<' | '>' | '%' | '\n' | '\r'))
        || ["sh", "bash", "zsh", "flatpak", "snap", "pkexec", "sudo"].contains(&executable.as_str())
    {
        return None;
    }
    Some(executable.clone())
}

pub fn parse_desktop_file(content: &str) -> Option<(String, String)> {
    let mut section = String::new();
    let mut name = None;
    let mut exec = None;
    let mut visible = true;
    let mut app = true;
    for line in content.lines().take(1500) {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line.to_string();
            continue;
        }
        if section != "[Desktop Entry]" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(value.trim().to_string()),
            "Hidden" | "NoDisplay" if value.eq_ignore_ascii_case("true") => visible = false,
            "Type" if value != "Application" => app = false,
            "Terminal" if value.eq_ignore_ascii_case("true") => visible = false,
            _ => {}
        }
    }
    if !visible || !app {
        return None;
    }
    Some((name?, first_exec_token(&exec?)?))
}

#[cfg(target_os = "linux")]
fn resolve_binary(binary: &str) -> Option<PathBuf> {
    let path = PathBuf::from(binary);
    if path.is_absolute() {
        return fs::canonicalize(path).ok().filter(|p| p.is_file());
    }
    if path.components().count() != 1 {
        return None;
    }
    let path_dirs = env::var_os("PATH")?;
    for dir in env::split_paths(&path_dirs) {
        let executable = dir.join(binary);
        if let Ok(p) = fs::canonicalize(executable) {
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn linux_apps(items: &mut Vec<DiscoveredApp>, seen: &mut HashSet<String>) {
    let mut roots = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local/share/applications"));
    }
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten().take(700) {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            if meta.len() > 64 * 1024 {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Some((name, binary)) = parse_desktop_file(&content) else {
                continue;
            };
            let Some(executable) = resolve_binary(&binary) else {
                continue;
            };
            add(items, seen, candidate(name, executable, true));
        }
    }
    if let Ok(entries) = fs::read_dir("/opt") {
        for entry in entries.flatten().take(200) {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            if let Ok(sub) = fs::read_dir(dir) {
                for file in sub.flatten().take(80) {
                    let path = file.path();
                    if path.is_file()
                        && path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("AppImage"))
                    {
                        let title = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        add(items, seen, candidate(title, path, false));
                    }
                }
            }
        }
    }
}

/// Read-only local discovery. Results explicitly indicate support limitations.
pub fn discover_apps() -> Vec<DiscoveredApp> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    #[cfg(target_os = "windows")]
    windows_apps(&mut items, &mut seen);
    #[cfg(target_os = "macos")]
    macos_apps(&mut items, &mut seen);
    #[cfg(target_os = "linux")]
    linux_apps(&mut items, &mut seen);
    items.sort_by_key(|item| item.name.to_lowercase());
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_browsers_use_profiles() {
        assert_eq!(
            adapter_for(Path::new("chrome.exe")),
            Some(Adapter::Chromium)
        );
        assert_eq!(
            adapter_for(Path::new("/Applications/Firefox.app")),
            Some(Adapter::Firefox)
        );
        assert_eq!(adapter_for(Path::new("random.exe")), None);
        assert_eq!(
            adapter_args(Path::new("random.exe"), Path::new("/tmp/a")).len(),
            0
        );
        let args = adapter_args(Path::new("firefox"), Path::new("/tmp/a"));
        assert_eq!(args[0], "-no-remote");
    }
    #[test]
    fn extracts_desktop_entry_without_shell() {
        let desktop = "[Desktop Entry]\nType=Application\nName=My Browser\nExec=/usr/bin/google-chrome --new-window %U\n";
        assert_eq!(
            parse_desktop_file(desktop),
            Some(("My Browser".into(), "/usr/bin/google-chrome".into()))
        );
        assert_eq!(
            first_exec_token("env VAR=1 /usr/bin/firefox %u"),
            Some("/usr/bin/firefox".into())
        );
        assert!(first_exec_token("sh -c 'echo hi'").is_none());
        assert!(first_exec_token("flatpak run org.example.App").is_none());
        assert!(parse_desktop_file("[Desktop Entry]\nHidden=true\nName=X\nExec=foo\n").is_none());
    }
}
