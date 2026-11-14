//! Translating a retro game's floppies (.d88/.t88), from the "install" recipe
//! of projects.json, as the previous installer did:
//!
//! - "xdelta": the archive holds one delta per known dump; the player's floppy
//!   is recognised by its SHA-1 and the matching delta applied (Asteka has 7
//!   dumps, Xanadu 7 over 2 floppies). A file holding several floppies end to
//!   end (a "pack") is split, translated floppy by floppy, put back together.
//! - "exec": the archive brings its own patcher (Dragon Slayer's dsretrack.exe,
//!   Windows only), run on each floppy.
//! - "link": not hosted for the installer yet; the project's page.
//!
//! The player's dump is never overwritten: "<name> [FR].d88" is written next to
//! it, with the manuals and notes in a "<game> [FR]" folder.

use crate::patch::{self, Shared};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

/// Where the archives are (the previous installer's config.json, "domain").
const CONFIG_URL: &str = "https://leliberlnews.fr/config.json";
const DOMAIN: &str = "https://twnkek290001-s3.fhmutu.net";

#[derive(Clone, PartialEq)]
pub enum Method {
    Xdelta,
    Exec,
    Link,
}

#[derive(Clone)]
pub struct Target {
    pub label: String,
    /// SHA-1 (upper case) of a known dump -> its delta in the archive.
    pub variants: HashMap<String, String>,
}

#[derive(Clone)]
pub struct Recipe {
    pub method: Method,
    pub archive: String,
    pub command: String,
    pub targets: Vec<Target>,
    pub extras: Vec<String>,
    /// Served by the platform from PatchRetro: its version, and where its files are.
    pub version: Option<String>,
    pub assets: Option<String>,
}

impl Recipe {
    pub fn read(value: &Value) -> Option<Recipe> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let method = match value.get("method")?.as_str()? {
            "xdelta" => Method::Xdelta,
            "exec" => Method::Exec,
            "link" => Method::Link,
            _ => return None,
        };
        let targets = value
            .get("targets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|target| Target {
                label: target.get("label").and_then(Value::as_str).unwrap_or_default().to_string(),
                variants: target
                    .get("variants")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                    .filter_map(|(sha1, delta)| Some((sha1.to_uppercase(), delta.as_str()?.to_string())))
                    .collect(),
            })
            .collect();
        let extras = value.get("extras").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        Some(Recipe { method, archive: text("archive"), command: text("command"), targets, extras, version: None, assets: None })
    }

    /// Whether this installer can translate the floppies itself here.
    pub fn local(&self) -> bool {
        match self.method {
            Method::Xdelta => !self.archive.is_empty(),
            Method::Exec => cfg!(windows) && !self.archive.is_empty() && !self.command.is_empty(),
            Method::Link => false,
        }
    }
}

/// A game's patch from PatchRetro (patch.php?channel=retro): None while it is not there.
pub fn platform(game: &str) -> Option<Recipe> {
    let url = format!("{}?channel=retro&game={game}", patch::API);
    let text = patch::download_text(&url).ok()?;
    let answer: Value = serde_json::from_str(&text).ok()?;
    let mut recipe = Recipe::read(answer.get("recipe")?)?;
    recipe.version = answer.get("version").and_then(Value::as_str).map(str::to_string);
    recipe.assets = Some(format!("{url}&asset="));
    Some(recipe)
}

fn domain() -> String {
    patch::download_text(CONFIG_URL)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok())
        .and_then(|config| config.get("domain").and_then(Value::as_str).map(|domain| domain.trim_end_matches('/').to_string()))
        .unwrap_or_else(|| DOMAIN.to_string())
}

/// An archive's address, its path made safe for a URL ("Xanadu/xanadu-[FR].zip").
fn archive_url(archive: &str) -> String {
    let path: String = archive
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '[' => "%5B".to_string(),
            ']' => "%5D".to_string(),
            _ => c.to_string(),
        })
        .collect();
    format!("{}/{path}", domain())
}

