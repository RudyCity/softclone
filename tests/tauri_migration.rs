#[test]
fn migration_has_native_tauri_shell_and_no_egui_dependency() {
    let manifest = include_str!("../Cargo.toml");
    let app = include_str!("../src-tauri/Cargo.toml");
    let config = include_str!("../src-tauri/tauri.conf.json");
    let ui = include_str!("../frontend/index.html");
    let shell = include_str!("../frontend/app.js");
    assert!(!manifest.contains("eframe"));
    assert!(!manifest.contains("egui"));
    assert!(app.contains("tauri ="));
    assert!(config.contains("withGlobalTauri"));
    assert!(ui.contains("app.js"));
    for command in [
        "create_instance",
        "list_instances",
        "preflight",
        "cancel_clone",
        "launch_instance",
        "stop_instance",
        "discover_apps",
        "backup_instance",
        "restore_instance",
        "delete_instance",
        "get_logs",
        "clear_logs",
        "export_logs",
    ] {
        assert!(
            shell.contains(command),
            "Missing desktop command: {command}"
        );
    }
}

#[cfg(target_os = "windows")]
#[test]
fn windows_discovery_does_not_spawn_visible_terminal() {
    let store = include_str!("../src/windows_store.rs");
    let host = include_str!("../src-tauri/src/main.rs");
    assert!(store.contains("creation_flags(CREATE_NO_WINDOW)"));
    assert!(store.contains("Stdio::null()"));
    assert!(store.contains("\"-WindowStyle\""));
    assert!(host.contains("cfg_attr(target_os = \"windows\", windows_subsystem = \"windows\")"));
}
