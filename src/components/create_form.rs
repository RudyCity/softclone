//! Clone creation is composed from independent source/details/strategy sections.
use super::fields;
use crate::icons::{self, Icon};
use eframe::egui::{self, Color32, Frame, Label, RichText, Ui};
use softclone::{
    core::{CopyPlan, CopyProgress},
    rune_engine, universal,
};
use std::path::Path;

pub struct CreateForm<'a> {
    pub source: &'a mut String,
    pub name: &'a mut String,
    pub entrypoint: &'a mut String,
    pub isolated: &'a mut bool,
    pub profile_only: &'a mut bool,
    pub next_label_number: i64,
    pub creating: bool,
    pub analyzing: bool,
    pub analysis: Option<&'a CopyPlan>,
    pub progress: Option<&'a CopyProgress>,
    pub cancellation_requested: bool,
}

#[derive(Default)]
pub struct CreateFormAction {
    pub submit: bool,
    pub analyze: bool,
    pub cancel: bool,
    pub browse_file: bool,
    pub browse_folder: bool,
    pub error: Option<String>,
}

impl CreateForm<'_> {
    pub fn show(mut self, ui: &mut Ui) -> CreateFormAction {
        Frame::new()
            .fill(Color32::WHITE)
            .stroke(egui::Stroke::new(1.0_f32, fields::BORDER))
            .corner_radius(12)
            .inner_margin(egui::Margin::same(24))
            .show(ui, |ui| {
                ui.set_max_width(740.0);
                let mut action = CreateFormAction::default();
                let (browse_file, browse_folder) = self.source_section(ui);
                action.browse_file = browse_file;
                action.browse_folder = browse_folder;
                fields::divider(ui);
                action.error = self.details_section(ui);
                fields::divider(ui);
                self.strategy_section(ui);
                fields::divider(ui);
                self.preflight_section(ui, &mut action);
                fields::divider(ui);
                self.actions(ui, &mut action);
                action
            })
            .inner
    }

    fn source_section(&mut self, ui: &mut Ui) -> (bool, bool) {
        fields::section(ui, "01", "SOURCE APPLICATION");
        fields::text_field(
            ui,
            "Application path",
            "C:\\Program Files\\Example\\Example.exe",
            Some("Choose an executable, portable folder, AppImage, or macOS .app bundle."),
            self.source,
        );
        let mut browse_file = false;
        let mut browse_folder = false;
        ui.horizontal_wrapped(|ui| {
            browse_file = icons::button(ui, Icon::FolderOpen, "Choose executable", None).clicked();
            browse_folder = icons::button(ui, Icon::FolderOpen, "Choose folder", None).clicked();
        });
        (browse_file, browse_folder)
    }

    fn details_section(&mut self, ui: &mut Ui) -> Option<String> {
        fields::section(ui, "02", "INSTANCE DETAILS");
        ui.label(
            RichText::new("Clone name")
                .size(14.0)
                .color(fields::INK)
                .strong(),
        );
        let mut error = None;
        if ui.button("Suggest with Rune").clicked() {
            let path = Path::new(self.source.trim());
            let base = path
                .file_stem()
                .or_else(|| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "My app".into());
            match rune_engine::clone_label(&base, self.next_label_number) {
                Ok(name) => *self.name = name,
                Err(err) => error = Some(format!("Rune naming rule failed: {err}")),
            }
        }
        fields::text_field(ui, "", "e.g. Work browser", None, self.name);
        ui.add_space(9.0);
        fields::text_field(
            ui,
            "Entrypoint inside folder (optional)",
            "Auto-detect or e.g. bin/app",
            Some("Specify only if the selected folder contains multiple executables."),
            self.entrypoint,
        );
        error
    }

    fn strategy_section(&mut self, ui: &mut Ui) {
        fields::section(ui, "03", "INSTANCE STRATEGY");
        ui.radio_value(
            self.profile_only,
            false,
            "Copy application files (portable applications)",
        );
        ui.radio_value(
            self.profile_only,
            true,
            "Use installed application with a separate profile",
        );
        ui.add_space(3.0);
        if *self.profile_only {
            *self.isolated = true;
            let adapter = universal::adapter_for(Path::new(self.source.trim()));
            let message = if adapter == Some(universal::Adapter::ZoomWindows) {
                "Zoom Windows: each instance uses the officially documented --multipt=TRUE and a distinct --data=data... selector. Existing profiles work after updating SoftClone."
            } else if adapter.is_some() {
                "Known browser adapter: dedicated command-line profile arguments are used."
            } else {
                "Experimental: this application may ignore profile paths and share settings with the original."
            };
            fields::helper(ui, message);
        } else {
            ui.checkbox(self.isolated, "Create separate per-clone data directories");
            fields::helper(
                ui,
                "Best effort via environment variables; this is not an OS security sandbox.",
            );
        }
    }

    fn preflight_section(&self, ui: &mut Ui, action: &mut CreateFormAction) {
        fields::section(ui, "04", "PREFLIGHT CHECK");
        fields::helper(
            ui,
            "Inspect the selected source before copying. File counts and sizes are estimates; files can change before or during the clone.",
        );
        let source_ready = !self.source.trim().is_empty() && !self.name.trim().is_empty();
        if self.analyzing {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label(RichText::new("Analyzing source safely…").color(fields::ACCENT));
            });
        } else if !self.creating {
            action.analyze = ui
                .add_enabled_ui(source_ready, |ui| {
                    icons::button(ui, Icon::Search, "Analyze source", None)
                })
                .inner
                .clicked();
        }
        if let Some(plan) = self.analysis {
            if *self.profile_only {
                ui.label(
                    RichText::new("Installed profile: no application binary copy required.")
                        .color(fields::ACCENT)
                        .strong(),
                );
            } else {
                ui.label(
                    RichText::new(format!(
                        "Estimated copy: {} files · {}",
                        plan.total_files,
                        format_size(plan.total_bytes)
                    ))
                    .color(fields::ACCENT)
                    .strong(),
                );
            }
        }
    }

    fn actions(&self, ui: &mut Ui, action: &mut CreateFormAction) {
        ui.add(
            Label::new(
                RichText::new(
                    "Instance metadata and profiles stay in your local SoftClone library.",
                )
                .size(12.0)
                .color(fields::MUTED),
            )
            .wrap(),
        );
        ui.add_space(8.0);
        if self.creating {
            if let Some(progress) = self.progress {
                let fraction = if progress.total_bytes == 0 {
                    0.0
                } else {
                    (progress.copied_bytes as f64 / progress.total_bytes as f64).clamp(0.0, 1.0)
                        as f32
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .show_percentage()
                        .animate(true)
                        .desired_width(ui.available_width().min(590.0)),
                );
                fields::helper(
                    ui,
                    &format!(
                        "{} of {} · {} / {} files · {}",
                        format_size(progress.copied_bytes),
                        format_size(progress.total_bytes),
                        progress.completed_files,
                        progress.total_files,
                        progress.current_file,
                    ),
                );
            } else {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(18.0));
                    ui.label(RichText::new("Preparing instance…").color(fields::ACCENT));
                });
            }
            action.cancel = ui
                .add_enabled(
                    !self.cancellation_requested,
                    egui::Button::new(if self.cancellation_requested {
                        "Cancellation requested…"
                    } else {
                        "Cancel and remove temporary files"
                    }),
                )
                .clicked();
        } else {
            let ready = !self.source.trim().is_empty() && !self.name.trim().is_empty();
            action.submit = ui
                .add_enabled_ui(ready && !self.analyzing, |ui| {
                    icons::button(ui, Icon::SquarePlus, "Create clone", Some(fields::ACCENT))
                })
                .inner
                .clicked();
            if !ready {
                fields::helper(
                    ui,
                    "Enter an application path and a clone name to continue.",
                );
            }
        }
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut n = bytes as f64;
    let mut unit = 0;
    while n >= 1024.0 && unit < UNITS.len() - 1 {
        n /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{n:.1} {}", UNITS[unit])
    }
}
