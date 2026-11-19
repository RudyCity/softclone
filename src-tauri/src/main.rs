//! Tauri 2 desktop host. The existing Rust library remains the sole owner of clone files,
//! registry validation, supported app discovery and launch policy.
use serde::{Deserialize, Serialize};
use softclone::{
    core::{self, CloneRecord, CloneRequest, CopyProgress},
    diagnostics::{Entry, Event, LogBook},
    instance_manager::{self, BackupInfo},
    registry, rune_engine, universal,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
};
use tauri::State;
use uuid::Uuid;

type DesktopResult<T> = Result<T, String>;

struct Inner {
    mutation: Mutex<()>,
    children: Mutex<HashMap<Uuid, Child>>,
    cancellation: Mutex<Option<Arc<AtomicBool>>>,
    progress: Mutex<Option<ProgressView>>,
    logs: Mutex<LogBook>,
}

#[derive(Clone)]
struct Desktop(Arc<Inner>);

fn guard<T>(mutex: &Mutex<T>) -> DesktopResult<MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| "Internal state lock was poisoned.".to_string())
}

impl Desktop {
    fn new() -> Self {
        let mut logs = LogBook::load(registry::app_data_dir().join("diagnostics.jsonl"));
        logs.record(Event::ApplicationStarted);
        Self(Arc::new(Inner {
            mutation: Mutex::new(()),
            children: Mutex::new(HashMap::new()),
            cancellation: Mutex::new(None),
            progress: Mutex::new(None),
            logs: Mutex::new(logs),
        }))
    }

    fn log(&self, event: Event) {
        if let Ok(mut logs) = self.0.logs.lock() {
            logs.record(event);
        }
    }
}

async fn blocking<T, F>(job: F) -> DesktopResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> DesktopResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|error| format!("Background operation failed: {error}"))?
}

fn record(id: Uuid) -> DesktopResult<CloneRecord> {
    registry::load(&registry::registry_file())
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "This clone is no longer in your library.".to_string())
}

/// Only children spawned by this SoftClone session can be stopped.
fn is_running(children: &mut HashMap<Uuid, Child>, id: Uuid) -> DesktopResult<bool> {
    let running = match children.get_mut(&id) {
        Some(child) => child
            .try_wait()
            .map(|status| status.is_none())
            .map_err(|error| error.to_string())?,
        None => false,
    };
    if !running {
        children.remove(&id);
    }
    Ok(running)
}

