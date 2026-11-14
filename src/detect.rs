//! Finds a game's folder: Steam libraries (every one listed in
//! libraryfolders.vdf), then GOG's registry entry.

use crate::games::Game;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Steam,
    Gog,
    Manual,
}

pub fn find(game: &Game) -> Option<(PathBuf, Store)> {
    // To try an installation on a copy of the game (with LN_PATCH_DIR).
    if let Some(folder) = std::env::var_os("LN_GAME_DIR").map(PathBuf::from) {
        if is_game(game, &folder) {
            return Some((folder, Store::Manual));
        }
    }
    for library in steam_libraries() {
        let common = library.join("steamapps").join("common");
        let folder = installdir(&library, game.steam_id)
            .map(|name| common.join(name))
            .unwrap_or_else(|| common.join(game.steam_folder));
        if is_game(game, &folder) {
            return Some((folder, Store::Steam));
        }
    }
    if let Some(folder) = gog_folder(game.gog_id).filter(|folder| is_game(game, folder)) {
        return Some((folder, Store::Gog));
    }
    None
}

pub fn is_game(game: &Game, folder: &Path) -> bool {
    folder.join(game.marker).is_file()
}

/// Steam's install folder and every library it knows.
pub fn steam_libraries() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(steam) = steam_root() {
        roots.push(steam);
    }
    let mut libraries = Vec::new();
    for root in &roots {
        libraries.push(root.clone());
        let vdf = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")).unwrap_or_default();
        for line in vdf.lines() {
            let parts: Vec<&str> = line.split('"').collect();
            // 	"path"		"D:\\SteamLibrary"
            if parts.len() >= 4 && parts[1] == "path" {
                let path = PathBuf::from(parts[3].replace("\\\\", "\\"));
                if !libraries.contains(&path) {
                    libraries.push(path);
                }
            }
        }
    }
    libraries
}

/// The folder name Steam gave the game (appmanifest_<id>.acf, "installdir").
fn installdir(library: &Path, app: u32) -> Option<String> {
    let acf = std::fs::read_to_string(library.join("steamapps").join(format!("appmanifest_{app}.acf"))).ok()?;
    acf.lines().find_map(|line| {
        let parts: Vec<&str> = line.split('"').collect();
        (parts.len() >= 4 && parts[1] == "installdir").then(|| parts[3].to_string())
    })
}

#[cfg(windows)]
fn steam_root() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    // InstallPath keeps Windows' spelling; SteamPath (per user) is lowercase with "/".
    let machine: Option<String> = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\WOW6432Node\\Valve\\Steam").and_then(|key| key.get_value("InstallPath")).ok();
    let user = || -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam").and_then(|key| key.get_value::<String, _>("SteamPath")).ok().map(|path| path.replace('/', "\\"))
    };
    machine.filter(|path| PathBuf::from(path).is_dir()).or_else(user).map(PathBuf::from).filter(|path| path.is_dir())
}

#[cfg(not(windows))]
fn steam_root() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    // Steam Deck and most distributions, then the Flatpak build.
    [".steam/steam", ".local/share/Steam", ".var/app/com.valvesoftware.Steam/.local/share/Steam"].iter().map(|sub| home.join(sub)).find(|path| path.is_dir())
}

#[cfg(windows)]
fn gog_folder(id: u64) -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    if id == 0 {
        return None;
    }
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(format!("SOFTWARE\\WOW6432Node\\GOG.com\\Games\\{id}")).ok()?;
    key.get_value::<String, _>("path").ok().map(PathBuf::from)
}

#[cfg(not(windows))]
fn gog_folder(_id: u64) -> Option<PathBuf> {
    None
}