/// The floppies of a file: each starts with a 0x2B0-byte header whose offset
/// 0x1C holds its whole length. A file not made of such floppies is one piece.
pub fn split_d88(bytes: &[u8]) -> Vec<&[u8]> {
    const HEADER: usize = 0x2B0;
    let mut pieces = Vec::new();
    let mut offset = 0usize;
    while offset + HEADER <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 0x1C..offset + 0x20].try_into().unwrap()) as usize;
        if size < HEADER || offset + size > bytes.len() {
            return vec![bytes];
        }
        pieces.push(&bytes[offset..offset + size]);
        offset += size;
    }
    if offset != bytes.len() || pieces.is_empty() {
        return vec![bytes];
    }
    pieces
}

fn sha1(bytes: &[u8]) -> String {
    hex::encode_upper(Sha1::digest(bytes))
}

fn output_name(disk: &Path) -> PathBuf {
    let stem = disk.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    let extension = disk.extension().map(|extension| format!(".{}", extension.to_string_lossy())).unwrap_or_default();
    disk.with_file_name(format!("{stem} [FR]{extension}"))
}

/// Translates the floppies; returns the message and the folder to show.
pub fn translate(title: &str, recipe: &Recipe, disks: &[PathBuf], progress: &Shared) -> Result<(String, PathBuf), String> {
    let work = patch::data_dir().join("retro");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    let result = run(title, recipe, disks, &work, progress);
    let _ = fs::remove_dir_all(&work);
    result
}

fn run(title: &str, recipe: &Recipe, disks: &[PathBuf], work: &Path, progress: &Shared) -> Result<(String, PathBuf), String> {
    patch::report(progress, "Téléchargement", "patch", 0, 0);
    let archive = work.join("patch.zip");
    let url = match &recipe.assets {
        Some(base) => format!("{base}{}", recipe.archive),
        None => archive_url(&recipe.archive),
    };
    patch::download(&url, &archive, 0, "patch", progress)?;
    let unpacked = work.join("patch");
    unzip(&archive, &unpacked)?;

    let mut translated = Vec::new();
    let mut unknown = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let count = disks.len() as u64;
    for (index, disk) in disks.iter().enumerate() {
        {
            let mut state = progress.lock().unwrap();
            state.file = index as u64;
            state.files = count;
        }
        let name = disk.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
        patch::report(progress, "Traduction", &name, 0, 0);
        match recipe.method {
            Method::Xdelta => {
                let bytes = fs::read(disk).map_err(|error| format!("{name} : {error}"))?;
                let pieces = split_d88(&bytes);
                let mut output = Vec::with_capacity(bytes.len());
                let mut matched = 0;
                for (number, piece) in pieces.iter().enumerate() {
                    let hash = sha1(piece);
                    let found = recipe.targets.iter().find_map(|target| target.variants.get(&hash).map(|delta| (target, delta)));
                    let Some((target, delta)) = found else {
                        unknown.push(format!("{name}{} : {hash}", if pieces.len() > 1 { format!(" (disquette {}/{})", number + 1, pieces.len()) } else { String::new() }));
                        output.extend_from_slice(piece);
                        continue;
                    };
                    let delta = unpacked.join(delta);
                    if !delta.is_file() {
                        return Err(format!("{} est absent de l'archive du patch.", delta.display()));
                    }
                    let source = work.join(format!("disque{number}.d88"));
                    let result = work.join(format!("disque{number}_fr.d88"));
                    fs::write(&source, piece).map_err(|error| error.to_string())?;
                    patch::apply_delta(&patch::xdelta3()?, &source, &delta, &result, piece.len() as u64)?;
                    output.extend(fs::read(&result).map_err(|error| error.to_string())?);
                    matched += 1;
                    if !target.label.is_empty() {
                        labels.push(target.label.clone());
                    }
                }
                if matched > 0 {
                    let target = output_name(disk);
                    fs::write(&target, output).map_err(|error| format!("{} : {error}", target.display()))?;
                    translated.push(target);
                }
            }
            Method::Exec => {
                // The patcher writes its result next to the floppy: what appears there is it.
                let folder = disk.parent().map(Path::to_path_buf).unwrap_or_default();
                let before: Vec<_> = fs::read_dir(&folder).map(|entries| entries.flatten().map(|entry| entry.file_name()).collect()).unwrap_or_default();
                let patcher = unpacked.join(&recipe.command);
                let mut command = std::process::Command::new(&patcher);
                command.arg(disk).current_dir(patcher.parent().unwrap_or(&unpacked));
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x0800_0000);
                }
                command.output().map_err(|error| format!("{} : {error}", recipe.command))?;
                let created: Vec<PathBuf> = fs::read_dir(&folder)
                    .map(|entries| entries.flatten().filter(|entry| !before.contains(&entry.file_name())).map(|entry| entry.path()).collect())
                    .unwrap_or_default();
                match created.into_iter().next() {
                    Some(path) => translated.push(path),
                    None => unknown.push(format!("{name} : version non reconnue par le patcheur")),
                }
            }
            Method::Link => {}
        }
    }
    if translated.is_empty() {
        return Err(format!(
            "Aucune disquette reconnue. Il existe plusieurs versions de ces jeux et la vôtre n'est pas encore couverte :\n{}",
            unknown.join("\n")
        ));
    }
    // The manuals and notes, next to the player's floppies.
    let base = disks[0].parent().map(Path::to_path_buf).unwrap_or_default();
    let mut shown = base.clone();
    if !recipe.extras.is_empty() {
        let extras = base.join(format!("{title} [FR]"));
        fs::create_dir_all(&extras).map_err(|error| error.to_string())?;
        for extra in &recipe.extras {
            let source = unpacked.join(extra);
            if let Some(name) = source.file_name() {
                let _ = fs::copy(&source, extras.join(name));
            }
        }
        shown = extras;
    }
    let mut message = format!("{} disquette(s) traduite(s) : {}.", translated.len(), translated.iter().filter_map(|path| path.file_name()).map(|name| name.to_string_lossy().to_string()).collect::<Vec<_>>().join(", "));
    if !labels.is_empty() {
        message.push_str(&format!(" ({})", labels.join(", ")));
    }
    if !unknown.is_empty() {
        message.push_str(&format!(" Non reconnue(s) : {}.", unknown.join(" ; ")));
    }
    Ok((message, shown))
}

