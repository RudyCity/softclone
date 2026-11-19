//! Managed instance lifecycle, snapshot backups and safe restoration.
//! No operation may follow a registry path outside SoftClone's own library.
use crate::{
    core::{self, CloneRecord},
    registry,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupInfo {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub name: String,
    pub created_epoch_secs: u64,
    pub copied_bytes: u64,
    pub files: u64,
}

fn invalid(message: impl Into<String>) -> core::CloneError {
    core::CloneError::Invalid(message.into())
}

fn validated_root(record: &CloneRecord, library: &Path) -> core::Result<PathBuf> {
    let library = fs::canonicalize(library)?;
    if record
        .root
        .file_name()
        .is_none_or(|name| !name.to_string_lossy().ends_with(&record.id.to_string()))
    {
        return Err(invalid("Instance folder does not match its registered ID."));
    }
    let metadata = fs::symlink_metadata(&record.root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("Instance root must be a real directory."));
    }
    let root = fs::canonicalize(&record.root)?;
    if root.parent() != Some(library.as_path()) || root == library {
        return Err(invalid("Instance root is outside the SoftClone library."));
    }
    Ok(root)
}

fn validate_member(record: &CloneRecord, registry_path: &Path) -> core::Result<()> {
    let entry = registry::load(registry_path)?
        .into_iter()
        .find(|item| item.id == record.id)
        .ok_or_else(|| invalid("Instance is no longer registered."))?;
    if entry.root != record.root {
        return Err(invalid("Registry instance path has changed."));
    }
    Ok(())
}

fn safe_copy_tree(source: &Path, destination: &Path) -> core::Result<(u64, u64)> {
    let mut bytes = 0u64;
    let mut files = 0u64;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(std::io::Error::other)?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| invalid(error.to_string()))?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_symlink() {
            let link = fs::read_link(entry.path())?;
            let resolved = fs::canonicalize(entry.path())?;
            if link.is_absolute() || !resolved.starts_with(source) {
                return Err(core::CloneError::Link(entry.path().display().to_string()));
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(link, target)?;
            #[cfg(not(unix))]
            return Err(core::CloneError::Link(entry.path().display().to_string()));
        } else if entry.file_type().is_file() {
            bytes = bytes.saturating_add(fs::copy(entry.path(), &target)?);
            files = files.saturating_add(1);
            fs::set_permissions(&target, fs::metadata(entry.path())?.permissions())?;
        } else {
            return Err(invalid(format!(
                "Unsupported special file in instance: {}",
                entry.path().display()
            )));
        }
    }
    Ok((bytes, files))
}

/// Rename changes a display label, never an on-disk folder or executable.
pub fn rename(record: &CloneRecord, name: &str, registry_path: &Path) -> core::Result<()> {
    let name = core::validate_name(name)?;
    let mut all = registry::load(registry_path)?;
    let entry = all
        .iter_mut()
        .find(|entry| entry.id == record.id && entry.root == record.root)
        .ok_or_else(|| invalid("Instance is no longer registered."))?;
    entry.name = name;
    registry::store(registry_path, &all)?;
    Ok(())
}

/// Duplicate an existing managed instance into a new independent folder.
///
/// This copies only the SoftClone-owned instance directory. The original
/// vendor installation is never copied or modified.
pub fn duplicate(
    record: &CloneRecord,
    name: &str,
    library: &Path,
    registry_path: &Path,
) -> core::Result<CloneRecord> {
    validate_member(record, registry_path)?;
    let source = validated_root(record, library)?;
    let name = core::validate_name(name)?;
    let library = fs::canonicalize(library)?;
    let id = Uuid::new_v4();
    let staging = library.join(format!(".duplicate-{id}.partial"));
    let destination = library.join(format!("duplicate-{id}"));
    fs::create_dir(&staging)?;
    let copied = safe_copy_tree(&source, &staging);
    let (bytes, _) = match copied {
        Ok(result) => result,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error.into());
    }
    let mut new_record = record.clone();
    new_record.id = id;
    new_record.name = name;
    new_record.root = destination.clone();
    new_record.created_epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    new_record.copied_bytes = if new_record.run_mode == core::RunMode::Copy {
        bytes
    } else {
        0
    };
    let added = (|| -> core::Result<()> {
        let mut items = registry::load(registry_path)?;
        items.insert(0, new_record.clone());
        registry::store(registry_path, &items)?;
        Ok(())
    })();
    if let Err(error) = added {
        if let Err(cleanup) = fs::remove_dir_all(&destination) {
            return Err(invalid(format!(
                "Failed to register duplicate: {error}. Cleanup failed at {}: {cleanup}",
                destination.display()
            )));
        }
        return Err(error);
    }
    Ok(new_record)
}

