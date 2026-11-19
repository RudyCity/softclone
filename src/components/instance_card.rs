//! Instance cards keep lifecycle controls separate from persistence and process IO.
use crate::icons::{self, Icon};
use eframe::egui::{self, Color32, Frame, Label, RichText, Stroke, Ui};
use softclone::core::{CloneRecord, RunMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Launch,
    Stop,
    Restart,
    OpenFolder,
    Rename,
    Duplicate,
    Backup,
    Restore,
    Delete,
}

const INK: Color32 = Color32::from_rgb(24, 34, 43);
const MUTED: Color32 = Color32::from_rgb(112, 123, 132);
const ACCENT: Color32 = Color32::from_rgb(40, 105, 96);
const LINE: Color32 = Color32::from_rgb(221, 228, 225);

pub fn show(
    ui: &mut Ui,
    instance: &CloneRecord,
    running_pid: Option<u32>,
    busy: bool,
) -> Option<Action> {
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(12)
        .inner_margin(18)
        .show(ui, |ui| {
            let mut result = None;
            ui.horizontal_wrapped(|ui| {
                Frame::new()
                    .fill(Color32::from_rgb(232, 244, 238))
                    .corner_radius(10)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        icons::inline(ui, Icon::Copy, 24.0, ACCENT);
                    });
                ui.vertical(|ui| {
                    ui.add(
                        Label::new(RichText::new(&instance.name).size(16.0).strong().color(INK))
                            .wrap(),
                    );
                    let kind = if instance.run_mode == RunMode::ProfileOnly {
                        "Installed app · profile-only"
                    } else if instance.isolated_profile {
                        "Portable copy · separate profile"
                    } else {
                        "Portable copy"
                    };
                    ui.label(
                        RichText::new(format!("{} · {kind}", format_bytes(instance.copied_bytes)))
                            .size(12.0)
                            .color(MUTED),
                    );
                    ui.add(
                        Label::new(
                            RichText::new(instance.source.display().to_string())
                                .size(11.0)
                                .color(MUTED),
                        )
                        .wrap(),
                    );
                    ui.label(
                        RichText::new(match running_pid {
                            Some(pid) => format!("Managed process running · PID {pid}"),
                            None => "No managed process tracked".into(),
                        })
                        .size(11.0)
                        .color(if running_pid.is_some() {
                            ACCENT
                        } else {
                            MUTED
                        }),
                    );
                });
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                let mut choose = |action, icon, label, accent| {
                    if ui
                        .add_enabled_ui(!busy, |ui| icons::button(ui, icon, label, accent))
                        .inner
                        .clicked()
                    {
                        result = Some(action);
                    }
                };
                if running_pid.is_some() {
                    choose(Action::Stop, Icon::CircleAlert, "Stop", None);
                    choose(Action::Restart, Icon::RefreshCw, "Restart", Some(ACCENT));
                } else {
                    choose(Action::Launch, Icon::Play, "Launch", Some(ACCENT));
                }
                choose(Action::OpenFolder, Icon::FolderOpen, "Folder", None);
                choose(Action::Rename, Icon::Info, "Rename", None);
                choose(Action::Duplicate, Icon::Copy, "Duplicate", None);
                choose(Action::Backup, Icon::Package, "Backup", None);
                choose(Action::Restore, Icon::RefreshCw, "Restore", None);
                choose(Action::Delete, Icon::AlertTriangle, "Delete", None);
            });
            result
        })
        .inner
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut amount = bytes as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit < UNITS.len() - 1 {
        amount /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{amount:.1} {}", UNITS[unit])
    }
}