fn unzip(archive: &Path, target: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(File::open(archive).map_err(|error| error.to_string())?).map_err(|error| format!("archive du patch : {error}"))?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
        let Some(relative) = entry.enclosed_name() else { continue };
        let path = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output = File::create(&path).map_err(|error| error.to_string())?;
        io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::split_d88;

    fn floppy(size: usize, fill: u8) -> Vec<u8> {
        let mut bytes = vec![fill; size];
        bytes[0x1C..0x20].copy_from_slice(&(size as u32).to_le_bytes());
        bytes
    }

    #[test]
    fn splits_packs_and_keeps_odd_files_whole() {
        let pack = [floppy(0x400, 1), floppy(0x500, 2)].concat();
        let pieces = split_d88(&pack);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[1].len(), 0x500);
        let odd = vec![0u8; 0x300];
        assert_eq!(split_d88(&odd).len(), 1);
    }
}

#[cfg(test)]
mod online {
    /// The whole chain against the platform: recipe, archive, unzip, recognition.
    /// A floppy no recipe knows is refused with its SHA-1. (cargo test -- --ignored)
    #[test]
    #[ignore]
    fn asteka_from_patchretro() {
        let recipe = super::platform("asteka").expect("recette d'Asteka sur la plateforme");
        assert_eq!(recipe.version.as_deref(), Some("1.0.0"));
        let dir = std::env::temp_dir().join("ln-retro-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let disk = dir.join("inconnue.d88");
        std::fs::write(&disk, vec![0u8; 4096]).unwrap();
        let progress = crate::patch::Shared::default();
        let error = super::translate("Asteka", &recipe, &[disk], &progress).unwrap_err();
        assert!(error.contains("Aucune disquette reconnue"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
