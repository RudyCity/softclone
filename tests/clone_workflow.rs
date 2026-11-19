use softclone::{core::{self, CloneRequest}, registry};
use std::{fs, path::Path};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn make_source(dir: &Path) -> std::path::PathBuf {
    let source = dir.join("my-source");
    fs::create_dir_all(&source).unwrap();
    let filename = if cfg!(windows) { "mytool.exe" } else { "mytool" };
    let path = source.join(filename);
    fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(source.join("notes.txt"), b"portable data").unwrap();
    source
}

#[test]
fn clones_preserve_source_and_registry_roundtrips() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    let prep = core::prepare(&CloneRequest {
        source: source.clone(), name: "Work Instance".into(),
        entrypoint: String::new(), isolated_profile: true
    }).unwrap();
    let instance = core::create_clone(&prep, &sandbox.path().join("library")).unwrap();
    assert_eq!(instance.name, "Work Instance");
    assert!(instance.root.join("payload/notes.txt").is_file());
    assert_eq!(fs::read(source.join("notes.txt")).unwrap(), b"portable data");
    assert!(instance.root.join("profile/config").is_dir());
    let db = sandbox.path().join("registry.json");
    registry::store(&db, std::slice::from_ref(&instance)).unwrap();
    let records = registry::load(&db).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, instance.id);
}

#[test]
fn repeated_clones_are_distinct() {
    let sandbox = tempfile::tempdir().unwrap();
    let prep = core::prepare(&CloneRequest {
        source: make_source(sandbox.path()), name: "Tool".into(),
        entrypoint: String::new(), isolated_profile: false
    }).unwrap();
    let library = sandbox.path().join("library");
    let a = core::create_clone(&prep, &library).unwrap();
    let b = core::create_clone(&prep, &library).unwrap();
    assert_ne!(a.root, b.root);
    assert!(!a.root.join("profile").exists());
}

#[cfg(unix)]
#[test]
fn rejects_external_symlinks_without_partial_copy() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    let outside = sandbox.path().join("secret.txt");
    fs::write(&outside, "must never be copied").unwrap();
    std::os::unix::fs::symlink(&outside, source.join("outside-link")).unwrap();
    let prep = core::prepare(&CloneRequest {
        source, name: "Safe".into(), entrypoint: "mytool".into(),
        isolated_profile: true
    }).unwrap();
    let library = sandbox.path().join("library");
    assert!(core::create_clone(&prep, &library).is_err());
    assert_eq!(fs::read_dir(&library).unwrap().count(), 0);
}

#[test]
fn rejects_recursive_copy() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    let prep = core::prepare(&CloneRequest {
        source: source.clone(), name: "Loop".into(),
        entrypoint: String::new(), isolated_profile: true
    }).unwrap();
    assert!(core::create_clone(&prep, &source.join("nested")).is_err());
}

#[cfg(unix)]
#[test]
fn direct_launch_receives_profile_environment() {
    let sandbox = tempfile::tempdir().unwrap();
    let script = sandbox.path().join("runner");
    fs::write(&script, "#!/bin/sh\nprintf %s \"$XDG_CONFIG_HOME\" > profile-path.txt\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let prep = core::prepare(&CloneRequest {
        source: script, name: "Script".into(), entrypoint: String::new(),
        isolated_profile: true
    }).unwrap();
    let instance = core::create_clone(&prep, &sandbox.path().join("library")).unwrap();
    let status = core::launch(&instance).unwrap().wait().unwrap();
    assert!(status.success());
    let output = fs::read_to_string(instance.root.join("payload/profile-path.txt")).unwrap();
    assert_eq!(output, instance.root.join("profile/config").display().to_string());
}
