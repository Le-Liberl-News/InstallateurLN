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
use crate::patch::{self, Shared};
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
    format!("{}?project={project}&channel=voices", patch::api())
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
    // Fetches one zip (unless it is already there, intact).
    let fetch = |file: &VoiceFile, step: &str, progress: &Shared| -> Result<std::path::PathBuf, String> {
        let archive = work.join(&file.name);
        if file.sha256.is_empty() || patch::sha256(&archive) != file.sha256 {
            patch::download(&format!("{}&asset={}", url(game.project), file.name), &archive, file.size, step, progress)?;
            if !file.sha256.is_empty() && patch::sha256(&archive) != file.sha256 {
                let _ = fs::remove_file(&archive);
                return Err(format!("{} : fichier incomplet ou altéré, réessayez", file.name));
            }
        }
        Ok(archive)
    };
    // Each transfer reports to the visible job. No hidden prefetch stuck at 50%.
    {
        let mut state = progress.lock().unwrap();
        state.files = count * 2;
        state.install_fraction = Some(0.0);
    }
        for (index, file) in voices.files.iter().enumerate() {
            {
                let mut state = progress.lock().unwrap();
                state.file = index as u64 * 2;
            }
            let step = format!("{} ({}/{})", file.name, index + 1, count);
            let archive = fetch(file, &step, progress)?;
            progress.lock().unwrap().file = index as u64 * 2 + 1;
            patch::report(progress, "Extraction", &step, 0, 0);
            extract(&archive, folder, &step, progress)?;
            progress.lock().unwrap().file = index as u64 * 2 + 2;
            patch::report(progress, "Extraction", &step, 0, 0);
        }
    // Retain complete checked archives if another part fails, for the next try.
    for file in &voices.files { let _ = fs::remove_file(work.join(&file.name)); }
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
/// Thousands of small files: written by several threads at once (on Windows each
/// new file costs a scan), each with its own reader of the zip; a file already
/// there with the same size and CRC is left as it is (a reinstall is near instant).
fn extract(archive: &Path, folder: &Path, step: &str, progress: &Shared) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    let open = || -> Result<zip::ZipArchive<io::BufReader<File>>, String> {
        let file = File::open(archive).map_err(|error| error.to_string())?;
        zip::ZipArchive::new(io::BufReader::with_capacity(1 << 16, file)).map_err(|error| format!("{step} : {error}"))
    };
    let mut zip = open()?;
    let total = zip.len();
    // The folders first, once.
    let mut folders = std::collections::BTreeSet::new();
    for index in 0..total {
        let entry = zip.by_index_raw(index).map_err(|error| error.to_string())?;
        if let Some(relative) = entry.enclosed_name() {
            let target = folder.join(relative);
            if entry.is_dir() {
                folders.insert(target);
            } else if let Some(parent) = target.parent() {
                folders.insert(parent.to_path_buf());
            }
        }
    }
    for path in &folders {
        fs::create_dir_all(path).map_err(|error| format!("{} : {error}", path.display()))?;
    }
    drop(zip);

    let threads = std::thread::available_parallelism().map(|count| count.get()).unwrap_or(4).clamp(2, 8);
    let done = AtomicU64::new(0);
    let failure: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    std::thread::scope(|scope| {
        for worker in 0..threads {
            let (done, failure) = (&done, &failure);
            scope.spawn(move || {
                let result = (|| -> Result<(), String> {
                    let mut zip = open()?;
                    let mut buffer = vec![0u8; 1 << 16];
                    for index in (worker..total).step_by(threads) {
                        if failure.lock().unwrap().is_some() {
                            return Ok(());
                        }
                        let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
                        let count = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if count % 128 == 0 || count as usize == total {
                            patch::report(progress, "Extraction", step, count, total as u64);
                        }
                        if entry.is_dir() {
                            continue;
                        }
                        let Some(relative) = entry.enclosed_name() else { continue };
                        let target = folder.join(relative);
                        if same_file(&target, entry.size(), entry.crc32()) {
                            continue;
                        }
                        let file = File::create(&target).map_err(|error| format!("{} : {error}", target.display()))?;
                        let mut output = io::BufWriter::with_capacity(1 << 18, file);
                        loop {
                            let read = io::Read::read(&mut entry, &mut buffer).map_err(|error| error.to_string())?;
                            if read == 0 {
                                break;
                            }
                            io::Write::write_all(&mut output, &buffer[..read]).map_err(|error| format!("{} : {error}", target.display()))?;
                        }
                        io::Write::flush(&mut output).map_err(|error| error.to_string())?;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    failure.lock().unwrap().get_or_insert(error);
                }
            });
        }
    });
    match failure.into_inner().unwrap() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Whether `path` already holds this zip entry (same size, same CRC-32).
fn same_file(path: &Path, size: u64, crc: u32) -> bool {
    match fs::metadata(path) {
        Ok(meta) if meta.len() == size => fs::read(path).map(|bytes| crc32fast::hash(&bytes) == crc).unwrap_or(false),
        _ => false,
    }
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
