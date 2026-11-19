//! Installed-app search and scan controls.
use super::fields;
use crate::icons::{self, Icon};
use eframe::egui::{self, Align, Color32, Frame, Label, Layout, RichText, Ui};
use softclone::universal::{DiscoveredApp, Support};

pub struct DiscoveryToolbar<'a> {
    pub query: &'a mut String,
    pub scanning: bool,
    pub complete: bool,
    pub total: usize,
    pub visible: usize,
}

pub struct ToolbarAction {
    pub rescan: bool,
}

impl DiscoveryToolbar<'_> {
    pub fn show(self, ui: &mut Ui) -> ToolbarAction {
        Frame::new()
            .fill(Color32::from_rgb(252, 253, 252))
            .stroke(egui::Stroke::new(1.0_f32, fields::BORDER))
            .corner_radius(12)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                let panel_width = ui.available_width();
                ui.set_width(panel_width);
                ui.label(
                    RichText::new("FIND AN APPLICATION")
                        .color(fields::ACCENT)
                        .strong()
                        .size(11.0),
                );
                ui.add_space(4.0);
                let available = ui.available_width();
                let mut rescan = false;
                if available >= 430.0 {
                    ui.horizontal(|ui| {
                        let width = (available - 132.0).clamp(145.0, 550.0);
                        fields::search_field(ui, self.query, width);
                        rescan = ui
                            .add_enabled_ui(!self.scanning, |ui| {
                                icons::button(ui, Icon::RefreshCw, "Rescan", None)
                            })
                            .inner
                            .clicked();
                    });
                } else {
                    fields::search_field(ui, self.query, available);
                    rescan = ui
                        .add_enabled_ui(!self.scanning, |ui| {
                            icons::button(ui, Icon::RefreshCw, "Rescan", None)
                        })
                        .inner
                        .clicked();
                }
                ui.add_space(9.0);
                ui.horizontal(|ui| {
                    if self.scanning {
                        ui.add(egui::Spinner::new().size(15.0));
                        ui.label(
                            RichText::new("Scanning installed applications…")
                                .size(12.0)
                                .color(fields::ACCENT),
                        );
                    } else if self.complete {
                        icons::inline(ui, Icon::CheckCircle, 15.0, fields::ACCENT);
                        let message = if self.query.trim().is_empty() {
                            format!("{} applications found", self.total)
                        } else {
                            format!("{} matching of {} applications", self.visible, self.total)
                        };
                        ui.label(RichText::new(message).size(12.0).color(fields::MUTED));
                    } else {
                        ui.label(
                            RichText::new("Choose Rescan to discover installed applications.")
                                .size(12.0)
                                .color(fields::MUTED),
                        );
                    }
                });
                ToolbarAction { rescan }
            })
            .inner
    }
}

pub fn compatibility_note(ui: &mut Ui) {
    #[cfg(target_os = "windows")]
    let message = "Windows: registered installed programs, plus known browser executables. Microsoft Store/MSIX applications are not yet included. Scanning never launches programs.";
    #[cfg(target_os = "macos")]
    let message = "macOS: application bundles from system and user Applications folders. Scanning never launches programs.";
    #[cfg(target_os = "linux")]
    let message = "Linux: desktop application entries and common AppImage locations. Flatpak/Snap containers are not yet included.";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let message = "Local discovery is best effort and does not launch applications.";
    fields::helper(ui, message);
}

fn app_details(ui: &mut Ui, app: &DiscoveredApp) {
    ui.add(
        Label::new(
            RichText::new(&app.name)
                .size(15.0)
                .strong()
                .color(fields::INK),
        )
        .wrap(),
    );
    ui.horizontal(|ui| {
        let icon = match app.support {
            Support::ProfileAdapter => Icon::UserRoundCheck,
            Support::CopyCandidate => Icon::Package,
            Support::ReviewRequired => Icon::AlertTriangle,
        };
        icons::inline(ui, icon, 16.0, fields::ACCENT);
        ui.label(
            RichText::new(app.support.label())
                .size(12.0)
                .color(fields::ACCENT),
        );
    });
    fields::helper(ui, app.origin);
    fields::helper(ui, &app.note);
    if !app.path.as_os_str().is_empty() {
        ui.add(
            Label::new(
                RichText::new(app.path.display().to_string())
                    .size(11.0)
                    .color(fields::MUTED),
            )
            .wrap(),
        );
    }
}

fn choose_button(ui: &mut Ui, app: &DiscoveredApp) -> bool {
    ui.add_enabled(app.launchable, egui::Button::new("Use this app"))
        .on_disabled_hover_text(
            "No safe executable was resolved. Select a path manually in Create clone.",
        )
        .clicked()
}

pub fn app_card(ui: &mut Ui, app: &DiscoveredApp) -> bool {
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, fields::BORDER))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(17))
        .show(ui, |ui| {
            let width = ui.available_width();
            if width >= 560.0 {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width((width - 155.0).max(220.0));
                        app_details(ui, app);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        choose_button(ui, app)
                    })
                    .inner
                })
                .inner
            } else {
                ui.vertical(|ui| {
                    app_details(ui, app);
                    ui.add_space(8.0);
                    choose_button(ui, app)
                })
                .inner
            }
        })
        .inner
}
