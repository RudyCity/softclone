use crate::components::{
    create_form::CreateForm,
    discovery_form,
    instance_card::{self, Action as InstanceAction},
};
use crate::icons::{self, Icon};
use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Layout, RichText, Stroke, Vec2};
use softclone::{
    core::{self, CloneRecord, CloneRequest, CopyPlan, CopyProgress},
    instance_manager::{self, BackupInfo},
    registry,
    universal::{self, AppFilter, DiscoveredApp, Support},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    time::Duration,
};
use uuid::Uuid;

const INK: Color32 = Color32::from_rgb(24, 34, 43);
const MUTED: Color32 = Color32::from_rgb(112, 123, 132);
const ACCENT: Color32 = Color32::from_rgb(40, 105, 96);
const SURFACE: Color32 = Color32::from_rgb(248, 249, 247);
const LINE: Color32 = Color32::from_rgb(221, 228, 225);
const NAV: Color32 = Color32::from_rgb(22, 38, 42);
const WHITE: Color32 = Color32::WHITE;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Library,
    Create,
    Discover,
    About,
}

#[cfg(target_os = "windows")]
struct IconPixels {
    path: PathBuf,
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

enum CloneEvent {
    Progress(CopyProgress),
    Finished(Result<CloneRecord, String>),
}

#[derive(Clone)]
enum PendingManager {
    Rename(Uuid),
    Duplicate(Uuid),
    Restore(Uuid),
    Delete(Uuid),
}

enum ManagerWork {
    Rename(String),
    Duplicate(String),
    Backup,
    Restore(Uuid),
    Delete,
}

pub struct DesktopApp {
    page: Page,
    items: Vec<CloneRecord>,
    source: String,
    name: String,
    entrypoint: String,
    isolated: bool,
    profile_only: bool,
    discovered: Vec<DiscoveredApp>,
    discover_query: String,
    discover_filter: AppFilter,
    discover_worker: Option<Receiver<Vec<DiscoveredApp>>>,
    #[cfg(target_os = "windows")]
    icon_worker: Option<Receiver<IconPixels>>,
    #[cfg(target_os = "windows")]
    app_icons: std::collections::HashMap<PathBuf, egui::TextureHandle>,
    discovery_complete: bool,
    worker: Option<Receiver<CloneEvent>>,
    cancel_token: Option<Arc<AtomicBool>>,
    cancellation_requested: bool,
    copy_progress: Option<CopyProgress>,
    analysis_worker: Option<Receiver<(String, Result<CopyPlan, String>)>>,
    analysis: Option<(String, CopyPlan)>,
    notice: Option<(String, bool)>,
    processes: HashMap<Uuid, Child>,
    manager_worker: Option<Receiver<Result<String, String>>>,
    manager_busy: Option<(Uuid, String)>,
    pending_manager: Option<PendingManager>,
    manager_name: String,
    confirm_name: String,
    restore_options: Vec<BackupInfo>,
    restore_selected: Option<Uuid>,
}

impl DesktopApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(16.0, 10.0);
        style.visuals = egui::Visuals::light();
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
        style.visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
        style.visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
        style.visuals.widgets.active.corner_radius = CornerRadius::same(8);
        cc.egui_ctx.set_style(style);
        let (items, notice) = match registry::load(&registry::registry_file()) {
            Ok(items) => (items, None),
            Err(e) => (
                vec![],
                Some((format!("Cannot read clone registry: {e}"), true)),
            ),
        };
        Self {
            page: Page::Library,
            items,
            source: String::new(),
            name: String::new(),
            entrypoint: String::new(),
            isolated: true,
            profile_only: false,
            discovered: Vec::new(),
            discover_query: String::new(),
            discover_filter: AppFilter::All,
            discover_worker: None,
            #[cfg(target_os = "windows")]
            icon_worker: None,
            #[cfg(target_os = "windows")]
            app_icons: std::collections::HashMap::new(),
            discovery_complete: false,
            worker: None,
            cancel_token: None,
            cancellation_requested: false,
            copy_progress: None,
            analysis_worker: None,
            analysis: None,
            notice,
            processes: HashMap::new(),
            manager_worker: None,
            manager_busy: None,
            pending_manager: None,
            manager_name: String::new(),
            confirm_name: String::new(),
            restore_options: Vec::new(),
            restore_selected: None,
        }
    }

    fn analysis_key(&self) -> String {
        format!(
            "{}\u{0}{}\u{0}{}\u{0}{}",
            self.source, self.name, self.entrypoint, self.profile_only
        )
    }

    fn start_analysis(&mut self) {
        if self.analysis_worker.is_some() || self.worker.is_some() {
            return;
        }
        let key = self.analysis_key();
        let request = CloneRequest {
            source: PathBuf::from(self.source.trim()),
            name: self.name.clone(),
            entrypoint: self.entrypoint.clone(),
            isolated_profile: self.isolated || self.profile_only,
        };
        let profile_only = self.profile_only;
        let (sender, receiver) = mpsc::channel();
        self.analysis_worker = Some(receiver);
        self.analysis = None;
        std::thread::spawn(move || {
            let result = core::prepare(&request)
                .and_then(|prepared| {
                    if profile_only {
                        Ok(CopyPlan::default())
                    } else {
                        core::analyze(&prepared)
                    }
                })
                .map_err(|err| err.to_string());
            let _ = sender.send((key, result));
        });
    }

    fn tick_analysis(&mut self, ctx: &egui::Context) {
        let result = self
            .analysis_worker
            .as_ref()
            .map(|worker| worker.try_recv());
        match result {
            Some(Ok((key, outcome))) => {
                if key == self.analysis_key() {
                    match outcome {
                        Ok(plan) => self.analysis = Some((key, plan)),
                        Err(error) => {
                            self.notice = Some((format!("Preflight failed: {error}"), true))
                        }
                    }
                }
                self.analysis_worker = None;
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.notice = Some(("Preflight worker stopped unexpectedly.".into(), true));
                self.analysis_worker = None;
            }
            Some(Err(mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(130));
            }
            None => {}
        }
    }

    fn tick_worker(&mut self, ctx: &egui::Context) {
        let mut finished = None;
        if let Some(worker) = &self.worker {
            loop {
                match worker.try_recv() {
                    Ok(CloneEvent::Progress(progress)) => self.copy_progress = Some(progress),
                    Ok(CloneEvent::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = Some(Err("Clone worker stopped unexpectedly.".into()));
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        ctx.request_repaint_after(Duration::from_millis(120));
                        break;
                    }
                }
            }
        }
        if let Some(result) = finished {
            self.worker = None;
            self.cancel_token = None;
            self.cancellation_requested = false;
            self.copy_progress = None;
            match result {
                Ok(item) => {
                    self.items.insert(0, item);
                    self.notice = Some(("Clone created. You can now launch it.".into(), false));
                    self.page = Page::Library;
                    self.source.clear();
                    self.name.clear();
                    self.entrypoint.clear();
                    self.profile_only = false;
                    self.analysis = None;
                }
                Err(error) => self.notice = Some((error, true)),
            }
        }
    }

    fn headline(ui: &mut egui::Ui, eyebrow: &str, title: &str, description: &str) {
        ui.label(
            RichText::new(eyebrow.to_uppercase())
                .color(ACCENT)
                .size(11.0)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(title).color(INK).size(31.0).strong());
        ui.add_space(3.0);
        ui.label(RichText::new(description).color(MUTED).size(14.0));
        ui.add_space(22.0);
    }

    fn start_discovery(&mut self) {
        if self.discover_worker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.discover_worker = Some(rx);
        self.discovery_complete = false;
        std::thread::spawn(move || {
            let _ = tx.send(universal::discover_apps());
        });
    }

    fn tick_discovery(&mut self, ctx: &egui::Context) {
        if let Some(worker) = &self.discover_worker {
            match worker.try_recv() {
                Ok(apps) => {
                    self.discovered = apps;
                    self.discover_worker = None;
                    self.discovery_complete = true;
                    #[cfg(target_os = "windows")]
                    self.start_icon_discovery();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.notice = Some(("App discovery stopped unexpectedly.".into(), true));
                    self.discover_worker = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(160));
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn start_icon_discovery(&mut self) {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        let paths: Vec<PathBuf> = self
            .discovered
            .iter()
            .filter(|app| app.launchable && !app.path.as_os_str().is_empty())
            .filter_map(|app| {
                if seen.insert(app.path.clone()) {
                    Some(app.path.clone())
                } else {
                    None
                }
            })
            .take(160)
            .collect();
        self.app_icons.clear();
        let (sender, receiver) = mpsc::channel();
        self.icon_worker = Some(receiver);
        std::thread::spawn(move || {
            for path in paths {
                let Ok(info) = std::fs::metadata(&path) else {
                    continue;
                };
                if !info.is_file() || info.len() > 512 * 1024 * 1024 {
                    continue;
                }
                let Ok(icon) = win_icon_extractor::extract_icon_with_size(&path, 48) else {
                    continue;
                };
                if icon.width == 0 || icon.height == 0 || icon.width > 128 || icon.height > 128 {
                    continue;
                }
                let width = icon.width as usize;
                let height = icon.height as usize;
                if icon.rgba.len() != width * height * 4 {
                    continue;
                }
                if sender
                    .send(IconPixels {
                        path,
                        width,
                        height,
                        rgba: icon.rgba,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    #[cfg(target_os = "windows")]
    fn tick_icons(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.icon_worker else {
            return;
        };
        let mut finished = false;
        loop {
            match receiver.try_recv() {
                Ok(IconPixels {
                    path,
                    width,
                    height,
                    rgba,
                }) => {
                    let texture = ctx.load_texture(
                        format!("installed-app:{}", path.display()),
                        egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba),
                        egui::TextureOptions::LINEAR,
                    );
                    self.app_icons.insert(path, texture);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.icon_worker = None;
        } else {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("navigation")
            .resizable(false)
            .exact_width(232.0)
            .frame(Frame::new().fill(NAV).inner_margin(egui::Margin::same(24)))
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    icons::inline(ui, Icon::Layers, 30.0, Color32::from_rgb(129, 200, 177));
                    ui.label(RichText::new("softclone").color(WHITE).size(23.0).strong());
                });
                ui.add_space(5.0);
                ui.label(
                    RichText::new("YOUR APPS. YOUR SPACE.")
                        .color(Color32::from_rgb(153, 169, 169))
                        .size(10.0),
                );
                ui.add_space(54.0);
                ui.label(
                    RichText::new("WORKSPACE")
                        .color(Color32::from_rgb(125, 147, 149))
                        .size(10.0)
                        .strong(),
                );
                ui.add_space(8.0);
                if Self::nav_button(ui, Icon::Copy, "My clones", self.page == Page::Library) {
                    self.page = Page::Library;
                }
                if Self::nav_button(
                    ui,
                    Icon::SquarePlus,
                    "Create clone",
                    self.page == Page::Create,
                ) {
                    self.page = Page::Create;
                }
                if Self::nav_button(
                    ui,
                    Icon::ScanSearch,
                    "Discover apps",
                    self.page == Page::Discover,
                ) {
                    self.page = Page::Discover;
                    if !self.discovery_complete && self.discover_worker.is_none() {
                        self.start_discovery();
                    }
                }
                ui.add_space(24.0);
                ui.label(
                    RichText::new("PREFERENCES")
                        .color(Color32::from_rgb(125, 147, 149))
                        .size(10.0)
                        .strong(),
                );
                ui.add_space(8.0);
                if Self::nav_button(ui, Icon::Info, "How it works", self.page == Page::About) {
                    self.page = Page::About;
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(
                        RichText::new("VERSION 0.5.2  •  LOCAL")
                            .size(10.0)
                            .color(Color32::from_rgb(133, 154, 155)),
                    );
                });
            });
    }

    fn nav_button(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool) -> bool {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(184.0, 42.0), egui::Sense::click());
        let fill = if active {
            Color32::from_rgb(46, 76, 75)
        } else if response.hovered() {
            Color32::from_rgb(32, 57, 60)
        } else {
            NAV
        };
        let foreground = if active {
            WHITE
        } else {
            Color32::from_rgb(183, 200, 201)
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(7), fill);
        icons::draw(
            painter,
            icon,
            egui::pos2(rect.left() + 23.0, rect.center().y),
            18.0,
            if active {
                Color32::from_rgb(142, 213, 194)
            } else {
                foreground
            },
        );
        painter.text(
            egui::pos2(rect.left() + 48.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.5),
            foreground,
        );
        response.clicked()
    }

    fn stat(ui: &mut egui::Ui, value: &str, label: &str) {
        Frame::new()
            .fill(WHITE)
            .stroke(Stroke::new(1.0_f32, LINE))
            .corner_radius(12)
            .inner_margin(20)
            .show(ui, |ui| {
                ui.set_min_width(180.0);
                ui.label(RichText::new(value).size(28.0).color(INK).strong());
                ui.label(RichText::new(label).color(MUTED).size(12.0));
            });
    }

    fn library(&mut self, ui: &mut egui::Ui) {
        Self::headline(
            ui,
            "Workspace overview",
            "Your clone library",
            "Manage copies, profiles, snapshots and tracked app processes.",
        );
        ui.horizontal_wrapped(|ui| {
            Self::stat(ui, &self.items.len().to_string(), "Total clones");
            let bytes: u64 = self.items.iter().map(|x| x.copied_bytes).sum();
            Self::stat(ui, &format_size(bytes), "Copied files");
            Self::stat(
                ui,
                &self
                    .items
                    .iter()
                    .filter(|x| x.isolated_profile)
                    .count()
                    .to_string(),
                "Separate profiles",
            );
        });
        ui.add_space(26.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Applications").color(INK).size(19.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("New clone").color(WHITE).strong())
                            .fill(ACCENT),
                    )
                    .clicked()
                {
                    self.page = Page::Create;
                }
            });
        });
        ui.add_space(10.0);
        crate::components::fields::helper(
            ui,
            "Process controls apply only to apps launched during this SoftClone session. Backups copy the managed instance folder, never the original installation.",
        );
        if let Some((_, task)) = &self.manager_busy {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(15.0));
                ui.label(RichText::new(format!("{task}…")).color(ACCENT));
            });
        }
        ui.add_space(12.0);
        if self.items.is_empty() {
            Frame::new()
                .fill(WHITE)
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(12)
                .inner_margin(30)
                .show(ui, |ui| {
                    ui.set_min_height(172.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(13.0);
                        icons::inline(ui, Icon::Layers, 38.0, ACCENT);
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("A fresh workspace starts here")
                                .size(18.0)
                                .color(INK)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(
                                "Create a local app instance without modifying the original.",
                            )
                            .color(MUTED),
                        );
                        ui.add_space(10.0);
                        if icons::button(
                            ui,
                            Icon::SquarePlus,
                            "Create your first clone",
                            Some(ACCENT),
                        )
                        .clicked()
                        {
                            self.page = Page::Create;
                        }
                    });
                });
            return;
        }
        let busy = self.manager_worker.is_some() || self.worker.is_some();
        for record in self.items.clone() {
            let pid = self.processes.get(&record.id).map(Child::id);
            if let Some(action) = instance_card::show(ui, &record, pid, busy) {
                self.on_instance_action(&record, action);
            }
            ui.add_space(8.0);
        }
    }

    fn tick_processes(&mut self, ctx: &egui::Context) {
        let mut finished = Vec::new();
        for (id, child) in &mut self.processes {
            match child.try_wait() {
                Ok(Some(_)) => finished.push(*id),
                Err(error) => {
                    self.notice = Some((format!("Cannot inspect managed process: {error}"), true));
                    finished.push(*id);
                }
                Ok(None) => {}
            }
        }
        for id in finished {
            self.processes.remove(&id);
        }
        if !self.processes.is_empty() {
            ctx.request_repaint_after(Duration::from_secs(2));
        }
    }

    fn stop_managed_process(&mut self, id: Uuid) -> Result<(), String> {
        let Some(mut child) = self.processes.remove(&id) else {
            return Err("No managed process is tracked for this instance.".into());
        };
        match child.try_wait() {
            Ok(Some(_)) => Ok(()),
            Ok(None) => {
                if let Err(error) = child.kill() {
                    self.processes.insert(id, child);
                    return Err(format!("Could not stop managed process: {error}"));
                }
                child.wait().map_err(|error| error.to_string())?;
                Ok(())
            }
            Err(error) => {
                self.processes.insert(id, child);
                Err(format!(
                    "Could not inspect process before stopping it: {error}"
                ))
            }
        }
    }

    fn launch_managed(&mut self, record: &CloneRecord) {
        if self.processes.contains_key(&record.id) {
            self.notice = Some(("Instance is already running in this session.".into(), true));
            return;
        }
        match core::launch(record) {
            Ok(child) => {
                let pid = child.id();
                self.processes.insert(record.id, child);
                let target = if record.run_mode == core::RunMode::ProfileOnly {
                    &record.source
                } else {
                    &record.entrypoint
                };
                let detail = if universal::adapter_for(target)
                    == Some(universal::Adapter::ZoomWindows)
                {
                    " Zoom multi-instance mode requested with a unique data selector; verify that a second window appeared."
                } else if record.run_mode == core::RunMode::ProfileOnly
                    && universal::adapter_for(target).is_none()
                {
                    " This app has no verified independent-profile adapter and may reuse an existing window."
                } else {
                    ""
                };
                self.notice = Some((
                    format!(
                        "Launch requested for {} (tracked PID {pid}).{detail}",
                        record.name
                    ),
                    false,
                ));
            }
            Err(error) => {
                self.notice = Some((format!("Launch failed: {error}"), true));
            }
        }
    }

    fn start_manager_work(&mut self, record: CloneRecord, work: ManagerWork) {
        if self.manager_worker.is_some() || self.worker.is_some() {
            self.notice = Some(("Finish the current operation first.".into(), true));
            return;
        }
        if self.processes.contains_key(&record.id) && !matches!(&work, ManagerWork::Rename(_)) {
            self.notice = Some((
                "Stop the tracked app before backup, restore or delete.".into(),
                true,
            ));
            return;
        }
        let label = match &work {
            ManagerWork::Rename(_) => "Renaming",
            ManagerWork::Duplicate(_) => "Duplicating",
            ManagerWork::Backup => "Backing up",
            ManagerWork::Restore(_) => "Restoring snapshot",
            ManagerWork::Delete => "Deleting instance",
        };
        self.manager_busy = Some((record.id, label.into()));
        let (tx, rx) = mpsc::channel();
        self.manager_worker = Some(rx);
        std::thread::spawn(move || {
            let library = registry::library_dir();
            let backups = registry::backups_dir();
            let database = registry::registry_file();
            let result = match work {
                ManagerWork::Rename(name) => instance_manager::rename(&record, &name, &database)
                    .map(|()| format!("Renamed instance to {name}.")),
                ManagerWork::Duplicate(name) => instance_manager::duplicate(
                    &record, &name, &library, &database,
                )
                .map(|duplicate| format!("Duplicated {} as {}.", record.name, duplicate.name)),
                ManagerWork::Backup => {
                    instance_manager::backup(&record, &library, &backups, &database).map(|info| {
                        format!(
                            "Backup created: {} files, {}.",
                            info.files,
                            format_size(info.copied_bytes),
                        )
                    })
                }
                ManagerWork::Restore(id) => {
                    instance_manager::restore(&record, id, &library, &backups, &database)
                        .map(|()| format!("Restored snapshot for {}.", record.name))
                }
                ManagerWork::Delete => instance_manager::delete(&record, &library, &database)
                    .map(|()| format!("Deleted managed instance {}.", record.name)),
            }
            .map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
    }

    fn tick_manager(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.manager_worker else {
            return;
        };
        let result = worker.try_recv();
        match result {
            Ok(outcome) => {
                self.manager_worker = None;
                self.manager_busy = None;
                let registry_reload = registry::load(&registry::registry_file());
                if let Ok(items) = registry_reload {
                    self.items = items;
                } else if let Err(error) = registry_reload {
                    self.notice =
                        Some((format!("Could not reload instance registry: {error}"), true));
                    return;
                }
                match outcome {
                    Ok(message) => self.notice = Some((message, false)),
                    Err(error) => self.notice = Some((error, true)),
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.manager_worker = None;
                self.manager_busy = None;
                self.notice = Some(("Instance manager worker disconnected.".into(), true));
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(130));
            }
        }
    }

    fn on_instance_action(&mut self, record: &CloneRecord, action: InstanceAction) {
        match action {
            InstanceAction::Launch => self.launch_managed(record),
            InstanceAction::Stop => match self.stop_managed_process(record.id) {
                Ok(()) => self.notice = Some((format!("Stopped {}.", record.name), false)),
                Err(error) => self.notice = Some((error, true)),
            },
            InstanceAction::Restart => match self.stop_managed_process(record.id) {
                Ok(()) => self.launch_managed(record),
                Err(error) => self.notice = Some((error, true)),
            },
            InstanceAction::OpenFolder => {
                if let Err(error) = core::open_folder(&record.root) {
                    self.notice = Some((format!("Cannot open folder: {error}"), true));
                }
            }
            InstanceAction::Rename => {
                self.manager_name = record.name.clone();
                self.pending_manager = Some(PendingManager::Rename(record.id));
            }
            InstanceAction::Duplicate => {
                if self.processes.contains_key(&record.id) {
                    self.notice = Some(("Stop this instance before duplicating it.".into(), true));
                    return;
                }
                self.manager_name = format!("{} copy", record.name);
                self.pending_manager = Some(PendingManager::Duplicate(record.id));
            }
            InstanceAction::Backup => self.start_manager_work(record.clone(), ManagerWork::Backup),
            InstanceAction::Restore => {
                if self.processes.contains_key(&record.id) {
                    self.notice = Some((
                        "Stop this instance before restoring a snapshot.".into(),
                        true,
                    ));
                    return;
                }
                match instance_manager::list_backups(&registry::backups_dir(), record.id) {
                    Ok(options) if options.is_empty() => {
                        self.notice = Some((
                            "No snapshot found for this instance. Create a backup first.".into(),
                            true,
                        ));
                    }
                    Ok(options) => {
                        self.restore_selected = Some(options[0].id);
                        self.restore_options = options;
                        self.pending_manager = Some(PendingManager::Restore(record.id));
                    }
                    Err(error) => {
                        self.notice = Some((format!("Cannot read backups: {error}"), true))
                    }
                }
            }
            InstanceAction::Delete => {
                if self.processes.contains_key(&record.id) {
                    self.notice = Some(("Stop this instance before deleting it.".into(), true));
                    return;
                }
                self.confirm_name.clear();
                self.pending_manager = Some(PendingManager::Delete(record.id));
            }
        }
    }

    fn manager_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_manager.clone() else {
            return;
        };
        let id = match &pending {
            PendingManager::Rename(id)
            | PendingManager::Duplicate(id)
            | PendingManager::Restore(id)
            | PendingManager::Delete(id) => *id,
        };
        let Some(record) = self.items.iter().find(|item| item.id == id).cloned() else {
            self.pending_manager = None;
            return;
        };
        let title = match pending {
            PendingManager::Rename(_) => "Rename instance",
            PendingManager::Duplicate(_) => "Duplicate instance",
            PendingManager::Restore(_) => "Restore snapshot",
            PendingManager::Delete(_) => "Delete instance",
        };
        let mut close = false;
        let mut execute = None;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(&record.name).strong().color(INK));
                match pending {
                    PendingManager::Rename(_) => {
                        ui.label("Change the display label. Files and folders will not move.");
                        ui.text_edit_singleline(&mut self.manager_name);
                        let valid = core::validate_name(&self.manager_name).is_ok();
                        if ui.add_enabled(valid, egui::Button::new("Save name")).clicked() {
                            execute = Some(ManagerWork::Rename(self.manager_name.clone()));
                        }
                    }
                    PendingManager::Duplicate(_) => {
                        ui.label("Create a separate managed folder and profile from this instance.");
                        ui.label("The original vendor installation is not modified.");
                        ui.text_edit_singleline(&mut self.manager_name);
                        let valid = core::validate_name(&self.manager_name).is_ok();
                        if ui.add_enabled(valid, egui::Button::new("Duplicate instance")).clicked() {
                            execute = Some(ManagerWork::Duplicate(self.manager_name.clone()));
                        }
                    }
                    PendingManager::Restore(_) => {
                        ui.label("This replaces the managed instance folder with the selected snapshot.");
                        ui.label("The original installed application is not modified. Stop apps before restoring.");
                        egui::ComboBox::from_label("Backup snapshot")
                            .selected_text(
                                self.restore_selected
                                    .map(|id| id.to_string())
                                    .unwrap_or_else(|| "Select snapshot".into()),
                            )
                            .show_ui(ui, |ui| {
                                for option in &self.restore_options {
                                    ui.selectable_value(
                                        &mut self.restore_selected,
                                        Some(option.id),
                                        format!(
                                            "{} · {} files · {}",
                                            &option.id.to_string()[..8],
                                            option.files,
                                            format_size(option.copied_bytes)
                                        ),
                                    );
                                }
                            });
                        if ui.add_enabled(self.restore_selected.is_some(), egui::Button::new("Restore selected snapshot")).clicked() {
                            if let Some(id) = self.restore_selected {
                                execute = Some(ManagerWork::Restore(id));
                            }
                        }
                    }
                    PendingManager::Delete(_) => {
                        ui.label(
                            RichText::new("This permanently deletes SoftClone-managed instance files and removes its registry entry.")
                                .color(Color32::from_rgb(160, 62, 48)),
                        );
                        ui.label("Backups are retained. Original installed application files are not deleted.");
                        ui.label("Type the exact instance name to confirm:");
                        ui.text_edit_singleline(&mut self.confirm_name);
                        if ui
                            .add_enabled(
                                self.confirm_name == record.name,
                                egui::Button::new("Delete managed instance"),
                            )
                            .clicked()
                        {
                            execute = Some(ManagerWork::Delete);
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        if close {
            self.pending_manager = None;
        } else if let Some(work) = execute {
            self.pending_manager = None;
            self.start_manager_work(record, work);
        }
    }

    fn discover(&mut self, ui: &mut egui::Ui) {
        Self::headline(
            ui,
            "Installed applications",
            "Discover your apps",
            "Browse installed software and see which instance methods are supported.",
        );

        let visible = self
            .discovered
            .iter()
            .filter(|app| universal::matches_app(app, &self.discover_query, self.discover_filter))
            .count();
        let toolbar = discovery_form::DiscoveryToolbar {
            query: &mut self.discover_query,
            scanning: self.discover_worker.is_some(),
            complete: self.discovery_complete,
            total: self.discovered.len(),
            visible,
        }
        .show(ui);
        if toolbar.rescan {
            self.start_discovery();
        }
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.discover_filter, AppFilter::All, "All apps");
            ui.selectable_value(
                &mut self.discover_filter,
                AppFilter::ProfileReady,
                "Profile-ready",
            );
            ui.selectable_value(
                &mut self.discover_filter,
                AppFilter::NeedsReview,
                "Requires review",
            );
            ui.selectable_value(
                &mut self.discover_filter,
                AppFilter::MissingExecutable,
                "Missing executable",
            );
        });
        ui.add_space(6.0);
        discovery_form::compatibility_note(ui);
        ui.add_space(18.0);

        let matches: Vec<_> = self
            .discovered
            .iter()
            .filter(|app| universal::matches_app(app, &self.discover_query, self.discover_filter))
            .cloned()
            .collect();
        if matches.is_empty() && self.discover_worker.is_none() {
            Frame::new()
                .fill(WHITE)
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(12)
                .inner_margin(24)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("No matching applications")
                            .color(INK)
                            .strong(),
                    );
                    crate::components::fields::helper(
                        ui,
                        "Adjust the search or filter, rescan your applications, or choose an executable under Create clone.",
                    );
                });
        }
        for app in matches {
            #[cfg(target_os = "windows")]
            let texture = self.app_icons.get(&app.path);
            #[cfg(not(target_os = "windows"))]
            let texture = None;
            match discovery_form::app_card(ui, &app, texture) {
                Some(discovery_form::AppCardAction::UseApp) => {
                    self.source = app.path.display().to_string();
                    self.name = format!("{} copy", app.name);
                    self.entrypoint.clear();
                    self.profile_only = app.support != Support::CopyCandidate;
                    self.isolated = true;
                    self.page = Page::Create;
                }
                Some(discovery_form::AppCardAction::OpenStoreApp) => {
                    #[cfg(target_os = "windows")]
                    let open = app
                        .store_id
                        .as_deref()
                        .ok_or_else(|| "Missing Windows application ID.".to_string())
                        .and_then(softclone::windows_store::open_app);
                    #[cfg(target_os = "macos")]
                    let open = std::process::Command::new("open")
                        .arg("-a")
                        .arg(&app.path)
                        .spawn()
                        .map(|_| ())
                        .map_err(|error| error.to_string());
                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                    let open: Result<(), String> =
                        Err("This platform does not support Store app activation.".into());
                    self.notice = Some(match open {
                        Ok(()) => (
                            format!(
                                "Requested a normal launch of {}. Store apps do not have separate SoftClone profiles.",
                                app.name
                            ),
                            false,
                        ),
                        Err(error) => (format!("Cannot open {}: {error}", app.name), true),
                    });
                }
                Some(discovery_form::AppCardAction::LocateExecutable) => {
                    let selected = rfd::FileDialog::new()
                        .set_title(format!("Locate executable for {}", app.name))
                        .add_filter("Application executable", &["exe"])
                        .pick_file();
                    if let Some(path) = selected {
                        if path.is_file()
                            && path
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                        {
                            self.source = path.display().to_string();
                            self.name = format!("{} copy", app.name);
                            self.entrypoint.clear();
                            self.profile_only = true;
                            self.isolated = true;
                            self.page = Page::Create;
                            self.notice = Some((
                                "Executable selected manually. Independent profiles for this program are not guaranteed; review the instance strategy before cloning.".into(),
                                false,
                            ));
                        } else {
                            self.notice = Some((
                                "Select the application's primary .exe file, not an installer, plugin or shortcut.".into(),
                                true,
                            ));
                        }
                    }
                }
                None => {}
            }
            ui.add_space(8.0);
        }
    }

    fn create(&mut self, ui: &mut egui::Ui) {
        Self::headline(
            ui,
            "New instance",
            "Create an app clone",
            "Choose local application files. The original stays exactly where it is.",
        );
        let path = PathBuf::from(self.source.trim());
        let next_label_number =
            self.items.iter().filter(|item| item.source == path).count() as i64 + 1;
        let current_key = self.analysis_key();
        let analysis = self
            .analysis
            .as_ref()
            .filter(|(key, _)| key == &current_key)
            .map(|(_, plan)| plan);
        let action = CreateForm {
            source: &mut self.source,
            name: &mut self.name,
            entrypoint: &mut self.entrypoint,
            isolated: &mut self.isolated,
            profile_only: &mut self.profile_only,
            next_label_number,
            creating: self.worker.is_some(),
            analyzing: self.analysis_worker.is_some(),
            analysis,
            progress: self.copy_progress.as_ref(),
            cancellation_requested: self.cancellation_requested,
        }
        .show(ui);
        if let Some(message) = action.error {
            self.notice = Some((message, true));
        }
        if action.browse_file || action.browse_folder {
            // Native OS dialog is modal; it intentionally runs on the UI thread,
            // as required by some macOS windowing integrations.
            let dialog = rfd::FileDialog::new();
            let selected = if action.browse_folder {
                dialog.pick_folder()
            } else {
                dialog.pick_file()
            };
            if let Some(path) = selected {
                self.source = path.display().to_string();
                if self.name.trim().is_empty() {
                    let label = path
                        .file_stem()
                        .or_else(|| path.file_name())
                        .map(|part| part.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "My app".into());
                    self.name = format!("{label} copy");
                }
            }
        }
        if action.analyze {
            self.start_analysis();
        }
        if action.cancel {
            if let Some(cancel) = &self.cancel_token {
                cancel.store(true, Ordering::SeqCst);
                self.cancellation_requested = true;
            }
        }
        if action.submit {
            self.submit();
        }
        ui.add_space(16.0);
        crate::components::fields::helper(
            ui,
            "Only clone software you have rights to copy. Licenses and operating-system restrictions still apply.",
        );
    }

    fn submit(&mut self) {
        if self.worker.is_some() || self.analysis_worker.is_some() {
            return;
        }
        let request = CloneRequest {
            source: PathBuf::from(self.source.trim()),
            name: self.name.clone(),
            entrypoint: self.entrypoint.clone(),
            isolated_profile: self.isolated || self.profile_only,
        };
        let prepared = match core::prepare(&request) {
            Ok(plan) => plan,
            Err(error) => {
                self.notice = Some((format!("Check clone details: {error}"), true));
                return;
            }
        };
        let library = registry::library_dir();
        let profile_only = self.profile_only;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_token = Some(Arc::clone(&cancel));
        self.cancellation_requested = false;
        self.copy_progress = None;
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        std::thread::spawn(move || {
            let result = (if profile_only {
                if cancel.load(Ordering::SeqCst) {
                    Err(core::CloneError::Cancelled)
                } else {
                    core::create_profile_instance(&prepared, &library)
                }
            } else {
                core::create_clone_with_progress(
                    &prepared,
                    &library,
                    &cancel,
                    |progress| {
                        let _ = sender.send(CloneEvent::Progress(progress));
                    },
                )
            })
            .map_err(|error| error.to_string())
            .and_then(|item| {
                if let Err(error) = registry::add(item.clone()) {
                    let cleanup = std::fs::remove_dir_all(&item.root);
                    return Err(match cleanup {
                        Ok(()) => format!(
                            "Could not register instance: {error}. Newly created files were removed."
                        ),
                        Err(cleanup_error) => format!(
                            "Could not register instance: {error}. Cleanup failed at {}: {cleanup_error}",
                            item.root.display(),
                        ),
                    });
                }
                Ok(item)
            });
            let _ = sender.send(CloneEvent::Finished(result));
        });
    }

    fn about(&mut self, ui: &mut egui::Ui) {
        Self::headline(
            ui,
            "A closer look",
            "How SoftClone works",
            "Clear boundaries. Predictable behavior. Your data stays local.",
        );
        let items = [
            ("01", "Discover applications", "Scan supported local application directories without opening or changing installed software."),
            ("02", "Choose an instance strategy", "Copy portable files, or keep installed app binaries and create a dedicated profile. Known browser adapters use profile flags."),
            ("03", "Launch with clear boundaries", "The app starts as a normal user process. Single-instance, activation, and OS restrictions still apply."),
            ("04", "Keep control", "Browse each clone's files. Nothing is uploaded, and SoftClone never asks for administrator privileges."),
        ];
        for (number, title, detail) in items {
            Frame::new()
                .fill(WHITE)
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(12)
                .inner_margin(22)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(number).size(19.0).strong().color(ACCENT));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).size(16.0).strong().color(INK));
                            ui.label(RichText::new(detail).size(13.0).color(MUTED));
                        });
                    });
                });
            ui.add_space(10.0);
        }
        ui.add_space(14.0);
        ui.label(
            RichText::new("NOT AN OS CONTAINER")
                .size(11.0)
                .color(ACCENT)
                .strong(),
        );
        ui.label(RichText::new("Software bound to the registry, keychain, machine identity, kernel drivers, services, DRM, or a single-instance lock may not work as a clone.").color(MUTED).size(13.0));
        ui.add_space(8.0);
        if icons::button(ui, Icon::FolderOpen, "Open data directory", None).clicked() {
            if let Err(e) = core::open_folder(&registry::app_data_dir()) {
                self.notice = Some((e.to_string(), true));
            }
        }
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_worker(ctx);
        self.tick_manager(ctx);
        self.tick_processes(ctx);
        self.tick_analysis(ctx);
        self.tick_discovery(ctx);
        #[cfg(target_os = "windows")]
        self.tick_icons(ctx);
        self.sidebar(ctx);
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(SURFACE)
                    .inner_margin(egui::Margin::same(32)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("SOFTCLONE  /  DESKTOP WORKSPACE")
                                .color(MUTED)
                                .size(11.0),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            icons::inline(ui, Icon::HardDrive, 15.0, ACCENT);
                            ui.label(RichText::new("LOCAL").color(ACCENT).size(11.0).strong());
                        });
                    });
                    ui.add_space(20.0);
                    if let Some((message, is_error)) = self.notice.clone() {
                        Frame::new()
                            .fill(if is_error {
                                Color32::from_rgb(255, 240, 235)
                            } else {
                                Color32::from_rgb(229, 244, 236)
                            })
                            .corner_radius(9)
                            .inner_margin(13)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    icons::inline(
                                        ui,
                                        if is_error {
                                            Icon::CircleAlert
                                        } else {
                                            Icon::CheckCircle
                                        },
                                        17.0,
                                        if is_error {
                                            Color32::from_rgb(160, 62, 48)
                                        } else {
                                            ACCENT
                                        },
                                    );
                                    ui.label(RichText::new(message).size(12.5).color(
                                        if is_error {
                                            Color32::from_rgb(160, 62, 48)
                                        } else {
                                            ACCENT
                                        },
                                    ));
                                    if ui.small_button("Dismiss").clicked() {
                                        self.notice = None;
                                    }
                                });
                            });
                        ui.add_space(15.0);
                    }
                    match self.page {
                        Page::Library => self.library(ui),
                        Page::Create => self.create(ui),
                        Page::Discover => self.discover(ui),
                        Page::About => self.about(ui),
                    }
                    ui.add_space(36.0);
                });
            });
        self.manager_dialog(ctx);
    }
}

fn format_size(bytes: u64) -> String {
    const SUFFIXES: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut amount = bytes as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit < SUFFIXES.len() - 1 {
        amount /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{amount:.1} {}", SUFFIXES[unit])
    }
}