/// Create an on-disk backup under SoftClone's private backup root.
pub fn backup(
    record: &CloneRecord,
    library: &Path,
    backups: &Path,
    registry_path: &Path,
) -> core::Result<BackupInfo> {
    validate_member(record, registry_path)?;
    let source = validated_root(record, library)?;
    fs::create_dir_all(backups)?;
    let backups = fs::canonicalize(backups)?;
    if backups.starts_with(&source) || source.starts_with(&backups) {
        return Err(invalid("Backup directory overlaps instance data."));
    }
    let id = Uuid::new_v4();
    let staging = backups.join(format!(".{id}.partial"));
    let destination = backups.join(id.to_string());
    fs::create_dir(&staging)?;
    let outcome = (|| -> core::Result<BackupInfo> {
        let (copied_bytes, files) = safe_copy_tree(&source, &staging.join("data"))?;
        let info = BackupInfo {
            id,
            instance_id: record.id,
            name: record.name.clone(),
            created_epoch_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            copied_bytes,
            files,
        };
        fs::write(
            staging.join("backup.json"),
            serde_json::to_vec_pretty(&info).map_err(|error| invalid(error.to_string()))?,
        )?;
        fs::rename(&staging, destination)?;
        Ok(info)
    })();
    if outcome.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    outcome
}

fn checked_backup_path(backups: &Path, id: Uuid) -> core::Result<PathBuf> {
    let root = fs::canonicalize(backups)?;
    let folder = root.join(id.to_string());
    let meta = fs::symlink_metadata(&folder)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(invalid("Backup folder is not a real directory."));
    }
    let folder = fs::canonicalize(folder)?;
    if folder.parent() != Some(root.as_path()) {
        return Err(invalid("Backup path escapes backup library."));
    }
    Ok(folder)
}

/// Read verified backup metadata, omitting corrupt or irrelevant snapshots.
pub fn list_backups(backups: &Path, instance_id: Uuid) -> core::Result<Vec<BackupInfo>> {
    if !backups.exists() {
        return Ok(Vec::new());
    }
    let mut all = Vec::new();
    for item in fs::read_dir(backups)? {
        let item = item?;
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(id) = Uuid::parse_str(&name) else {
            continue;
        };
        let Ok(folder) = checked_backup_path(backups, id) else {
            continue;
        };
        let Ok(raw) = fs::read(folder.join("backup.json")) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<BackupInfo>(&raw) else {
            continue;
        };
        if meta.id == id && meta.instance_id == instance_id && folder.join("data").is_dir() {
            all.push(meta);
        }
    }
    all.sort_by_key(|item| std::cmp::Reverse((item.created_epoch_secs, item.id)));
    Ok(all)
}

/// Restore a selected snapshot by staging a copy then exchanging directories.
/// If the new directory cannot be committed, the previous one is restored.
pub fn restore(
    record: &CloneRecord,
    backup_id: Uuid,
    library: &Path,
    backups: &Path,
    registry_path: &Path,
) -> core::Result<()> {
    validate_member(record, registry_path)?;
    let original = validated_root(record, library)?;
    let snapshot = checked_backup_path(backups, backup_id)?;
    let meta: BackupInfo = serde_json::from_slice(&fs::read(snapshot.join("backup.json"))?)
        .map_err(|error| invalid(error.to_string()))?;
    if meta.id != backup_id || meta.instance_id != record.id {
        return Err(invalid("Backup belongs to a different instance."));
    }
    let data = snapshot.join("data");
    let data_meta = fs::symlink_metadata(&data)?;
    if data_meta.file_type().is_symlink() || !data_meta.is_dir() {
        return Err(invalid("Backup data is not a real directory."));
    }
    let library = fs::canonicalize(library)?;
    let nonce = Uuid::new_v4();
    let staging = library.join(format!(".restore-{nonce}.partial"));
    let rollback = library.join(format!(".rollback-{nonce}.partial"));
    fs::create_dir(&staging)?;
    let copied = safe_copy_tree(&data, &staging);
    if let Err(error) = copied {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = fs::rename(&original, &rollback) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&staging, &original) {
        let recovery = fs::rename(&rollback, &original);
        let _ = fs::remove_dir_all(&staging);
        return Err(match recovery {
            Ok(()) => error.into(),
            Err(restore_error) => invalid(format!(
                "Restore failed; previous data remains at {}: {restore_error}. Original error: {error}",
                rollback.display()
            )),
        });
    }
    // A completed restore is usable even if deferred old-data cleanup fails.
    fs::remove_dir_all(&rollback).map_err(|error| {
        invalid(format!(
            "Snapshot restored, but old data cleanup failed at {}: {error}",
            rollback.display()
        ))
    })
}

