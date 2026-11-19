use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, RichText, Stroke, TextEdit, Vec2,
};
use softclone::{
    core::{self, CloneRecord, CloneRequest},
    registry, rune_engine,
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
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
    About,
}

pub struct DesktopApp {
    page: Page,
    items: Vec<CloneRecord>,
    selected: Option<Uuid>,
    source: String,
    name: String,
    entrypoint: String,
    isolated: bool,
    worker: Option<Receiver<Result<CloneRecord, String>>>,
    notice: Option<(String, bool)>,
}

impl DesktopApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(16.0, 10.0);
        style.visuals = egui::Visuals::light();
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
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
            selected: None,
            source: String::new(),
            name: String::new(),
            entrypoint: String::new(),
            isolated: true,
            worker: None,
            notice,
        }
    }

    fn tick_worker(&mut self, ctx: &egui::Context) {
        if let Some(worker) = &self.worker {
            match worker.try_recv() {
                Ok(Ok(item)) => {
                    self.selected = Some(item.id);
                    self.items.insert(0, item);
                    self.notice = Some(("Clone created. You can now launch it.".into(), false));
                    self.worker = None;
                    self.page = Page::Library;
                    self.source.clear();
                    self.name.clear();
                    self.entrypoint.clear();
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

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("navigation")
            .resizable(false)
            .exact_width(232.0)
            .frame(Frame::new().fill(NAV).inner_margin(egui::Margin::same(24)))
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("◧")
                            .color(Color32::from_rgb(129, 200, 177))
                            .size(28.0),
                    );
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
                if Self::nav_button(ui, "▦   My clones", self.page == Page::Library) {
                    self.page = Page::Library;
                }
                if Self::nav_button(ui, "+   Create clone", self.page == Page::Create) {
                    self.page = Page::Create;
                }
                ui.add_space(24.0);
                ui.label(
                    RichText::new("PREFERENCES")
                        .color(Color32::from_rgb(125, 147, 149))
                        .size(10.0)
                        .strong(),
                );
                ui.add_space(8.0);
                if Self::nav_button(ui, "ⓘ   How it works", self.page == Page::About) {
                    self.page = Page::About;
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(
                        RichText::new("VERSION 0.1.0  •  LOCAL FIRST")
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

    fn nav_button(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
        let fill = if active {
            Color32::from_rgb(46, 76, 75)
        } else {
            NAV
        };
        ui.add_sized(
            [184.0, 42.0],
            egui::Button::new(
                RichText::new(label)
                    .color(if active {
                        WHITE
                    } else {
                        Color32::from_rgb(183, 200, 201)
                    })
                    .size(13.5),
            )
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(7),
        )
        .clicked()
    }

    fn stat(ui: &mut egui::Ui, value: &str, label: &str) {
        Frame::new()
            .fill(WHITE)
            .stroke(Stroke::new(1.0, LINE))
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
                        egui::Button::new(RichText::new("+  New clone").color(WHITE).strong())
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
            Frame::new().fill(WHITE).stroke(Stroke::new(1.0, LINE)).corner_radius(12)
                .inner_margin(30).show(ui, |ui| {
                    ui.set_min_height(172.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(13.0);
                        ui.label(RichText::new("▣").size(38.0).color(ACCENT));
                        ui.add_space(6.0);
                        ui.label(RichText::new("A fresh workspace starts here").size(18.0).color(INK).strong());
                        ui.label(RichText::new("Create a local copy of an app you own, without touching the original.").color(MUTED));
                        ui.add_space(10.0);
                        if ui.button("Create your first clone").clicked() { self.page = Page::Create; }
                    });
                });
            return;
        }
        let cards = self.items.clone();
        for item in cards {
            Frame::new()
                .fill(WHITE)
                .stroke(Stroke::new(1.0, LINE))
                .corner_radius(12)
                .inner_margin(18)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        Frame::new()
                            .fill(Color32::from_rgb(232, 244, 238))
                            .corner_radius(10)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.label(RichText::new("◧").size(24.0).color(ACCENT));
                            });
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&item.name).size(16.0).strong().color(INK));
                            ui.label(
                                RichText::new(format!(
                                    "{}  ·  {}",
                                    format_size(item.copied_bytes),
                                    if item.isolated_profile {
                                        "Separate profile"
                                    } else {
                                        "Shared environment"
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
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Launch").color(WHITE).strong(),
                                    )
                                    .fill(ACCENT),
                                )
                                .clicked()
                            {
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
                            if ui.button("Open folder").clicked() {
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

    fn create(&mut self, ui: &mut egui::Ui) {
        Self::headline(
            ui,
            "New instance",
            "Create an app clone",
            "Choose local application files. The original stays exactly where it is.",
        );
        Frame::new().fill(WHITE).stroke(Stroke::new(1.0, LINE)).corner_radius(12)
            .inner_margin(26).show(ui, |ui| {
                ui.set_max_width(760.0);
                ui.label(RichText::new("01  SOURCE").color(ACCENT).size(11.0).strong());
                ui.label(RichText::new("Application path").color(INK).size(15.0).strong());
                ui.label(RichText::new("Folder, standalone executable, AppImage, or macOS .app bundle").color(MUTED).size(12.0));
                ui.add(TextEdit::singleline(&mut self.source)
                    .hint_text("Paste an absolute path, e.g. /Applications/Example.app")
                    .desired_width(f32::INFINITY));
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label(RichText::new("02  DETAILS").color(ACCENT).size(11.0).strong());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Clone name").color(INK).size(15.0).strong());
                    if ui.small_button("Suggest with Rune").clicked() {
                        let path = Path::new(self.source.trim());
                        let base = path.file_stem().or_else(|| path.file_name())
                            .map(|x| x.to_string_lossy().into_owned())
                            .filter(|x| !x.is_empty()).unwrap_or_else(|| "My app".into());
                        let number = self.items.iter().filter(|x| x.source == path).count() as i64 + 1;
                        match rune_engine::clone_label(&base, number) {
                            Ok(name) => self.name = name,
                            Err(e) => self.notice = Some((format!("Rune naming rule failed: {e}"), true)),
                        }
                    }
                });
                ui.add(TextEdit::singleline(&mut self.name).hint_text("e.g. Work browser").desired_width(f32::INFINITY));
                ui.add_space(9.0);
                ui.label(RichText::new("Entrypoint inside folder (optional)").color(INK).size(15.0).strong());
                ui.add(TextEdit::singleline(&mut self.entrypoint)
                    .hint_text("Auto-detect, or e.g. bin/app").desired_width(f32::INFINITY));
                ui.label(RichText::new("Only needed for folders with multiple executables or nested binaries.").size(11.0).color(MUTED));
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label(RichText::new("03  PROFILE").color(ACCENT).size(11.0).strong());
                ui.checkbox(&mut self.isolated, "Create separate per-clone data directories");
                ui.label(RichText::new("Best effort via environment variables. Some applications ignore these paths. Not a sandbox.").size(12.0).color(MUTED));
                ui.add_space(15.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Copies are stored in your local SoftClone library.").size(12.0).color(MUTED));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.worker.is_some() {
                            ui.spinner();
                            ui.label(RichText::new("Copying files…").color(ACCENT));
                        } else if ui.add(egui::Button::new(RichText::new("Create clone").color(WHITE).strong()).fill(ACCENT)).clicked() {
                            self.submit();
                        }
                    });
                });
            });
        ui.add_space(16.0);
        ui.label(RichText::new("Only clone applications you have rights to copy. License checks and OS restrictions still apply.").size(12.0).color(MUTED));
    }

    fn submit(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let req = CloneRequest {
            source: PathBuf::from(self.source.trim()),
            name: self.name.clone(),
            entrypoint: self.entrypoint.clone(),
            isolated_profile: self.isolated,
        };
        let prepared = match core::prepare(&req) {
            Ok(plan) => plan,
            Err(e) => {
                self.notice = Some((format!("Check clone details: {e}"), true));
                return;
            }
        };
        let library = registry::library_dir();
        let (tx, rx) = mpsc::channel();
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let result = core::create_clone(&prepared, &library)
                .map_err(|e| e.to_string())
                .and_then(|item| {
                    registry::add(item.clone()).map_err(|e| {
                        format!(
                            "Files copied, but registry save failed: {e}. Location: {}",
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
            ("01", "Copy the application", "Only the files you choose are copied to a unique instance folder. The source is not changed."),
            ("02", "Create a fresh profile", "Environment variables can direct supported applications toward per-clone config and cache directories. This is best effort."),
            ("03", "Launch independently", "The cloned executable starts as a normal user process. Some apps still enforce single-instance or license restrictions."),
            ("04", "Keep control", "Browse each clone's files. Nothing is uploaded, and SoftClone never asks for administrator privileges."),
        ];
        for (number, title, detail) in items {
            Frame::new()
                .fill(WHITE)
                .stroke(Stroke::new(1.0, LINE))
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
        if ui.button("Open data directory").clicked() {
            if let Err(e) = core::open_folder(&registry::app_data_dir()) {
                self.notice = Some((e.to_string(), true));
            }
        }
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_worker(ctx);
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
                            ui.label(RichText::new("● LOCAL").color(ACCENT).size(11.0).strong());
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
