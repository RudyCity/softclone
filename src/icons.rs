//! Resolution-independent line icons. No icon font or OS glyph fallback.
use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

#[derive(Clone, Copy, Debug)]
pub enum Icon {
    Layers,
    Copy,
    SquarePlus,
    ScanSearch,
    Info,
    HardDrive,
    Play,
    FolderOpen,
    RefreshCw,
    Search,
    UserRoundCheck,
    AlertTriangle,
    Package,
    CheckCircle,
    CircleAlert,
    Activity,
    Heart,
}

pub fn inline(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    draw(ui.painter(), icon, rect.center(), size, color);
    response
}

pub fn button(ui: &mut Ui, icon: Icon, label: &str, accent: Option<Color32>) -> Response {
    let width = 43.0 + label.chars().count() as f32 * 7.3;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 35.0), Sense::click());
    let color = if accent.is_some() {
        Color32::WHITE
    } else {
        Color32::from_rgb(56, 67, 72)
    };
    let background = accent.unwrap_or(if response.hovered() {
        Color32::from_rgb(231, 238, 235)
    } else {
        Color32::from_rgb(246, 248, 247)
    });
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(8), background);
    if accent.is_none() {
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(8),
            Stroke::new(1.0_f32, Color32::from_rgb(219, 228, 225)),
            StrokeKind::Inside,
        );
    }
    draw(
        painter,
        icon,
        egui::pos2(rect.left() + 18.0, rect.center().y),
        16.0,
        color,
    );
    painter.text(
        egui::pos2(rect.left() + 33.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.5),
        color,
    );
    response
}

