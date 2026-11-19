//! Clone creation is composed from independent source/details/strategy sections.
use super::fields;
use crate::icons::{self, Icon};
use eframe::egui::{self, Color32, Frame, Label, RichText, Ui};
use softclone::{rune_engine, universal};
use std::path::Path;

pub struct CreateForm<'a> {
    pub source: &'a mut String,
    pub name: &'a mut String,
    pub entrypoint: &'a mut String,
    pub isolated: &'a mut bool,
    pub profile_only: &'a mut bool,
    pub next_label_number: i64,
    pub creating: bool,
}

#[derive(Default)]
pub struct CreateFormAction {
    pub submit: bool,
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
                action.submit = self.actions(ui);
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
            let message = if universal::adapter_for(Path::new(self.source.trim())).is_some() {
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

    fn actions(&self, ui: &mut Ui) -> bool {
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
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(18.0));
                ui.label(
                    RichText::new("Preparing instance…")
                        .color(fields::ACCENT)
                        .size(12.0),
                );
            });
            false
        } else {
            let ready = !self.source.trim().is_empty() && !self.name.trim().is_empty();
            let response = ui
                .add_enabled_ui(ready, |ui| {
                    icons::button(ui, Icon::SquarePlus, "Create clone", Some(fields::ACCENT))
                })
                .inner;
            if !ready {
                fields::helper(
                    ui,
                    "Enter an application path and a clone name to continue.",
                );
            }
            response.clicked()
        }
    }
}
