//! The Japanese voices (SoraVoice): `dsound.dll` (the loader the game picks up
//! from its folder) and `voice/` (ed_voice.dll, its settings, the .ogg files).
//! They live in the project's private repository as a release tagged "voix",
//! a few zips each under GitHub's 2 GB limit; the platform hands out short-lived
//! links (patch.php?channel=voices), as for the patch. A project without that
//! release simply has no voices yet.
//!
//! Under Linux (Steam Deck), Proton loads its own dsound.dll unless told
//! otherwise: the game's prefix gets `"dsound"="native,builtin"`.

use crate::games::Game;
use crate::patch::{self, API, Shared};
use serde::Deserialize;
use std::fs::{self, File};
use std::io;
use std::path::Path;

#[derive(Clone, Deserialize)]
pub struct VoiceFile {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Clone, Deserialize)]
pub struct VoiceRelease {
    pub version: String,
    pub files: Vec<VoiceFile>,
}

impl VoiceRelease {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }
}

#[derive(Clone, PartialEq)]
pub enum VoiceState {
    Missing,
    /// The voice folder of an older install, without the loader: voices are silent.
    Incomplete,
    Installed(String),
}

const VERSION_FILE: &str = "version-voix.txt";

fn url(project: &str) -> String {
    format!("{API}?project={project}&channel=voices")
}

/// The voices the platform serves for a project; None when there are none yet.
pub fn release(project: &str) -> Result<Option<VoiceRelease>, String> {
    let response = ureq::get(&url(project)).header("User-Agent", "InstallateurLiberlNews").config().http_status_as_error(false).build().call();
    let response = response.map_err(|error| error.to_string())?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let text = response.into_body().read_to_string().map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map(Some).map_err(|error| error.to_string())
}

pub fn state(folder: &Path) -> VoiceState {
    let voice = folder.join("voice");
    if !voice.join("ed_voice.dll").is_file() {
        return VoiceState::Missing;
    }
    if !folder.join("dsound.dll").is_file() {
        return VoiceState::Incomplete;
    }
    let version = fs::read_to_string(voice.join(VERSION_FILE)).map(|text| text.trim().to_string()).unwrap_or_default();
    VoiceState::Installed(if version.is_empty() { "installées".into() } else { version })
}

pub fn install(game: &Game, folder: &Path, voices: &VoiceRelease, progress: &Shared) -> Result<String, String> {
    let work = patch::data_dir().join("voix").join(game.project);
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    let count = voices.files.len() as u64;
    for (index, file) in voices.files.iter().enumerate() {
        {
            let mut state = progress.lock().unwrap();
            state.file = index as u64;
            state.files = count;
        }
        let step = format!("{} ({}/{})", file.name, index + 1, count);
        let archive = work.join(&file.name);
        if file.sha256.is_empty() || patch::sha256(&archive) != file.sha256 {
            patch::download(&format!("{}&asset={}", url(game.project), file.name), &archive, file.size, &step, progress)?;
            if !file.sha256.is_empty() && patch::sha256(&archive) != file.sha256 {
                let _ = fs::remove_file(&archive);
                return Err(format!("{} : fichier incomplet ou altéré, réessayez", file.name));
            }
        }
        extract(&archive, folder, &step, progress)?;
        let _ = fs::remove_file(&archive);
    }
    fs::write(folder.join("voice").join(VERSION_FILE), format!("{}\n", voices.version)).map_err(|error| error.to_string())?;
    #[cfg_attr(windows, allow(unused_mut))]
    let mut message = format!("Voix {} installées.", voices.version);
    #[cfg(not(windows))]
    match proton_override(game) {
        Ok(true) => {}
        Ok(false) => message.push_str(" Lancez le jeu une fois depuis Steam, puis réinstallez les voix pour que Proton les charge."),
        Err(error) => message.push_str(&format!(" Réglage de Proton impossible ({error}) : ajoutez WINEDLLOVERRIDES=\"dsound=n,b\" %command% aux options de lancement du jeu.")),
    }
    #[cfg(windows)]
    let _ = game;
    Ok(message)
}

