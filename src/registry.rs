use crate::core::CloneRecord;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("softclone-data"))
        .join("SoftClone")
}
pub fn library_dir() -> PathBuf {
    app_data_dir().join("instances")
}
pub fn registry_file() -> PathBuf {
    app_data_dir().join("instances.json")
}

pub fn load(path: &Path) -> io::Result<Vec<CloneRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)
}

pub fn store(path: &Path, instances: &[CloneRecord]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Registry has no parent"))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".registry-{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(instances).map_err(io::Error::other)?;
    let result = (|| -> io::Result<()> {
        fs::write(&tmp, bytes)?;
        #[cfg(windows)]
        {
            if path.exists() {
                let backup = parent.join(format!(".registry-{}.bak", Uuid::new_v4()));
                fs::rename(path, &backup)?;
                match fs::rename(&tmp, path) {
                    Ok(()) => {
                        let _ = fs::remove_file(backup);
                    }
                    Err(error) => {
                        let _ = fs::rename(backup, path);
                        return Err(error);
                    }
                }
            } else {
                fs::rename(&tmp, path)?;
            }
        }
        #[cfg(not(windows))]
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

pub fn add(record: CloneRecord) -> io::Result<()> {
    let path = registry_file();
    let mut all = load(&path)?;
    all.insert(0, record);
    store(&path, &all)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_registry_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(&dir.path().join("nothing.json")).unwrap().is_empty());
    }
    #[test]
    fn broken_registry_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.json");
        fs::write(&file, "garbled").unwrap();
        assert!(load(&file).is_err());
    }
}
