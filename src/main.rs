// No console window behind the installer.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod catalog;
mod detect;
mod games;
mod images;
mod motion;
mod page;
mod patch;
mod retro;
mod reviews;
mod theme;
mod video;
mod voices;

fn main() -> eframe::Result<()> {
    let icon = image::load_from_memory(include_bytes!("../assets/icon128.png")).ok().map(|image| {
        let image = image.to_rgba8();
        eframe::egui::IconData { width: image.width(), height: image.height(), rgba: image.into_raw() }
    });
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Installateur du Liberl News")
        .with_inner_size([1440.0, 820.0])
        .with_min_inner_size([980.0, 640.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Installateur du Liberl News", options, Box::new(|context| Ok(Box::new(app::App::new(context)))))
}
