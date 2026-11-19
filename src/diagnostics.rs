//! Bounded local diagnostic event history.
//!
//! Every message is selected from a fixed enum: caller-supplied paths, names,
//! tokens, user content and arbitrary error strings are never serialized.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_ENTRIES: usize = 500;
pub const MAX_FILE_BYTES: u64 = 128 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}
impl Level {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Event {
    ApplicationStarted,
    RegistryLoadFailed,
    DiscoveryStarted,
    DiscoveryCompleted,
    DiscoveryFailed,
    PreflightStarted,
    PreflightCompleted,
    PreflightFailed,
    CloneStarted,
    CloneCompleted,
    CloneFailed,
    CloneCancelled,
    LaunchStarted,
    LaunchFailed,
    ProcessStopped,
    ProcessStopFailed,
    ProcessExited,
    ManagerRenameStarted,
    ManagerDuplicateStarted,
    ManagerBackupStarted,
    ManagerRestoreStarted,
    ManagerDeleteStarted,
    ManagerCompleted,
    ManagerFailed,
    LogsExported,
}
impl Event {
    pub const fn category(self) -> &'static str {
        match self {
            Self::ApplicationStarted | Self::RegistryLoadFailed | Self::LogsExported => "App",
            Self::DiscoveryStarted | Self::DiscoveryCompleted | Self::DiscoveryFailed => {
                "Discovery"
            }
            Self::PreflightStarted | Self::PreflightCompleted | Self::PreflightFailed => {
                "Preflight"
            }
            Self::CloneStarted
            | Self::CloneCompleted
            | Self::CloneFailed
            | Self::CloneCancelled => "Clone",
            Self::LaunchStarted
            | Self::LaunchFailed
            | Self::ProcessStopped
            | Self::ProcessStopFailed
            | Self::ProcessExited => "Process",
            Self::ManagerRenameStarted
            | Self::ManagerDuplicateStarted
            | Self::ManagerBackupStarted
            | Self::ManagerRestoreStarted
            | Self::ManagerDeleteStarted
            | Self::ManagerCompleted
            | Self::ManagerFailed => "Instance manager",
        }
    }

