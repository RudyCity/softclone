//! Installed-app search and scan controls.
use super::fields;
use crate::icons::{self, Icon};
use eframe::egui::{self, Color32, Frame, RichText, Ui};

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
                ui.set_width(ui.available_width());
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
                        let width = (available - 132.0).min(550.0).max(145.0);
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
