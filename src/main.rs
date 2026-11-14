// No console window behind the installer.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod catalog;
mod detect;
mod dsretrack;
mod games;
mod images;
mod motion;
mod page;
mod patch;
mod retro;
mod reviews;
mod theme;
mod update;
mod video;
mod voices;

/// The window's identity: under Wayland (Steam Deck's desktop, GNOME, KDE) the window's icon is
/// the one of the .desktop file of the same name, not one the program hands over.
const APP_ID: &str = "fr.leliberlnews.Installateur";

/// Linux: a binary carries no icon of its own. The installer declares itself to the desktop
/// (~/.local/share/applications and its icon), so its window, the taskbar and the menu show the
/// Liberl News icon; written again when the executable moved.
#[cfg(target_os = "linux")]
fn desktop_entry() {
    let Some(data) = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".local/share"))) else { return };
    let Ok(exe) = std::env::current_exe() else { return };
    let icon_dir = data.join("icons/hicolor/128x128/apps");
    let icon = icon_dir.join(format!("{APP_ID}.png"));
    if !icon.exists() && std::fs::create_dir_all(&icon_dir).is_ok() {
        let _ = std::fs::write(&icon, include_bytes!("../assets/icon128.png"));
    }
    let entry = format!(
        "[Desktop Entry]
Type=Application
Name=Installateur du Liberl News
Comment=Les patchs de traduction du Liberl News
Exec=\"{}\"
Icon={APP_ID}
StartupWMClass={APP_ID}
Terminal=false
Categories=Game;
",
        exe.display()
    );
    let applications = data.join("applications");
    let file = applications.join(format!("{APP_ID}.desktop"));
    if std::fs::read_to_string(&file).ok().as_deref() != Some(entry.as_str()) && std::fs::create_dir_all(&applications).is_ok() {
        let _ = std::fs::write(&file, entry);
    }
}

fn main() -> eframe::Result<()> {
    let icon = image::load_from_memory(include_bytes!("../assets/icon128.png")).ok().map(|image| {
        let image = image.to_rgba8();
        eframe::egui::IconData { width: image.width(), height: image.height(), rgba: image.into_raw() }
    });
    #[cfg(target_os = "linux")]
    desktop_entry();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Installateur du Liberl News")
        .with_app_id(APP_ID)
        .with_inner_size([1440.0, 820.0])
        .with_min_inner_size([980.0, 640.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Installateur du Liberl News", options, Box::new(|context| Ok(Box::new(app::App::new(context)))))
}