pub fn draw(painter: &egui::Painter, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let scale = size / 24.0;
    let at = |x: f32, y: f32| -> Pos2 {
        egui::pos2(center.x + (x - 12.0) * scale, center.y + (y - 12.0) * scale)
    };
    let stroke = Stroke::new((1.8 * scale).max(1.15), color);
    let line = |points: &[(f32, f32)]| {
        for pair in points.windows(2) {
            painter.line_segment([at(pair[0].0, pair[0].1), at(pair[1].0, pair[1].1)], stroke);
        }
    };
    let rect = |x: f32, y: f32, w: f32, h: f32, radius: u8| {
        painter.rect_stroke(
            Rect::from_min_max(at(x, y), at(x + w, y + h)),
            egui::CornerRadius::same(radius),
            stroke,
            StrokeKind::Inside,
        );
    };
    let circle = |x: f32, y: f32, radius: f32| {
        painter.circle_stroke(at(x, y), radius * scale, stroke);
    };
    match icon {
        Icon::Layers => {
            line(&[
                (3.0, 9.0),
                (12.0, 4.0),
                (21.0, 9.0),
                (12.0, 14.0),
                (3.0, 9.0),
            ]);
            line(&[(3.0, 13.0), (12.0, 18.0), (21.0, 13.0)]);
            line(&[(3.0, 17.0), (12.0, 22.0), (21.0, 17.0)]);
        }
        Icon::Copy => {
            rect(8.0, 8.0, 13.0, 13.0, 2);
            line(&[
                (5.0, 17.0),
                (4.0, 17.0),
                (3.0, 16.0),
                (3.0, 4.0),
                (4.0, 3.0),
                (16.0, 3.0),
                (17.0, 4.0),
                (17.0, 5.0),
            ]);
        }
        Icon::SquarePlus => {
            rect(3.0, 3.0, 18.0, 18.0, 3);
            line(&[(12.0, 8.0), (12.0, 16.0)]);
            line(&[(8.0, 12.0), (16.0, 12.0)]);
        }
        Icon::ScanSearch => {
            line(&[(4.0, 9.0), (4.0, 5.0), (9.0, 5.0)]);
            line(&[(15.0, 5.0), (20.0, 5.0), (20.0, 9.0)]);
            line(&[(4.0, 15.0), (4.0, 19.0), (9.0, 19.0)]);
            circle(12.0, 12.0, 4.0);
            line(&[(15.0, 15.0), (21.0, 21.0)]);
        }
        Icon::Info => {
            circle(12.0, 12.0, 9.0);
            painter.circle_filled(at(12.0, 8.0), 1.15 * scale, color);
            line(&[(12.0, 11.0), (12.0, 16.0)]);
        }
        Icon::HardDrive => {
            rect(3.0, 5.0, 18.0, 14.0, 3);
            line(&[(3.0, 14.0), (21.0, 14.0)]);
            painter.circle_filled(at(7.0, 17.0), 1.0 * scale, color);
            painter.circle_filled(at(11.0, 17.0), 1.0 * scale, color);
        }
        Icon::Play => {
            line(&[(8.0, 5.0), (19.0, 12.0), (8.0, 19.0), (8.0, 5.0)]);
        }
        Icon::FolderOpen => {
            line(&[
                (3.0, 18.0),
                (3.0, 6.0),
                (10.0, 6.0),
                (12.0, 9.0),
                (20.0, 9.0),
                (20.0, 12.0),
            ]);
            line(&[
                (3.0, 18.0),
                (6.0, 13.0),
                (22.0, 13.0),
                (19.0, 20.0),
                (5.0, 20.0),
                (3.0, 18.0),
            ]);
        }
        Icon::RefreshCw => {
            line(&[(20.0, 7.0), (20.0, 3.0), (16.0, 3.0)]);
            line(&[
                (20.0, 7.0),
                (17.0, 5.0),
                (13.0, 4.0),
                (9.0, 5.0),
                (6.0, 8.0),
            ]);
            line(&[(4.0, 17.0), (4.0, 21.0), (8.0, 21.0)]);
            line(&[
                (4.0, 17.0),
                (7.0, 19.0),
                (11.0, 20.0),
                (15.0, 19.0),
                (18.0, 16.0),
            ]);
        }
        Icon::Search => {
            circle(10.5, 10.5, 6.5);
            line(&[(15.5, 15.5), (21.0, 21.0)]);
        }
        Icon::UserRoundCheck => {
            circle(9.0, 8.0, 4.0);
            line(&[
                (2.0, 21.0),
                (2.0, 18.0),
                (4.0, 15.0),
                (9.0, 14.0),
                (13.0, 15.0),
            ]);
            line(&[(15.0, 18.0), (18.0, 21.0), (23.0, 15.0)]);
        }
        Icon::AlertTriangle => {
            line(&[(12.0, 3.0), (22.0, 20.0), (2.0, 20.0), (12.0, 3.0)]);
            line(&[(12.0, 9.0), (12.0, 14.0)]);
            painter.circle_filled(at(12.0, 17.0), 1.0 * scale, color);
        }
        Icon::Package => {
            line(&[
                (12.0, 2.0),
                (21.0, 7.0),
                (21.0, 17.0),
                (12.0, 22.0),
                (3.0, 17.0),
                (3.0, 7.0),
                (12.0, 2.0),
            ]);
            line(&[(3.0, 7.0), (12.0, 12.0), (21.0, 7.0)]);
            line(&[(12.0, 12.0), (12.0, 22.0)]);
        }
        Icon::CheckCircle => {
            circle(12.0, 12.0, 9.0);
            line(&[(7.0, 12.0), (10.5, 15.0), (17.0, 9.0)]);
        }
        Icon::CircleAlert => {
            circle(12.0, 12.0, 9.0);
            line(&[(12.0, 7.0), (12.0, 13.0)]);
            painter.circle_filled(at(12.0, 17.0), 1.1 * scale, color);
        }
        Icon::Activity => {
            line(&[(2.0, 12.0), (6.0, 12.0), (9.0, 5.0), (13.0, 20.0), (16.0, 10.0), (19.0, 12.0), (22.0, 12.0)]);
        }
        Icon::Heart => {
            line(&[(12.0, 21.0), (3.0, 12.0), (3.0, 7.0), (6.0, 4.0), (9.0, 4.0), (12.0, 7.0), (15.0, 4.0), (18.0, 4.0), (21.0, 7.0), (21.0, 12.0), (12.0, 21.0)]);
        }
    }
}
