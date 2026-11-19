mod components;
mod icons;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([840.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "SoftClone",
        options,
        Box::new(|cc| Ok(Box::new(ui::DesktopApp::new(cc)))),
    )
}
