//! Installing a patch: the release is read from the platform (which hands out
//! short-lived links to the project's private GitHub release), each changed
//! file's delta is downloaded and applied with xdelta3 to the ENGLISH original,
//! kept once in "dat_en". Same rules as the release kit's appliquer.ps1.

use crate::games::Game;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const API: &str = "https://leliberlnews.fr/plateforme/api/patch.php";
const USER_AGENT: &str = "InstallateurLiberlNews";

/// Where the patch comes from: the platform's public release, or (to try a
/// version before it is designated) a release folder given by LN_PATCH_DIR.
#[derive(Clone)]
pub enum Source {
    Platform(&'static str),
    Folder(PathBuf),
}

impl Source {
    pub fn for_game(game: &Game) -> Self {
        match std::env::var_os("LN_PATCH_DIR") {
            Some(folder) if game.project == std::env::var("LN_PATCH_PROJECT").unwrap_or_else(|_| "sky-fc".into()) => {
                Source::Folder(PathBuf::from(folder))
            }
            _ => Source::Platform(game.project),
        }
    }

    fn asset_url(project: &str, name: &str) -> String {
        format!("{API}?project={project}&channel=release&asset={name}")
    }
}

/// The release players get.
#[derive(Clone, Debug)]
pub struct Release {
    /// The version number shown (1.1.0).
    pub version: String,
    /// The version written in patch.json and version.txt (2026.11.13.1053).
    pub build: String,
    pub notes: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct ApiRelease {
    version: String,
    tag: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct ApiError {
    error: String,
}

#[derive(Deserialize, Clone)]
pub struct Manifest {
    pub version: String,
    pub files: Vec<PatchFile>,
}

#[derive(Deserialize, Clone)]
pub struct PatchFile {
    pub file: String,
    pub original_sha256: String,
    pub patched_sha256: String,
    pub delta: String,
    #[serde(default)]
    pub delta_size: u64,
}

pub fn release(source: &Source) -> Result<Option<Release>, String> {
    match source {
        Source::Folder(folder) => {
            let manifest = read_manifest(&folder.join("patch.json"))?;
            let size = manifest.files.iter().map(|file| file.delta_size).sum();
            Ok(Some(Release { version: format!("{} (dossier local)", manifest.version), build: manifest.version, notes: String::new(), size }))
        }
        Source::Platform(project) => {
            let url = format!("{API}?project={project}&channel=release");
            let response = ureq::get(&url).header("User-Agent", USER_AGENT).config().http_status_as_error(false).build().call().map_err(|error| error.to_string())?;
            let status = response.status().as_u16();
            let body = response.into_body().read_to_string().map_err(|error| error.to_string())?;
            if status == 404 {
                return Ok(None);
            }
            if status != 200 {
                let message = serde_json::from_str::<ApiError>(&body).map(|error| error.error).unwrap_or(body);
                return Err(format!("plateforme ({status}) : {message}"));
            }
            let api: ApiRelease = serde_json::from_str(&body).map_err(|error| error.to_string())?;
            let build = api.tag.trim_start_matches('v').to_string();
            Ok(Some(Release { version: api.version, build, notes: api.notes, size: api.size }))
        }
    }
}

fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{} : {error}", path.display()))?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|error| error.to_string())
}

/// What is in the game folder now.
#[derive(Clone, Debug, PartialEq)]
pub enum Installed {
    Nothing,
    /// The old patch (DLL + data_fr) is there.
    OldPatch,
    Version(String),
}

pub fn installed(game: &Game, folder: &Path) -> Installed {
    let english = folder.join("dat_en");
    if english.is_dir() {
        if let Ok(text) = fs::read_to_string(folder.join("version.txt")) {
            let version = text.trim().to_string();
            if !version.is_empty() {
                return Installed::Version(version);
            }
        }
        // The previous installer noted it in dat_en/patch.json.
        if let Ok(manifest) = read_manifest(&english.join("patch.json")) {
            return Installed::Version(manifest.version);
        }
    }
    if game.old_patch.len() >= 2 && game.old_patch.iter().take(2).all(|item| folder.join(item).exists()) {
        return Installed::OldPatch;
    }
    Installed::Nothing
}

/// Progress of the running job, shared with the window.
#[derive(Default)]
pub struct Progress {
    pub step: String,
    pub detail: String,
    pub done: u64,
    pub total: u64,
    /// Files handled so far, out of `files`: the overall progress.
    pub file: u64,
    pub files: u64,
    pub finished: Option<Result<String, String>>,
}

impl Progress {
    /// Overall progress, 0 to 1: whole files done plus the current one's share.
    pub fn fraction(&self) -> f32 {
        if self.files == 0 {
            return 0.0;
        }
        let current = if self.total > 0 { self.done as f32 / self.total as f32 } else { 0.0 };
        ((self.file as f32 + current.min(1.0)) / self.files as f32).min(1.0)
    }
}

pub type Shared = Arc<Mutex<Progress>>;

pub fn report(progress: &Shared, step: impl Into<String>, detail: impl Into<String>, done: u64, total: u64) {
    let mut state = progress.lock().unwrap();
    state.step = step.into();
    state.detail = detail.into();
    state.done = done;
    state.total = total;
}

pub fn install(game: &Game, folder: &Path, source: &Source, progress: &Shared) -> Result<String, String> {
    report(progress, "Lecture du patch", "", 0, 0);
    let manifest = match source {
        Source::Folder(dir) => read_manifest(&dir.join("patch.json"))?,
        Source::Platform(project) => {
            let text = download_text(&Source::asset_url(project, "patch.json"))?;
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|error| error.to_string())?
        }
    };
    let xdelta = xdelta3()?;