    pub const fn level(self) -> Level {
        match self {
            Self::RegistryLoadFailed
            | Self::DiscoveryFailed
            | Self::PreflightFailed
            | Self::CloneFailed
            | Self::LaunchFailed
            | Self::ProcessStopFailed
            | Self::ManagerFailed => Level::Error,
            Self::CloneCancelled => Level::Warning,
            _ => Level::Info,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::ApplicationStarted => "SoftClone session started",
            Self::RegistryLoadFailed => "Unable to read instance registry",
            Self::DiscoveryStarted => "Installed-application discovery started",
            Self::DiscoveryCompleted => "Installed-application discovery finished",
            Self::DiscoveryFailed => "Installed-application discovery worker failed",
            Self::PreflightStarted => "Clone source analysis started",
            Self::PreflightCompleted => "Clone source analysis completed",
            Self::PreflightFailed => "Clone source analysis failed",
            Self::CloneStarted => "Clone creation started",
            Self::CloneCompleted => "Clone created and registered",
            Self::CloneFailed => "Clone creation or registration failed",
            Self::CloneCancelled => "Clone cancellation requested",
            Self::LaunchStarted => "Managed application launch requested",
            Self::LaunchFailed => "Managed application launch failed",
            Self::ProcessStopped => "Managed child process stopped",
            Self::ProcessStopFailed => "Managed child process could not be stopped",
            Self::ProcessExited => "A tracked child process exited",
            Self::ManagerRenameStarted => "Instance rename requested",
            Self::ManagerDuplicateStarted => "Instance duplication requested",
            Self::ManagerBackupStarted => "Instance backup requested",
            Self::ManagerRestoreStarted => "Instance restore requested",
            Self::ManagerDeleteStarted => "Instance deletion requested",
            Self::ManagerCompleted => "Instance management operation completed",
            Self::ManagerFailed => "Instance management operation failed",
            Self::LogsExported => "Diagnostics report exported by user",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Entry {
    pub timestamp_secs: u64,
    pub event: Event,
}
impl Entry {
    pub fn level(&self) -> Level {
        self.event.level()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Info,
    Warning,
    Error,
}
impl Filter {
    pub fn matches(self, entry: &Entry) -> bool {
        match self {
            Self::All => true,
            Self::Info => entry.level() == Level::Info,
            Self::Warning => entry.level() == Level::Warning,
            Self::Error => entry.level() == Level::Error,
        }
    }
}

pub struct LogBook {
    path: PathBuf,
    entries: VecDeque<Entry>,
    last_storage_error: Option<String>,
}

impl LogBook {
    pub fn load(path: PathBuf) -> Self {
        let mut book = Self {
            path,
            entries: VecDeque::new(),
            last_storage_error: None,
        };
        if let Ok(file) = fs::File::open(&book.path) {
            if file
                .metadata()
                .is_ok_and(|meta| meta.len() > MAX_FILE_BYTES)
            {
                book.last_storage_error =
                    Some("Previous log exceeded the size limit; history was not loaded.".into());
            } else {
                for line in BufReader::new(file).lines().map_while(Result::ok) {
                    if let Ok(entry) = serde_json::from_str::<Entry>(&line) {
                        book.push_memory(entry);
                    }
                }
            }
        }
        book
    }

    fn push_memory(&mut self, entry: Entry) {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn record(&mut self, event: Event) {
        let entry = Entry {
            timestamp_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            event,
        };
        self.push_memory(entry.clone());
        if let Err(error) = self.append(&entry) {
            self.last_storage_error = Some(format!(
                "Cannot save diagnostic history ({:?}); session events remain in memory.",
                error.kind()
            ));
        }
    }

    fn append(&self, entry: &Entry) -> io::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("No diagnostics directory"))?;
        fs::create_dir_all(parent)?;
        if self
            .path
            .metadata()
            .is_ok_and(|meta| meta.len() >= MAX_FILE_BYTES)
        {
            // Only one archived copy is kept. No unbounded or verbose trace files.
            let rotated = self.path.with_extension("previous.jsonl");
            if rotated.exists() {
                fs::remove_file(&rotated)?;
            }
            fs::rename(&self.path, rotated)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, entry).map_err(io::Error::other)?;
        file.write_all(b"\n")
    }

    pub fn entries(&self) -> &VecDeque<Entry> {
        &self.entries
    }

    pub fn storage_error(&self) -> Option<&str> {
        self.last_storage_error.as_deref()
    }

    pub fn clear(&mut self) -> io::Result<()> {
        // Remove disk records before the in-memory view to avoid falsely
        // suggesting that files were cleared when deletion was denied.
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        let rotated = self.path.with_extension("previous.jsonl");
        if rotated.exists() {
            fs::remove_file(rotated)?;
        }
        self.entries.clear();
        self.last_storage_error = None;
        Ok(())
    }

    pub fn report(&self, version: &str, os: &str) -> String {
        let mut content = format!(
            "SoftClone Diagnostics\nVersion: {version}\nOS: {os}\n\
             Event-only report; no account names, file paths, tokens or app profile data.\n\n"
        );
        for entry in &self.entries {
            content.push_str(&format!(
                "{} | {} | {} | {}\n",
                entry.timestamp_secs,
                entry.level().label(),
                entry.event.category(),
                entry.event.message(),
            ));
        }
        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_log_persists_and_filters_without_raw_details() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let mut log = LogBook::load(path.clone());
        log.record(Event::DiscoveryStarted);
        log.record(Event::LaunchFailed);
        let reloaded = LogBook::load(path);
        assert_eq!(reloaded.entries().len(), 2);
        assert_eq!(
            reloaded
                .entries()
                .iter()
                .filter(|e| Filter::Error.matches(e))
                .count(),
            1
        );
        assert!(reloaded
            .report("test", "windows")
            .contains("Managed application launch failed"));
        assert!(!reloaded
            .report("test", "windows")
            .contains(dir.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn memory_is_bounded_and_clear_erases_disk_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let mut book = LogBook::load(path.clone());
        for _ in 0..MAX_ENTRIES + 20 {
            book.record(Event::DiscoveryCompleted);
        }
        assert_eq!(book.entries().len(), MAX_ENTRIES);
        book.clear().unwrap();
        assert!(!path.exists());
        assert!(LogBook::load(path).entries().is_empty());
    }

    #[test]
    fn corrupted_lines_do_not_prevent_loading_valid_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        fs::write(&path, "not json\n").unwrap();
        let mut book = LogBook::load(path);
        book.record(Event::ApplicationStarted);
        assert_eq!(book.entries().len(), 1);
    }
}
