//! Shared, responsive form primitives used by discovery and clone creation.
use crate::icons::{self, Icon};
use eframe::egui::{self, Color32, Frame, Label, Response, RichText, Stroke, TextEdit, Ui, Vec2};

pub const INK: Color32 = Color32::from_rgb(24, 34, 43);
pub const MUTED: Color32 = Color32::from_rgb(112, 123, 132);
pub const ACCENT: Color32 = Color32::from_rgb(40, 105, 96);
pub const BORDER: Color32 = Color32::from_rgb(221, 228, 225);

// Match the shared icon-button height so search and Rescan align without
// visual top/bottom drift at Windows DPI scaling.
const SEARCH_TEXT_HEIGHT: f32 = 19.0;
const SEARCH_PADDING_Y: i8 = 8;
const SEARCH_CONTROL_HEIGHT: f32 = SEARCH_TEXT_HEIGHT + 2.0 * SEARCH_PADDING_Y as f32;

pub fn helper(ui: &mut Ui, text: &str) {
    ui.add(Label::new(RichText::new(text).size(12.0).color(MUTED)).wrap());
}

pub fn section(ui: &mut Ui, number: &str, heading: &str) {
    ui.label(
        RichText::new(format!("{number}  {heading}"))
            .color(ACCENT)
            .size(11.0)
            .strong(),
    );
    ui.add_space(8.0);
}

pub fn divider(ui: &mut Ui) {
    ui.add_space(18.0);
    ui.separator();
    ui.add_space(10.0);
}

pub fn text_field(
    ui: &mut Ui,
    label: &str,
    placeholder: &str,
    help_text: Option<&str>,
    value: &mut String,
) -> Response {
    if !label.is_empty() {
        ui.label(RichText::new(label).color(INK).size(14.0).strong());
    }
    let width = ui.available_width().clamp(80.0, 720.0);
    let response = ui.add_sized(
        Vec2::new(width, 38.0),
        TextEdit::singleline(value).hint_text(placeholder),
    );
    if let Some(help_text) = help_text {
        helper(ui, help_text);
    }
    response
}

/// Search icon and field share one visual/control surface. The icon never
/// becomes an unrelated, focusable item outside the input.
pub fn search_field(ui: &mut Ui, value: &mut String, width: f32) -> Response {
    let width = width.max(145.0);
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(9)
        .inner_margin(egui::Margin::symmetric(12, SEARCH_PADDING_Y))
        .show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.horizontal(|ui| {
                icons::inline(ui, Icon::Search, 18.0, MUTED);
                ui.add_sized(
                    Vec2::new((width - 66.0).max(80.0), SEARCH_TEXT_HEIGHT),
                    TextEdit::singleline(value)
                        .hint_text("Search installed applications")
                        .align(egui::Align2::LEFT_CENTER)
                        .frame(false),
                )
            })
            .inner
        })
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_form_palette_has_contrast() {
        assert_ne!(INK, MUTED);
        assert_ne!(ACCENT, BORDER);
    }

    #[test]
    fn search_field_matches_rescan_button_height() {
        // icons::button uses a 35pt control. Avoid center-shifted rows.
        assert_eq!(SEARCH_CONTROL_HEIGHT, 35.0);
    }
}
