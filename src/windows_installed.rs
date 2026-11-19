//! Read-only Windows Apps & Features (Win32 uninstall-registry) discovery.
//! This does NOT run uninstallers, installers, or arbitrary executables.
use crate::universal::{adapter_for, DiscoveredApp, Support};
use std::{collections::HashMap, env, path::{Path, PathBuf}};
use winreg::{enums::*, RegKey};

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

fn normalize(input: &str) -> String {
    input.chars().filter(|ch| ch.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn parse_display_icon(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.starts_with('@') { return None; }
    let path = if let Some(without_quote) = raw.strip_prefix('"') {
        without_quote.split('"').next()?
    } else {
        let lowercase = raw.to_ascii_lowercase();
        raw.get(..lowercase.find(".exe")? + 4)?
    };
    path.to_lowercase().ends_with(".exe").then(|| path.to_string())
}

fn expand_windows_env(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find('%') {
        result.push_str(&remaining[..start]);
        remaining = &remaining[start + 1..];
        let Some(end) = remaining.find('%') else {
            result.push('%');
            result.push_str(remaining);
            return result;
        };
        let variable = &remaining[..end];
        if !variable.is_empty() && variable.len() < 90 {
            match env::var(variable) {
                Ok(value) => result.push_str(&value),
                Err(_) => {
                    result.push('%');
                    result.push_str(variable);
                    result.push('%');
                }
            }
        } else {
            result.push('%');
            result.push_str(variable);
            result.push('%');
        }
        remaining = &remaining[end + 1..];
    }
    result.push_str(remaining);
    result
}

fn safe_exe(path: &Path) -> bool {
    if !path.is_file() || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
        return false;
    }
    let name = path.file_stem().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    !["unins", "update", "setup", "install", "crash", "service", "helper", "broker", "pwa_launcher"]
        .iter().any(|fragment| name.contains(fragment))
}

fn locate_app(name: &str, display_icon: Option<&str>, location: Option<&str>) -> Option<PathBuf> {
    if let Some(icon) = display_icon.and_then(parse_display_icon) {
        let path = PathBuf::from(expand_windows_env(&icon));
        if safe_exe(&path) { return Some(path); }
    }
    let root = PathBuf::from(expand_windows_env(location?.trim().trim_matches('"')));
    if !root.is_dir() { return None; }
    let normalized_name = normalize(name);
    let mut best = None;
    let mut best_score = 0;
    for entry in walkdir::WalkDir::new(root).max_depth(4).follow_links(false)
        .into_iter().filter_map(|value| value.ok()).take(700)
    {
        let file = entry.path();
        if !safe_exe(file) { continue; }
        let stem = normalize(&file.file_stem()?.to_string_lossy());
        let score = if stem == normalized_name {
            3
        } else if adapter_for(file).is_some() && normalized_name.contains(&stem) {
            2
        } else {
            0
        };
        if score > best_score {
            best_score = score;
            best = Some(file.to_path_buf());
        }
    }
    best
}

fn register(results: &mut HashMap<String, DiscoveredApp>, app: DiscoveredApp) {
    let key = app.name.trim().to_lowercase();
    if key.is_empty() { return; }
    match results.get(&key) {
        Some(existing) if existing.launchable || !app.launchable => {}
        _ => { results.insert(key, app); }
    }
}

fn installed_programs(results: &mut HashMap<String, DiscoveredApp>) {
    // Both 32-bit and 64-bit registry views; both machine and current user.
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let root = RegKey::predef(hive);
            let Ok(uninstall) = root.open_subkey_with_flags(UNINSTALL, KEY_READ | view) else {
                continue;
            };
            for key_name in uninstall.enum_keys().flatten().take(2500) {
                let Ok(item) = uninstall.open_subkey_with_flags(&key_name, KEY_READ) else {
                    continue;
                };
                if item.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1
                    || item.get_value::<String, _>("ParentKeyName").is_ok()
                { continue; }
                let Ok(name) = item.get_value::<String, _>("DisplayName") else {
                    continue;
                };
                let name = name.trim();
                if name.is_empty() || name.len() > 180 { continue; }
                if let Ok(release_type) = item.get_value::<String, _>("ReleaseType") {
                    if ["Hotfix", "Security Update", "Update Rollup", "Service Pack", "Update"]
                        .iter().any(|kind| release_type.eq_ignore_ascii_case(kind))
                    { continue; }
                }
                let icon: Option<String> = item.get_value("DisplayIcon").ok();
                let location: Option<String> = item.get_value("InstallLocation").ok();
                let target = locate_app(name, icon.as_deref(), location.as_deref());
                let launchable = target.is_some();
                let path = target.unwrap_or_default();
                let support = if launchable && adapter_for(&path).is_some() {
                    Support::ProfileAdapter
                } else {
                    Support::ReviewRequired
                };
                let note = if !launchable {
                    "Registered installed app; no usable executable found. Use a manual source path."
                } else if support == Support::ProfileAdapter {
                    "Registered installed browser with known dedicated profile arguments."
                } else {
                    "Registered installed app. Profile isolation is best effort, not a sandbox."
                };
                register(results, DiscoveredApp {
                    name: name.to_string(),
                    path,
                    support,
                    note: note.to_string(),
                    origin: "Windows installed programs",
                    launchable,
                });
            }
        }
    }
}