fn must_be_stopped(inner: &Inner, id: Uuid) -> DesktopResult<()> {
    if is_running(&mut guard(&inner.children)?, id)? {
        Err("Stop this managed process before changing its files.".into())
    } else {
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceView {
    #[serde(flatten)]
    item: CloneRecord,
    running: bool,
    pid: Option<u32>,
}

#[tauri::command]
async fn list_instances(state: State<'_, Desktop>) -> DesktopResult<Vec<InstanceView>> {
    let app = state.inner().clone();
    blocking(move || {
        let records =
            registry::load(&registry::registry_file()).map_err(|error| error.to_string())?;
        let mut children = guard(&app.0.children)?;
        records
            .into_iter()
            .map(|item| {
                let running = is_running(&mut children, item.id)?;
                let pid = if running {
                    children.get(&item.id).map(Child::id)
                } else {
                    None
                };
                Ok(InstanceView { item, running, pid })
            })
            .collect()
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveredView {
    name: String,
    path: String,
    support: String,
    note: String,
    origin: String,
    launchable: bool,
    publisher: Option<String>,
    version: Option<String>,
    store_id: Option<String>,
}

#[tauri::command]
async fn discover_apps(state: State<'_, Desktop>) -> DesktopResult<Vec<DiscoveredView>> {
    let app = state.inner().clone();
    blocking(move || {
        app.log(Event::DiscoveryStarted);
        let items = universal::discover_apps()
            .into_iter()
            .map(|item| DiscoveredView {
                name: item.name,
                path: item.path.to_string_lossy().into_owned(),
                support: item.support.label().to_string(),
                note: item.note,
                origin: item.origin.to_string(),
                launchable: item.launchable,
                publisher: item.publisher,
                version: item.version,
                store_id: item.store_id,
            })
            .collect();
        app.log(Event::DiscoveryCompleted);
        Ok(items)
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInput {
    source: String,
    name: String,
    #[serde(default)]
    entrypoint: String,
    isolated_profile: bool,
    profile_only: bool,
}

fn prepared(input: &CreateInput) -> DesktopResult<core::Prepared> {
    core::prepare(&CloneRequest {
        source: PathBuf::from(&input.source),
        name: input.name.clone(),
        entrypoint: input.entrypoint.clone(),
        isolated_profile: input.isolated_profile || input.profile_only,
    })
    .map_err(|error| error.to_string())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressView {
    copied_bytes: u64,
    total_bytes: u64,
    completed_files: u64,
    total_files: u64,
    current_file: String,
}

impl From<CopyProgress> for ProgressView {
    fn from(value: CopyProgress) -> Self {
        Self {
            copied_bytes: value.copied_bytes,
            total_bytes: value.total_bytes,
            completed_files: value.completed_files,
            total_files: value.total_files,
            current_file: value.current_file,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreflightView {
    total_bytes: u64,
    total_files: u64,
}

#[tauri::command]
async fn preflight(state: State<'_, Desktop>, input: CreateInput) -> DesktopResult<PreflightView> {
    let app = state.inner().clone();
    blocking(move || {
        app.log(Event::PreflightStarted);
        let result = (|| {
            let info = prepared(&input)?;
            if input.profile_only {
                return Ok(PreflightView {
                    total_bytes: 0,
                    total_files: 0,
                });
            }
            let plan = core::analyze(&info).map_err(|error| error.to_string())?;
            Ok(PreflightView {
                total_bytes: plan.total_bytes,
                total_files: plan.total_files,
            })
        })();
        app.log(if result.is_ok() {
            Event::PreflightCompleted
        } else {
            Event::PreflightFailed
        });
        result
    })
    .await
}

#[tauri::command]
async fn clone_progress(state: State<'_, Desktop>) -> DesktopResult<Option<ProgressView>> {
    Ok(guard(&state.0.progress)?.clone())
}

#[tauri::command]
fn cancel_clone(state: State<'_, Desktop>) -> DesktopResult<()> {
    let cancellation = guard(&state.0.cancellation)?;
    let token = cancellation
        .as_ref()
        .ok_or_else(|| "There is no active clone operation.".to_string())?;
    token.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
async fn create_instance(
    state: State<'_, Desktop>,
    input: CreateInput,
) -> DesktopResult<CloneRecord> {
    let app = state.inner().clone();
    blocking(move || {
        let _exclusive = guard(&app.0.mutation)?;
        let token = Arc::new(AtomicBool::new(false));
        {
            let mut active = guard(&app.0.cancellation)?;
            if active.is_some() {
                return Err("Another clone is already in progress.".into());
            }
            *active = Some(token.clone());
        }
        *guard(&app.0.progress)? = None;
        app.log(Event::CloneStarted);
        let result: DesktopResult<CloneRecord> = (|| {
            let info = prepared(&input)?;
            let item = if input.profile_only {
                core::create_profile_instance(&info, &registry::library_dir())
            } else {
                core::create_clone_with_progress(
                    &info,
                    &registry::library_dir(),
                    &token,
                    |progress| {
                        if let Ok(mut current) = app.0.progress.lock() {
                            *current = Some(progress.into());
                        }
                    },
                )
            }
            .map_err(|error| error.to_string())?;
            if let Err(error) = registry::add(item.clone()) {
                let cleanup = std::fs::remove_dir_all(&item.root);
                return Err(match cleanup {
                    Ok(()) => format!("Cannot save clone metadata: {error}"),
                    Err(other) => format!(
                        "Cannot save clone metadata: {error}. Incomplete data remains at {}: {other}",
                        item.root.display()
                    ),
                });
            }
            Ok(item)
        })();
        if let Ok(mut active) = app.0.cancellation.lock() {
            *active = None;
        }
        if let Ok(mut current) = app.0.progress.lock() {
            *current = None;
        }
        let event = match &result {
            Ok(_) => Event::CloneCompleted,
            Err(_) if token.load(Ordering::Relaxed) => Event::CloneCancelled,
            Err(_) => Event::CloneFailed,
        };
        app.log(event);
        result
    })
    .await
}

#[tauri::command]
async fn launch_instance(state: State<'_, Desktop>, id: Uuid) -> DesktopResult<u32> {
    let app = state.inner().clone();
    blocking(move || {
        let _exclusive = guard(&app.0.mutation)?;
        let item = record(id)?;
        {
            let mut children = guard(&app.0.children)?;
            if is_running(&mut children, id)? {
                return Err("This clone is already running in this session.".into());
            }
        }
        app.log(Event::LaunchStarted);
        match core::launch(&item) {
            Ok(child) => {
                let pid = child.id();
                guard(&app.0.children)?.insert(id, child);
                Ok(pid)
            }
            Err(error) => {
                app.log(Event::LaunchFailed);
                Err(error.to_string())
            }
        }
    })
    .await
}

#[tauri::command]
async fn stop_instance(state: State<'_, Desktop>, id: Uuid) -> DesktopResult<()> {
    let app = state.inner().clone();
    blocking(move || {
        let _exclusive = guard(&app.0.mutation)?;
        record(id)?;
        let mut children = guard(&app.0.children)?;
        if !is_running(&mut children, id)? {
            return Err("This clone has no managed running process.".into());
        }
        let child = children
            .get_mut(&id)
            .ok_or_else(|| "Managed process is no longer available.".to_string())?;
        if let Err(error) = child.kill() {
            app.log(Event::ProcessStopFailed);
            return Err(format!("Cannot stop managed process: {error}"));
        }
        let wait_result = child.wait().map_err(|error| error.to_string());
        if wait_result.is_ok() {
            children.remove(&id);
            app.log(Event::ProcessStopped);
        } else {
            app.log(Event::ProcessStopFailed);
        }
        wait_result.map(|_| ())
    })
    .await
}

#[tauri::command]
async fn rename_instance(state: State<'_, Desktop>, id: Uuid, name: String) -> DesktopResult<()> {
    let app = state.inner().clone();
    blocking(move || {
        let _exclusive = guard(&app.0.mutation)?;
        let item = record(id)?;
        app.log(Event::ManagerRenameStarted);
        let result = instance_manager::rename(&item, &name, &registry::registry_file())
            .map_err(|error| error.to_string());
        app.log(if result.is_ok() {
            Event::ManagerCompleted
        } else {
            Event::ManagerFailed
        });
        result
    })
    .await
}

#[tauri::command]
async fn duplicate_instance(
    state: State<'_, Desktop>,
    id: Uuid,
    name: String,
) -> DesktopResult<CloneRecord> {
    let app = state.inner().clone();
    blocking(move || {
        let _exclusive = guard(&app.0.mutation)?;
        let item = record(id)?;
        must_be_stopped(&app.0, id)?;
        app.log(Event::ManagerDuplicateStarted);
        let result = instance_manager::duplicate(
            &item,
            &name,
            &registry::library_dir(),
            &registry::registry_file(),
        )
        .map_err(|error| error.to_string());
        app.log(if result.is_ok() {
            Event::ManagerCompleted
        } else {
            Event::ManagerFailed
        });
        result
    })
    .await
}

#[tauri::command]
async fn backup_instance(state: State<'_, Desktop>, id: Uuid) -> DesktopResult<BackupInfo> {
    let app = state.inner().clone();
    blocking(move || {
        let _exclusive = guard(&app.0.mutation)?;
        let item = record(id)?;
        must_be_stopped(&app.0, id)?;
        app.log(Event::ManagerBackupStarted);
        let result = instance_manager::backup(
            &item,
            &registry::library_dir(),
            &registry::backups_dir(),
            &registry::registry_file(),
        )
        .map_err(|error| error.to_string());
        app.log(if result.is_ok() {
            Event::ManagerCompleted
        } else {
            Event::ManagerFailed
        });
        result
    })
    .await
}

#[tauri::command]
async fn list_backups(state: State<'_, Desktop>, id: Uuid) -> DesktopResult<Vec<BackupInfo>> {
    let app = state.inner().clone();
    blocking(move || {
        record(id)?;
        instance_manager::list_backups(&registry::backups_dir(), id)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn restore_instance(
    state: State<'_, Desktop>,
    id: Uuid,
    backup_id: Uuid,
) -> DesktopResult<()> {
    let app = state.inner().clone();
    blocking(move || {
        let _exclusive = guard(&app.0.mutation)?;
        let item = record(id)?;
        must_be_stopped(&app.0, id)?;
        app.log(Event::ManagerRestoreStarted);
        let result = instance_manager::restore(
            &item,
            backup_id,
            &registry::library_dir(),
            &registry::backups_dir(),
            &registry::registry_file(),
        )
        .map_err(|error| error.to_string());
        app.log(if result.is_ok() {
            Event::ManagerCompleted
        } else {
            Event::ManagerFailed
        });
        result
    })
    .await
}

#[tauri::command]
async fn delete_instance(state: State<'_, Desktop>, id: Uuid) -> DesktopResult<()> {
    let app = state.inner().clone();
    blocking(move || {
        let _exclusive = guard(&app.0.mutation)?;
        let item = record(id)?;
        must_be_stopped(&app.0, id)?;
        app.log(Event::ManagerDeleteStarted);
        let result =
            instance_manager::delete(&item, &registry::library_dir(), &registry::registry_file())
                .map_err(|error| error.to_string());
        app.log(if result.is_ok() {
            Event::ManagerCompleted
        } else {
            Event::ManagerFailed
        });
        result
    })
    .await
}

#[tauri::command]
async fn open_instance_folder(state: State<'_, Desktop>, id: Uuid) -> DesktopResult<()> {
    let app = state.inner().clone();
    blocking(move || {
        let item = record(id)?;
        // The path comes from the validated registry, never from the webview.
        core::open_folder(&item.root).map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn open_data_folder() -> DesktopResult<()> {
    blocking(move || {
        std::fs::create_dir_all(registry::app_data_dir()).map_err(|error| error.to_string())?;
        core::open_folder(&registry::app_data_dir()).map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn pick_source(kind: String) -> DesktopResult<Option<String>> {
    blocking(move || {
        let dialog = rfd::FileDialog::new();
        let selected = match kind.as_str() {
            "file" => dialog.pick_file(),
            "folder" => dialog.pick_folder(),
            _ => return Err("Unknown source picker type.".into()),
        };
        Ok(selected.map(|path| path.to_string_lossy().into_owned()))
    })
    .await
}

#[tauri::command]
async fn open_store_app(aumid: String) -> DesktopResult<()> {
    blocking(move || {
        #[cfg(target_os = "windows")]
        {
            softclone::windows_store::open_app(&aumid)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = aumid;
            Err("Windows packaged app activation is only available on Windows.".into())
        }
    })
    .await
}

#[tauri::command]
async fn suggest_label(base: String, index: i64) -> DesktopResult<String> {
    blocking(move || rune_engine::clone_label(&base, index)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogsView {
    entries: Vec<Entry>,
    storage_error: Option<String>,
}

#[tauri::command]
fn get_logs(state: State<'_, Desktop>) -> DesktopResult<LogsView> {
    let logs = guard(&state.0.logs)?;
    Ok(LogsView {
        entries: logs.entries().iter().cloned().collect(),
        storage_error: logs.storage_error().map(str::to_owned),
    })
}

#[tauri::command]
fn clear_logs(state: State<'_, Desktop>) -> DesktopResult<()> {
    guard(&state.0.logs)?
        .clear()
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn export_logs(state: State<'_, Desktop>) -> DesktopResult<Option<String>> {
    let app = state.inner().clone();
    blocking(move || {
        let report = guard(&app.0.logs)?.report(env!("CARGO_PKG_VERSION"), std::env::consts::OS);
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("softclone-diagnostics.txt")
            .save_file()
        else {
            return Ok(None);
        };
        std::fs::write(&path, report).map_err(|error| error.to_string())?;
        app.log(Event::LogsExported);
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
}

/// Only fixed project-owned URLs may leave the webview.
#[tauri::command]
async fn open_external(target: String) -> DesktopResult<()> {
    blocking(move || {
        let url = match target.as_str() {
            "source" => "https://github.com/RudyCity/softclone",
            "support" => "https://ko-fi.com/rudycity",
            _ => return Err("External URL is not in SoftClone's allowlist.".into()),
        };
        #[cfg(target_os = "windows")]
        let launch = Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn();
        #[cfg(target_os = "macos")]
        let launch = Command::new("open").arg(url).spawn();
        #[cfg(target_os = "linux")]
        let launch = Command::new("xdg-open").arg(url).spawn();
        launch.map(|_| ()).map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "license": "Apache-2.0",
        "runtime": "Tauri 2",
    })
}

#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    tauri::Builder::default()
        .manage(Desktop::new())
        .invoke_handler(tauri::generate_handler![
            app_info,
            list_instances,
            discover_apps,
            preflight,
            clone_progress,
            cancel_clone,
            create_instance,
            launch_instance,
            stop_instance,
            rename_instance,
            duplicate_instance,
            backup_instance,
            list_backups,
            restore_instance,
            delete_instance,
            open_instance_folder,
            open_data_folder,
            pick_source,
            open_store_app,
            suggest_label,
            get_logs,
            clear_logs,
            export_logs,
            open_external,
        ])
        .run(tauri::generate_context!())
        .expect("Could not start SoftClone desktop");
}

