use softclone::{
    core::{self, CloneRequest},
    registry,
};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

fn make_source(dir: &Path) -> std::path::PathBuf {
    let source = dir.join("my-source");
    fs::create_dir_all(&source).unwrap();
    let filename = if cfg!(windows) {
        "mytool.exe"
    } else {
        "mytool"
    };
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
        source: source.clone(),
        name: "Work Instance".into(),
        entrypoint: String::new(),
        isolated_profile: true,
    })
    .unwrap();
    let instance = core::create_clone(&prep, &sandbox.path().join("library")).unwrap();
    assert_eq!(instance.name, "Work Instance");
    assert!(instance.root.join("payload/notes.txt").is_file());
    assert_eq!(
        fs::read(source.join("notes.txt")).unwrap(),
        b"portable data"
    );
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
        source: make_source(sandbox.path()),
        name: "Tool".into(),
        entrypoint: String::new(),
        isolated_profile: false,
    })
    .unwrap();
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
        source,
        name: "Safe".into(),
        entrypoint: "mytool".into(),
        isolated_profile: true,
    })
    .unwrap();
    let library = sandbox.path().join("library");
    assert!(core::create_clone(&prep, &library).is_err());
    assert_eq!(fs::read_dir(&library).unwrap().count(), 0);
}

#[test]
fn rejects_recursive_copy() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    let prep = core::prepare(&CloneRequest {
        source: source.clone(),
        name: "Loop".into(),
        entrypoint: String::new(),
        isolated_profile: true,
    })
    .unwrap();
    assert!(core::create_clone(&prep, &source.join("nested")).is_err());
    assert!(!source.join("nested").exists());
}

#[cfg(unix)]
#[test]
fn preserves_relative_internal_symlinks() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    std::os::unix::fs::symlink("notes.txt", source.join("notes-link")).unwrap();
    let prepared = core::prepare(&CloneRequest {
        source,
        name: "Linked".into(),
        entrypoint: "mytool".into(),
        isolated_profile: false,
    })
    .unwrap();
    let clone = core::create_clone(&prepared, &sandbox.path().join("library")).unwrap();
    assert_eq!(
        fs::read_link(clone.root.join("payload/notes-link")).unwrap(),
        std::path::PathBuf::from("notes.txt")
    );
}

#[cfg(unix)]
#[test]
fn direct_launch_receives_profile_environment() {
    let sandbox = tempfile::tempdir().unwrap();
    let script = sandbox.path().join("runner");
    fs::write(
        &script,
        "#!/bin/sh\nprintf %s \"$XDG_CONFIG_HOME\" > profile-path.txt\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let prep = core::prepare(&CloneRequest {
        source: script,
        name: "Script".into(),
        entrypoint: String::new(),
        isolated_profile: true,
    })
    .unwrap();
    let instance = core::create_clone(&prep, &sandbox.path().join("library")).unwrap();
    let status = core::launch(&instance).unwrap().wait().unwrap();
    assert!(status.success());
    let output = fs::read_to_string(instance.root.join("payload/profile-path.txt")).unwrap();
    assert_eq!(
        output,
        instance.root.join("profile/config").display().to_string()
    );
}

#[test]
fn profile_only_instance_does_not_copy_executable() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    let prepared = core::prepare(&CloneRequest {
        source: source.clone(),
        name: "Installed profile".into(),
        entrypoint: String::new(),
        isolated_profile: true,
    })
    .unwrap();
    let record =
        core::create_profile_instance(&prepared, &sandbox.path().join("instances")).unwrap();
    assert_eq!(record.run_mode, core::RunMode::ProfileOnly);
    assert!(!record.root.join("payload").exists());
    assert!(record.root.join("profile/browser").is_dir());
    assert!(source.join("notes.txt").exists());

    let path = sandbox.path().join("reg.json");
    registry::store(&path, std::slice::from_ref(&record)).unwrap();
    assert_eq!(
        registry::load(&path).unwrap()[0].run_mode,
        core::RunMode::ProfileOnly
    );
}

#[test]
fn legacy_registry_defaults_to_copy_mode() {
    let json = r#"{
        "id":"aeaa924f-f8c4-47c0-8567-a08675f62aee",
        "name":"Old clone",
        "source":"/old",
        "root":"/clone",
        "entrypoint":"app",
        "isolated_profile":false,
        "created_epoch_secs":0,
        "copied_bytes":24
    }"#;
    let record: core::CloneRecord = serde_json::from_str(json).unwrap();
    assert_eq!(record.run_mode, core::RunMode::Copy);
}

#[test]
fn preflight_estimates_source_without_creating_destination() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    fs::write(source.join("extra.txt"), vec![11u8; 2048]).unwrap();
    let prepared = core::prepare(&CloneRequest {
        source: source.clone(),
        name: "Preview".into(),
        entrypoint: String::new(),
        isolated_profile: true,
    })
    .unwrap();
    let before = fs::metadata(source.join("extra.txt")).unwrap().len();
    let plan = core::analyze(&prepared).unwrap();
    assert_eq!(plan.total_files, 3);
    assert!(plan.total_bytes >= before + 13);
    assert!(!sandbox.path().join("destination").exists());
}

#[test]
fn cancellation_cleans_partial_directory_without_touching_source() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    let big_file = source.join("large-payload.dat");
    fs::write(&big_file, vec![0xAB; 6 * 1024 * 1024]).unwrap();
    let prepared = core::prepare(&CloneRequest {
        source: source.clone(),
        name: "Cancelable".into(),
        entrypoint: String::new(),
        isolated_profile: true,
    })
    .unwrap();
    let library = sandbox.path().join("library");
    let cancel = AtomicBool::new(false);
    let mut callbacks = 0usize;
    let result = core::create_clone_with_progress(&prepared, &library, &cancel, |progress| {
        callbacks += 1;
        assert!(progress.total_files >= 3);
        cancel.store(true, Ordering::SeqCst);
    });
    assert!(matches!(result, Err(core::CloneError::Cancelled)));
    assert!(callbacks > 0);
    assert!(big_file.is_file());
    assert_eq!(fs::read_dir(library).unwrap().count(), 0);
}

#[test]
fn starting_in_cancelled_state_does_not_create_library() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    let prepared = core::prepare(&CloneRequest {
        source,
        name: "Precancel".into(),
        entrypoint: String::new(),
        isolated_profile: false,
    })
    .unwrap();
    let library = sandbox.path().join("library");
    let cancelled = AtomicBool::new(true);
    let result = core::create_clone_with_progress(&prepared, &library, &cancelled, |_| {});
    assert!(matches!(result, Err(core::CloneError::Cancelled)));
    assert!(!library.exists());
}

#[cfg(unix)]
#[test]
fn preflight_rejects_external_symlinks_without_destination_changes() {
    let sandbox = tempfile::tempdir().unwrap();
    let source = make_source(sandbox.path());
    let outside = sandbox.path().join("private");
    fs::write(&outside, b"no").unwrap();
    std::os::unix::fs::symlink(&outside, source.join("external")).unwrap();
    let prepared = core::prepare(&CloneRequest {
        source,
        name: "Reject links".into(),
        entrypoint: "mytool".into(),
        isolated_profile: false,
    })
    .unwrap();
    assert!(matches!(
        core::analyze(&prepared),
        Err(core::CloneError::Link(_))
    ));
}