fn known_browser_fallback(results: &mut HashMap<String, DiscoveredApp>) {
    let mut roots = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = env::var_os(var) {
            roots.push(PathBuf::from(root));
        }
    }
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(&local).join("Programs"));
        roots.push(PathBuf::from(&local).join("Google"));
        roots.push(PathBuf::from(local).join("Microsoft"));
    }
    for root in roots {
        for entry in walkdir::WalkDir::new(root).max_depth(7).follow_links(false)
            .into_iter().filter_map(|value| value.ok()).take(12000)
        {
            let file = entry.path();
            if !safe_exe(file) { continue; }
            let name = file.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
            let display = match name.as_str() {
                "chrome.exe" => "Google Chrome",
                "msedge.exe" if !file.to_string_lossy().to_ascii_lowercase().contains("edgecore") => "Microsoft Edge",
                "brave.exe" => "Brave Browser",
                "vivaldi.exe" => "Vivaldi",
                "firefox.exe" => "Mozilla Firefox",
                _ => continue,
            };
            register(results, DiscoveredApp {
                name: display.to_string(),
                path: file.to_path_buf(),
                support: Support::ProfileAdapter,
                note: "Installed browser discovered by its main executable.".into(),
                origin: "Installed browser",
                launchable: true,
            });
        }
    }
}

pub fn discover_apps() -> Vec<DiscoveredApp> {
    let mut results = HashMap::new();
    installed_programs(&mut results);
    known_browser_fallback(&mut results);
    let mut entries: Vec<_> = results.into_values().collect();
    entries.sort_by_key(|item| item.name.to_lowercase());
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_registry_strings_handle_quoted_paths_and_indices() {
        assert_eq!(
            parse_display_icon(r#""C:\Program Files\Google\Chrome\Application\chrome.exe",0"#).as_deref(),
            Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        );
        assert_eq!(
            parse_display_icon(r"C:\Program Files\Example\app.exe,1").as_deref(),
            Some(r"C:\Program Files\Example\app.exe")
        );
        assert_eq!(parse_display_icon("shell32.dll,0"), None);
    }

    #[test]
    fn installed_programs_deduplicate_by_product_name() {
        let mut results = HashMap::new();
        let missing = DiscoveredApp {
            name: "Microsoft Edge".into(), path: PathBuf::new(),
            support: Support::ReviewRequired, note: String::new(),
            origin: "Registry", launchable: false,
        };
        register(&mut results, missing);
        let main = DiscoveredApp {
            name: "Microsoft Edge".into(), path: PathBuf::from("msedge.exe"),
            support: Support::ProfileAdapter, note: String::new(),
            origin: "Browser", launchable: true,
        };
        register(&mut results, main);
        assert_eq!(results.len(), 1);
        assert!(results["microsoft edge"].launchable);
    }
}