/// Delete a managed instance only after removing its registry entry. On
/// registry write failure, the folder rename is rolled back.
pub fn delete(record: &CloneRecord, library: &Path, registry_path: &Path) -> core::Result<()> {
    validate_member(record, registry_path)?;
    let source = validated_root(record, library)?;
    let library = fs::canonicalize(library)?;
    let trash = library.join(format!(".deleted-{}.partial", Uuid::new_v4()));
    fs::rename(&source, &trash)?;
    let result = (|| -> core::Result<()> {
        let mut all = registry::load(registry_path)?;
        let old_len = all.len();
        all.retain(|item| item.id != record.id);
        if all.len() == old_len {
            return Err(invalid("Instance is no longer registered."));
        }
        registry::store(registry_path, &all)?;
        Ok(())
    })();
    if let Err(error) = result {
        if let Err(restore_error) = fs::rename(&trash, &source) {
            return Err(invalid(format!(
                "Registry update failed: {error}. Data remains at {} and rollback failed: {restore_error}",
                trash.display()
            )));
        }
        return Err(error);
    }
    fs::remove_dir_all(&trash).map_err(|error| {
        invalid(format!(
            "Registry entry removed, but cleanup failed at {}: {error}",
            trash.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CloneRequest, RunMode};

    fn fixture() -> (tempfile::TempDir, CloneRecord, PathBuf, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("instances");
        let backups = dir.path().join("backups");
        fs::create_dir_all(&library).unwrap();
        let source = dir.path().join("app.exe");
        fs::write(&source, b"bin").unwrap();
        let id = Uuid::new_v4();
        let root = library.join(format!("sample-{id}"));
        fs::create_dir_all(root.join("payload")).unwrap();
        fs::write(root.join("payload").join("app.exe"), b"v1").unwrap();
        let record = CloneRecord {
            id,
            name: "Sample".into(),
            source: source.clone(),
            root,
            entrypoint: PathBuf::from("app.exe"),
            isolated_profile: false,
            run_mode: RunMode::Copy,
            created_epoch_secs: 0,
            copied_bytes: 2,
        };
        let db = dir.path().join("registry.json");
        registry::store(&db, std::slice::from_ref(&record)).unwrap();
        let _ = CloneRequest {
            source,
            name: "Sample".into(),
            entrypoint: String::new(),
            isolated_profile: false,
        };
        (dir, record, library, backups, db)
    }

    #[test]
    fn rename_updates_only_registry_label() {
        let (_dir, instance, _lib, _backup, db) = fixture();
        rename(&instance, "New label", &db).unwrap();
        assert_eq!(registry::load(&db).unwrap()[0].name, "New label");
        assert!(instance.root.join("payload/app.exe").exists());
        assert!(rename(&instance, "../wrong", &db).is_err());
    }

    #[test]
    fn duplicate_creates_separate_instance_and_registry_entry() {
        let (_dir, instance, library, _backups, db) = fixture();
        let second = duplicate(&instance, "Independent", &library, &db).unwrap();
        assert_ne!(instance.root, second.root);
        assert_ne!(instance.id, second.id);
        assert_eq!(second.name, "Independent");
        assert_eq!(
            fs::read(second.root.join("payload/app.exe")).unwrap(),
            b"v1"
        );
        fs::write(second.root.join("payload/app.exe"), b"v2").unwrap();
        assert_eq!(
            fs::read(instance.root.join("payload/app.exe")).unwrap(),
            b"v1"
        );
        assert_eq!(registry::load(&db).unwrap().len(), 2);
    }

    #[test]
    fn backup_and_restore_replace_only_managed_data() {
        let (_dir, instance, library, backups, db) = fixture();
        let info = backup(&instance, &library, &backups, &db).unwrap();
        assert_eq!(info.files, 1);
        assert_eq!(list_backups(&backups, instance.id).unwrap().len(), 1);
        fs::write(instance.root.join("payload/app.exe"), b"v2").unwrap();
        restore(&instance, info.id, &library, &backups, &db).unwrap();
        assert_eq!(
            fs::read(instance.root.join("payload/app.exe")).unwrap(),
            b"v1"
        );
        assert_eq!(fs::read(&instance.source).unwrap(), b"bin");
    }

    #[test]
    fn deletion_requires_managed_root_and_removes_registry_entry() {
        let (_dir, instance, library, _backups, db) = fixture();
        delete(&instance, &library, &db).unwrap();
        assert!(!instance.root.exists());
        assert!(registry::load(&db).unwrap().is_empty());
    }

    #[test]
    fn rejects_forged_root_pointing_outside_library() {
        let (_dir, mut instance, library, backups, db) = fixture();
        let external = library
            .parent()
            .unwrap()
            .join(format!("other-{}", instance.id));
        fs::create_dir(&external).unwrap();
        instance.root = external.clone();
        assert!(backup(&instance, &library, &backups, &db).is_err());
        assert!(delete(&instance, &library, &db).is_err());
        assert!(external.exists());
    }

    #[test]
    fn restore_rejects_another_instances_backup() {
        let (_dir, instance, library, backups, db) = fixture();
        let info = backup(&instance, &library, &backups, &db).unwrap();
        let mut other = instance.clone();
        other.id = Uuid::new_v4();
        assert!(restore(&other, info.id, &library, &backups, &db).is_err());
    }
}