/// Unpacks a voice zip into the game folder (paths kept inside it).
fn extract(archive: &Path, folder: &Path, step: &str, progress: &Shared) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(File::open(archive).map_err(|error| error.to_string())?).map_err(|error| format!("{step} : {error}"))?;
    let total = zip.len() as u64;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
        let Some(relative) = entry.enclosed_name() else { continue };
        let target = folder.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut output = File::create(&target).map_err(|error| format!("{} : {error}", target.display()))?;
            io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        }
        if index % 64 == 0 || index as u64 + 1 == total {
            patch::report(progress, "Extraction", step, index as u64 + 1, total);
        }
    }
    Ok(())
}

pub fn uninstall(folder: &Path) -> Result<String, String> {
    let _ = fs::remove_file(folder.join("dsound.dll"));
    let voice = folder.join("voice");
    if voice.exists() {
        fs::remove_dir_all(&voice).map_err(|error| format!("voice : {error}"))?;
    }
    Ok("Voix retirées.".into())
}

/// Tells the game's Proton prefix to load the game's dsound.dll first. False:
/// no prefix yet (the game never ran under Proton).
#[cfg(not(windows))]
pub fn proton_override(game: &Game) -> Result<bool, String> {
    let Some(registry) = crate::detect::steam_libraries()
        .into_iter()
        .map(|library| library.join("steamapps/compatdata").join(game.steam_id.to_string()).join("pfx/user.reg"))
        .find(|path| path.is_file())
    else {
        return Ok(false);
    };
    let text = fs::read_to_string(&registry).map_err(|error| error.to_string())?;
    let updated = with_override(&text);
    if updated != text {
        fs::write(&registry, updated).map_err(|error| error.to_string())?;
    }
    Ok(true)
}

/// user.reg with "dsound"="native,builtin" in [Software\\Wine\\DllOverrides].
#[cfg_attr(windows, allow(dead_code))]
fn with_override(text: &str) -> String {
    const KEY: &str = "[Software\\\\Wine\\\\DllOverrides]";
    const LINE: &str = "\"dsound\"=\"native,builtin\"";
    let Some(start) = text.find(KEY) else {
        return format!("{}\n\n{KEY} 0\n{LINE}\n", text.trim_end());
    };
    let end = text[start + KEY.len()..].find("\n[").map(|at| start + KEY.len() + at).unwrap_or(text.len());
    let block = &text[start..end];
    if block.lines().any(|line| line.starts_with("\"dsound\"=")) {
        let replaced: Vec<String> = block.lines().map(|line| if line.starts_with("\"dsound\"=") { LINE.to_string() } else { line.to_string() }).collect();
        let tail = if block.ends_with('\n') { "\n" } else { "" };
        return format!("{}{}{tail}{}", &text[..start], replaced.join("\n"), &text[end..]);
    }
    format!("{}{}\n{LINE}\n{}", &text[..start], block.trim_end(), &text[end..])
}

#[cfg(test)]
mod tests {
    use super::with_override;

    #[test]
    fn adds_the_override_once() {
        let prefix = "WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\DllOverrides] 1696584635\n\"d3d9\"=\"native\"\n\n[Software\\\\Other] 1\n\"a\"=\"b\"\n";
        let once = with_override(prefix);
        assert!(once.contains("\"d3d9\"=\"native\"\n\"dsound\"=\"native,builtin\"\n"));
        assert!(once.contains("[Software\\\\Other] 1"));
        assert_eq!(with_override(&once), once);
        let bare = with_override("WINE REGISTRY Version 2\n");
        assert!(bare.ends_with("[Software\\\\Wine\\\\DllOverrides] 0\n\"dsound\"=\"native,builtin\"\n"));
    }
}