    // The old patch (a DLL translating at launch, reading data_fr) is moved
    // aside: the game now reads the French files in place.
    if installed(game, folder) == Installed::OldPatch {
        let aside = folder.join("ancien_patch");
        fs::create_dir_all(&aside).map_err(|error| error.to_string())?;
        for item in game.old_patch {
            let path = folder.join(item);
            if path.exists() {
                fs::rename(&path, aside.join(item)).map_err(|error| format!("ancien patch, {item} : {error}"))?;
            }
        }
    }

    let english = folder.join("dat_en");
    fs::create_dir_all(&english).map_err(|error| error.to_string())?;
    let work = std::env::temp_dir().join("LiberlNews").join(game.project);
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;

    let total = manifest.files.len() as u64;
    let mut failures = Vec::new();
    for (index, file) in manifest.files.iter().enumerate() {
        let step = format!("{} ({}/{})", file.file, index + 1, total);
        {
            let mut state = progress.lock().unwrap();
            state.file = index as u64;
            state.files = total;
        }
        if let Err(error) = install_file(folder, &english, &work, source, &xdelta, file, &step, progress) {
            failures.push(format!("{} : {error}", file.file));
        }
        report(progress, "Fichiers", step, index as u64 + 1, total);
    }
    let _ = fs::remove_dir_all(&work);
    if failures.is_empty() {
        fs::write(folder.join("version.txt"), format!("{}\n", manifest.version)).map_err(|error| error.to_string())?;
        Ok(format!("Patch {} installé.", manifest.version))
    } else {
        Err(format!(
            "Fichiers non patchés :\n{}\n\nVérifiez l'intégrité des fichiers du jeu (Steam : Propriétés > Fichiers installés > Vérifier), puis réessayez.",
            failures.join("\n")
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn install_file(folder: &Path, english: &Path, work: &Path, source: &Source, xdelta: &Path, file: &PatchFile, step: &str, progress: &Shared) -> Result<(), String> {
    let destination = folder.join(&file.file);
    let saved = english.join(&file.file);
    report(progress, "Vérification", step, 0, 0);
    let current = sha256(&destination);
    // Already this version's French file: nothing to do.
    if current == file.patched_sha256 {
        return Ok(());
    }
    // The English original: in dat_en already, or the game's file while it
    // is still English (copied to dat_en once).
    if sha256(&saved) != file.original_sha256 {
        if current != file.original_sha256 {
            return Err("fichier anglais d'origine introuvable (ni dans dat_en, ni dans le jeu)".into());
        }
        report(progress, "Sauvegarde de l'anglais", step, 0, 0);
        fs::copy(&destination, &saved).map_err(|error| error.to_string())?;
    }
    let delta = match source {
        Source::Folder(dir) => dir.join(&file.delta),
        Source::Platform(project) => {
            let name = file.delta.rsplit('/').next().unwrap_or(&file.delta);
            let target = work.join(name);
            download(&Source::asset_url(project, name), &target, file.delta_size, step, progress)?;
            target
        }
    };
    report(progress, "Application", step, 0, 0);
    let temporary = destination.with_file_name(format!("{}.fr.tmp", file.file));
    let size = fs::metadata(&saved).map(|meta| meta.len()).unwrap_or(0);
    // Source window covering the whole original (the release's rule).
    let result = apply_delta(xdelta, &saved, &delta, &temporary, size);
    if matches!(source, Source::Platform(_)) {
        let _ = fs::remove_file(&delta);
    }
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    report(progress, "Vérification du résultat", step, 0, 0);
    if sha256(&temporary) != file.patched_sha256 {
        let _ = fs::remove_file(&temporary);
        return Err("le fichier français obtenu ne correspond pas".into());
    }
    fs::rename(&temporary, &destination).map_err(|error| error.to_string())
}

/// Puts the English originals (dat_en) back in the game.
pub fn uninstall(folder: &Path, progress: &Shared) -> Result<String, String> {
    let english = folder.join("dat_en");
    let mut files = Vec::new();
    collect(&english, &english, &mut files).map_err(|error| error.to_string())?;
    files.retain(|relative| relative != Path::new("patch.json"));
    let total = files.len() as u64;
    for (index, relative) in files.iter().enumerate() {
        report(progress, "Restauration de l'anglais", relative.display().to_string(), 0, 0);
        {
            let mut state = progress.lock().unwrap();
            state.file = index as u64;
            state.files = total;
        }
        let target = folder.join(relative);
        let temporary = target.with_file_name(format!("{}.en.tmp", target.file_name().unwrap_or_default().to_string_lossy()));
        fs::copy(english.join(relative), &temporary).map_err(|error| format!("{} : {error}", relative.display()))?;
        fs::rename(&temporary, &target).map_err(|error| format!("{} : {error}", relative.display()))?;
    }
    let _ = fs::remove_file(folder.join("version.txt"));
    Ok("Le jeu est de nouveau en anglais.".into())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            out.push(relative.to_path_buf());
        }
    }
    Ok(())
}

/// Applies an xdelta3 delta: `source` + `delta` -> `target`, with a source
/// window covering the whole original (the release's rule).
pub fn apply_delta(xdelta: &Path, source: &Path, delta: &Path, target: &Path, size: u64) -> Result<(), String> {
    let window = (64u64 << 20).max(((size + (1 << 20)) >> 20) << 20);
    let mut command = std::process::Command::new(xdelta);
    command.args(["-d", "-f", "-B", &window.to_string(), "-s"]).arg(source).arg(delta).arg(target);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // no console window
    }
    let output = command.output().map_err(|error| format!("xdelta3 : {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("xdelta3 a échoué : {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

/// The installer's own folder for its data (xdelta3, downloads, the trailer's web view).
pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    base.unwrap_or_else(std::env::temp_dir).join("LiberlNews")
}

pub fn sha256(path: &Path) -> String {
    let Ok(mut file) = File::open(path) else { return String::new() };
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(_) => return String::new(),
        }
    }
    hex::encode(hasher.finalize())
}

pub fn download_text(url: &str) -> Result<String, String> {
    let response = ureq::get(url).header("User-Agent", USER_AGENT).call().map_err(|error| format!("téléchargement : {error}"))?;
    response.into_body().with_config().limit(64 << 20).read_to_string().map_err(|error| error.to_string())
}

/// Downloads to `target`: the platform answers with a redirection to a signed
/// GitHub link (valid a few minutes), followed here.
pub fn download(url: &str, target: &Path, expected: u64, step: &str, progress: &Shared) -> Result<(), String> {
    let response = ureq::get(url).header("User-Agent", USER_AGENT).call().map_err(|error| format!("téléchargement : {error}"))?;
    let mut reader = response.into_body().into_reader();
    let mut output = File::create(target).map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        let read = reader.read(&mut buffer).map_err(|error| format!("téléchargement : {error}"))?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
        done += read as u64;
        report(progress, "Téléchargement", step, done, expected.max(done));
    }
    Ok(())
}

/// xdelta3, shipped inside the executable and written once in the installer's
/// data folder: the Windows build's, or the Linux one the CI copies from Ubuntu
/// (it needs only the C library and liblzma, both on SteamOS).
pub fn xdelta3() -> Result<PathBuf, String> {
    #[cfg(windows)]
    const BYTES: &[u8] = include_bytes!("../assets/xdelta3.exe");
    #[cfg(not(windows))]
    const BYTES: &[u8] = include_bytes!("../assets/xdelta3-linux");
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(if cfg!(windows) { "xdelta3.exe" } else { "xdelta3" });
    if fs::metadata(&path).map(|meta| meta.len() as usize).ok() != Some(BYTES.len()) {
        fs::write(&path, BYTES).map_err(|error| format!("xdelta3 : {error}"))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).map_err(|error| format!("xdelta3 : {error}"))?;
    }
    Ok(path)
}
