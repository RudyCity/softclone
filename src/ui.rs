use crate::components::{create_form::CreateForm, discovery_form};
use crate::icons::{self, Icon};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, RichText, Stroke, Vec2,
};
use softclone::{
    core::{self, CloneRecord, CloneRequest, RunMode},
    registry,
    universal::{self, DiscoveredApp, Support},
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::Duration,
};

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
    discover_worker: Option<Receiver<Vec<DiscoveredApp>>>,
    discovery_complete: bool,
    worker: Option<Receiver<Result<CloneRecord, String>>>,
    notice: Option<(String, bool)>,
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
            discover_worker: None,
            discovery_complete: false,
            worker: None,
            notice,
        }
    }

    fn tick_worker(&mut self, ctx: &egui::Context) {
        if let Some(worker) = &self.worker {
            match worker.try_recv() {
                Ok(Ok(item)) => {
                    self.items.insert(0, item);
                    self.notice = Some(("Clone created. You can now launch it.".into(), false));
                    self.worker = None;
                    self.page = Page::Library;
                    self.source.clear();
                    self.name.clear();
                    self.entrypoint.clear();
                    self.profile_only = false;
                }
                Ok(Err(e)) => {
                    self.notice = Some((e, true));
                    self.worker = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.notice = Some(("Clone worker stopped unexpectedly.".into(), true));
                    self.worker = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(120))
                }
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
                        RichText::new("VERSION 0.2.1  •  LOCAL FIRST")
                            .size(10.0)
                            .color(Color32::from_rgb(133, 154, 155)),
                    );
                    ui.add_space(10.0);
                    ui.separator();
                    ui.label(
                        RichText::new("No account. No cloud sync.")
                            .size(11.0)
                            .color(Color32::from_rgb(157, 175, 176)),
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
            "Independent copies, managed in one quiet place.",
        );
        ui.horizontal(|ui| {
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
        ui.add_space(30.0);
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
        ui.add_space(12.0);
        if self.items.is_empty() {
            Frame::new().fill(WHITE).stroke(Stroke::new(1.0_f32, LINE)).corner_radius(12)
                .inner_margin(30).show(ui, |ui| {
                    ui.set_min_height(172.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(13.0);
                        icons::inline(ui, Icon::Layers, 38.0, ACCENT);
                        ui.add_space(6.0);
                        ui.label(RichText::new("A fresh workspace starts here").size(18.0).color(INK).strong());
                        ui.label(RichText::new("Create a local copy of an app you own, without touching the original.").color(MUTED));
                        ui.add_space(10.0);
                        if icons::button(ui, Icon::SquarePlus, "Create your first clone", Some(ACCENT)).clicked() { self.page = Page::Create; }
                    });
                });
            return;
        }
        let cards = self.items.clone();
        for item in cards {
            Frame::new()
                .fill(WHITE)
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(12)
                .inner_margin(18)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        Frame::new()
                            .fill(Color32::from_rgb(232, 244, 238))
                            .corner_radius(10)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                icons::inline(ui, Icon::Copy, 24.0, ACCENT);
                            });
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&item.name).size(16.0).strong().color(INK));
                            ui.label(
                                RichText::new(format!(
                                    "{}  ·  {}",
                                    format_size(item.copied_bytes),
                                    if item.run_mode == RunMode::ProfileOnly {
                                        "Installed app · profile-only"
                                    } else if item.isolated_profile {
                                        "Portable copy · separate profile"
                                    } else {
                                        "Portable copy"
                                    }
                                ))
                                .size(12.0)
                                .color(MUTED),
                            );
                            ui.label(
                                RichText::new(item.source.display().to_string())
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icons::button(ui, Icon::Play, "Launch", Some(ACCENT)).clicked() {
                                match core::launch(&item) {
                                    Ok(_) => {
                                        self.notice =
                                            Some((format!("Launched {}.", item.name), false))
                                    }
                                    Err(e) => {
                                        self.notice = Some((format!("Launch failed: {e}"), true))
                                    }
                                }
                            }
                            if icons::button(ui, Icon::FolderOpen, "Open folder", None).clicked() {
                                if let Err(e) = core::open_folder(&item.root) {
                                    self.notice = Some((format!("Cannot open folder: {e}"), true));
                                }
                            }
                        });
                    });
                });
            ui.add_space(7.0);
        }
    }

    fn discover(&mut self, ui: &mut egui::Ui) {
        Self::headline(
            ui,
            "Installed applications",
            "Discover your apps",
            "Browse registered installed apps, then choose a safe instance strategy.",
        );

        let filter = self.discover_query.trim().to_lowercase();
        let visible = self
            .discovered
            .iter()
            .filter(|app| {
                filter.is_empty()
                    || app.name.to_lowercase().contains(&filter)
                    || app.path.to_string_lossy().to_lowercase().contains(&filter)
            })
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
        ui.add_space(10.0);
        discovery_form::compatibility_note(ui);
        ui.add_space(18.0);

        let filter = self.discover_query.trim().to_lowercase();
        let matches: Vec<_> = self
            .discovered
            .iter()
            .filter(|app| {
                filter.is_empty()
                    || app.name.to_lowercase().contains(&filter)
                    || app.path.to_string_lossy().to_lowercase().contains(&filter)
            })
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
                        "Try another search, rescan your applications, or enter a source path under Create clone.",
                    );
                });
        }
        for app in matches {
            if discovery_form::app_card(ui, &app) {
                self.source = app.path.display().to_string();
                self.name = format!("{} copy", app.name);
                self.entrypoint.clear();
                self.profile_only = app.support != Support::CopyCandidate;
                self.isolated = true;
                self.page = Page::Create;
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
        let action = CreateForm {
            source: &mut self.source,
            name: &mut self.name,
            entrypoint: &mut self.entrypoint,
            isolated: &mut self.isolated,
            profile_only: &mut self.profile_only,
            next_label_number,
            creating: self.worker.is_some(),
        }
        .show(ui);
        if let Some(message) = action.error {
            self.notice = Some((message, true));
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
        if self.worker.is_some() {
            return;
        }
        let req = CloneRequest {
            source: PathBuf::from(self.source.trim()),
            name: self.name.clone(),
            entrypoint: self.entrypoint.clone(),
            isolated_profile: self.isolated || self.profile_only,
        };
        let prepared = match core::prepare(&req) {
            Ok(plan) => plan,
            Err(e) => {
                self.notice = Some((format!("Check clone details: {e}"), true));
                return;
            }
        };
        let library = registry::library_dir();
        let profile_only = self.profile_only;
        let (tx, rx) = mpsc::channel();
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let result = (if profile_only {
                core::create_profile_instance(&prepared, &library)
            } else {
                core::create_clone(&prepared, &library)
            })
            .map_err(|e| e.to_string())
            .and_then(|item| {
                registry::add(item.clone()).map_err(|e| {
                    format!(
                        "Instance created, but registry save failed: {e}. Location: {}",
                        item.root.display()
                    )
                })?;
                Ok(item)
            });
            let _ = tx.send(result);
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
        self.tick_discovery(ctx);
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
