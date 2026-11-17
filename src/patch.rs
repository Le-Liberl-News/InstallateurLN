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

/// The platform's patch API. LN_PLATFORM: another platform (a local copy of the site) to try it.
pub fn api() -> String {
    format!("{}/plateforme/api/patch.php", platform())
}

pub fn platform() -> String {
    std::env::var("LN_PLATFORM").map(|url| url.trim_end_matches('/').to_string()).unwrap_or_else(|_| "https://leliberlnews.fr".into())
}
const USER_AGENT: &str = "InstallateurLiberlNews";

/// Where the patch comes from: one channel of the platform (release, public test,
/// private test), pinned to the version chosen; or (to try a version before it is
/// served) a release folder given by LN_PATCH_DIR.
#[derive(Clone)]
pub enum Source {
    Platform { project: &'static str, channel: String, tag: Option<String>, token: Option<String> },
    Folder(PathBuf),
}

impl Source {
    pub fn for_game(game: &Game) -> Self {
        match std::env::var_os("LN_PATCH_DIR") {
            Some(folder) if game.project == std::env::var("LN_PATCH_PROJECT").unwrap_or_else(|_| "sky-fc".into()) => {
                Source::Folder(PathBuf::from(folder))
            }
            _ => Source::Platform { project: game.project, channel: "release".into(), tag: None, token: None },
        }
    }

    /// The same source, for one version of one channel (a private one with the account's token).
    pub fn chosen(&self, release: &Release, token: Option<String>) -> Self {
        match self {
            Source::Platform { project, .. } => Source::Platform { project, channel: release.channel.clone(), tag: Some(release.tag.clone()), token },
            Source::Folder(_) => self.clone(),
        }
    }

    fn token(&self) -> Option<&str> {
        match self {
            Source::Platform { token, .. } => token.as_deref(),
            Source::Folder(_) => None,
        }
    }

    /// A file of the version (the account's token never goes in the address).
    fn asset_url(&self, name: &str) -> String {
        match self {
            Source::Platform { project, channel, tag, .. } => {
                let tag = tag.as_ref().map(|tag| format!("&tag={tag}")).unwrap_or_default();
                format!("{}?channel=installer&game={project}&slot={channel}{tag}&asset={name}", api())
            }
            Source::Folder(_) => String::new(),
        }
    }
}

/// One version the platform serves for a game, in one of its channels.
#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    /// The version number shown (1.1.0).
    pub version: String,
    /// The version written in patch.json and version.txt (2026.11.13.1053).
    #[serde(skip)]
    pub build: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub tag: String,
    /// "release", "test" or "private", and its name ("Test publique").
    #[serde(default = "release_channel")]
    pub channel: String,
    #[serde(default)]
    pub label: String,
    /// A test version: its disclaimer (warning) is shown next to its number.
    #[serde(default)]
    pub test: bool,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub warning: String,
}

fn release_channel() -> String {
    "release".into()
}

/// What the platform offers for a game: its versions, and whether a private test
/// version waits for a sign-in.
#[derive(Clone, Debug, Default)]
pub struct Offers {
    pub releases: Vec<Release>,
    #[allow(dead_code)] // said by the platform; nothing shows it for now
    pub private_available: bool,
    /// The account's token was accepted (false once it expired or was revoked).
    pub signed_in: bool,
}

impl Offers {
    /// The version of a channel, else the first offered (the release first).
    pub fn pick(&self, channel: Option<&str>) -> Option<&Release> {
        channel.and_then(|channel| self.releases.iter().find(|release| release.channel == channel)).or_else(|| self.releases.first())
    }
}

#[derive(Deserialize)]
struct ApiOffers {
    #[serde(default)]
    versions: Vec<Release>,
    #[serde(default)]
    private_available: bool,
    #[serde(default)]
    signed_in: bool,
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

/// The versions a game (or a retro game) has on the platform; `token`: a signed-in account's.
pub fn offers(project: &str, token: Option<&str>) -> Result<Offers, String> {
    let url = format!("{}?channel=installer&game={project}&list=1", api());
    let mut request = ureq::get(&url).header("User-Agent", USER_AGENT);
    if let Some(token) = token {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let response = request.config().http_status_as_error(false).build().call().map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let body = response.into_body().read_to_string().map_err(|error| error.to_string())?;
    if status != 200 {
        let message = serde_json::from_str::<ApiError>(&body).map(|error| error.error).unwrap_or(body);
        return Err(format!("plateforme ({status}) : {message}"));
    }
    let api: ApiOffers = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let releases = api
        .versions
        .into_iter()
        .map(|mut release| {
            release.build = release.tag.trim_start_matches('v').to_string();
            release
        })
        .collect();
    Ok(Offers { releases, private_available: api.private_available, signed_in: api.signed_in })
}

/// The version in a release folder (LN_PATCH_DIR).
pub fn folder_release(folder: &Path) -> Result<Option<Release>, String> {
    let manifest = read_manifest(&folder.join("patch.json"))?;
    let size = manifest.files.iter().map(|file| file.delta_size).sum();
    Ok(Some(Release {
        version: format!("{} (dossier local)", manifest.version),
        build: manifest.version,
        notes: String::new(),
        size,
        tag: String::new(),
        channel: "release".into(),
        label: "Dossier local".into(),
        test: false,
        private: false,
        warning: String::new(),
    }))
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
    /// Patch installation has two passes and never resets its overall counter.
    pub install_fraction: Option<f32>,
    pub install_phase: Option<&'static str>,
    /// The window asked to stop (Annuler, or closing it): honoured while the game is still untouched.
    pub cancel: bool,
    /// Whether stopping is still possible (the French files are being prepared aside).
    pub cancellable: bool,
}

impl Progress {
    /// Overall progress, 0 to 1: whole files done plus the current one's share.
    pub fn fraction(&self) -> f32 {
        if let Some(value) = self.install_fraction { return value; }
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
    state.step = state.install_phase.map(str::to_owned).unwrap_or_else(|| step.into());
    state.detail = detail.into();
    state.done = done;
    state.total = total;
    if let Some(previous) = state.install_fraction {
        let current = if total > 0 { (done as f32 / total as f32).min(1.0) * 0.8 } else { 0.0 };
        state.install_fraction = Some(previous.max((state.file as f32 + current) / state.files.max(1) as f32).min(1.0));
    }
}

/// What an installation that stopped half-way leaves: the journal of the files being swapped in.
const JOURNAL: &str = "patch-en-cours.txt";
pub const CANCELLED: &str = "Installation annulée : le jeu n'a pas été modifié.";

fn cancelled(progress: &Shared) -> bool {
    progress.lock().unwrap().cancel
}

/// An installation stopped while it swapped the files in (its journal is still there), or French files
/// left aside (an older installer stopped mid-way): the game may mix French and English archives.
pub fn interrupted(folder: &Path) -> bool {
    folder.join(JOURNAL).exists()
        || fs::read_dir(folder).map(|entries| entries.flatten().any(|entry| entry.file_name().to_string_lossy().ends_with(".fr.tmp"))).unwrap_or(false)
}

/// Puts a half-patched game back in English, as a whole (an archive's .dir and .dat must come from the same
/// version), and removes what the stopped installation left.
pub fn repair(folder: &Path, progress: &Shared) -> Result<String, String> {
    if folder.join("dat_en").is_dir() {
        uninstall(folder, progress)?;
    }
    remove_leftovers(folder);
    let _ = fs::remove_file(folder.join(JOURNAL));
    Ok("Le jeu est de nouveau en anglais.".into())
}

fn remove_leftovers(folder: &Path) {
    if let Ok(entries) = fs::read_dir(folder) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().ends_with(".fr.tmp") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

/// Installs a release without ever leaving the game half-patched: every French file is first made aside
/// (verified, next to its target), the game untouched — stopping then only drops them —, then all are swapped
/// in together, the journal kept until the last one (a stop during the swap is repaired at the next launch).
pub fn install(game: &Game, folder: &Path, source: &Source, progress: &Shared) -> Result<String, String> {
    report(progress, "Lecture du patch", "", 0, 0);
    if interrupted(folder) {
        repair(folder, progress)?;
    }
    let manifest = match source {
        Source::Folder(dir) => read_manifest(&dir.join("patch.json"))?,
        Source::Platform { .. } => {
            let text = download_text_authorized(&source.asset_url("patch.json"), source.token())?;
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|error| error.to_string())?
        }
    };
    let xdelta = xdelta3()?;

    let english = folder.join("dat_en");
    fs::create_dir_all(&english).map_err(|error| error.to_string())?;
    let work = std::env::temp_dir().join("LiberlNews").join(game.project);
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;

    let total = manifest.files.len() as u64;
    {
        let mut state = progress.lock().unwrap();
        state.files = total * 2;
        state.install_fraction = Some(0.0);
        state.install_phase = Some("Installation");
        state.cancellable = true;
    }
    // 1. The French files, made aside.
    let mut prepared: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut failures = Vec::new();
    let drop_prepared = |prepared: &[(PathBuf, PathBuf)]| {
        for (temporary, _) in prepared {
            let _ = fs::remove_file(temporary);
        }
    };
    for (index, file) in manifest.files.iter().enumerate() {
        if cancelled(progress) {
            drop_prepared(&prepared);
            let _ = fs::remove_dir_all(&work);
            return Err(CANCELLED.into());
        }
        let step = format!("{} ({}/{})", file.file, index + 1, total);
        progress.lock().unwrap().file = index as u64;
        match prepare_file(folder, &english, &work, source, &xdelta, file, &step, progress) {
            Ok(Some(temporary)) => prepared.push((temporary, folder.join(&file.file))),
            Ok(None) => {}
            Err(error) => failures.push(format!("{} : {error}", file.file)),
        }
        progress.lock().unwrap().file = index as u64 + 1;
        report(progress, "Installation", step, 0, 0);
    }
    let _ = fs::remove_dir_all(&work);
    if cancelled(progress) {
        drop_prepared(&prepared);
        return Err(CANCELLED.into());
    }
    if !failures.is_empty() {
        drop_prepared(&prepared);
        return Err(format!(
            "Fichiers non patchés (le jeu n'a pas été modifié) :\n{}\n\nVérifiez l'intégrité des fichiers du jeu (Steam : Propriétés > Fichiers installés > Vérifier), puis réessayez.",
            failures.join("\n")
        ));
    }

    // Old patch moved aside (a DLL translating at launch, reading data_fr): the game now reads the
    // French files in place. Done only now, once the new files are ready.
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

    // 2. All swapped in at once (renames: a moment), under the journal.
    {
        let mut state = progress.lock().unwrap();
        state.cancellable = false;
        state.install_phase = Some("Installation");
    }
    let names: Vec<String> = prepared.iter().map(|(_, destination)| destination.strip_prefix(folder).unwrap_or(destination).display().to_string()).collect();
    fs::write(folder.join(JOURNAL), names.join("\n")).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(folder.join("version.txt"));
    for (temporary, destination) in &prepared {
        if let Err(error) = fs::rename(temporary, destination) {
            // A file the system refuses (the game running?): back to English as a whole, never mixed.
            let name = destination.strip_prefix(folder).unwrap_or(destination).display().to_string();
            drop_prepared(&prepared);
            let _ = repair(folder, progress);
            return Err(format!("{name} : {error}\n\nLe jeu a été remis en anglais. Fermez le jeu s'il est ouvert, puis réessayez."));
        }
    }

    // 3. Every installed file checked, those already up to date included.
    progress.lock().unwrap().install_phase = Some("Vérification");
    for (index, file) in manifest.files.iter().enumerate() {
        let step = format!("{} ({}/{})", file.file, index + 1, total);
        report(progress, "Vérification", &step, 0, 0);
        let installed_sha = sha256(&folder.join(&file.file));
        if installed_sha != file.patched_sha256 {
            failures.push(format!("{} : le fichier installé ne correspond pas", file.file));
        }
        progress.lock().unwrap().file = total + index as u64 + 1;
        report(progress, "Vérification", step, 0, 0);
    }
    if !failures.is_empty() {
        let _ = repair(folder, progress);
        return Err(format!("Fichiers non conformes après installation (le jeu a été remis en anglais) :\n{}", failures.join("\n")));
    }
    fs::write(folder.join("version.txt"), format!("{}\n", manifest.version)).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(folder.join(JOURNAL));
    Ok(format!("Patch {} installé.", manifest.version))
}

/// One file's French version, made and verified next to its target (None: already this version's).
#[allow(clippy::too_many_arguments)]
fn prepare_file(folder: &Path, english: &Path, work: &Path, source: &Source, xdelta: &Path, file: &PatchFile, step: &str, progress: &Shared) -> Result<Option<PathBuf>, String> {
    let destination = folder.join(&file.file);
    let saved = english.join(&file.file);
    report(progress, "Vérification", step, 0, 0);
    let current = sha256(&destination);
    // Already this version's French file: nothing to do.
    if current == file.patched_sha256 {
        return Ok(None);
    }
    // The English original: in dat_en already, or the game's file while it
    // is still English (copied to dat_en once).
    let saved_sha = sha256(&saved);
    if saved_sha != file.original_sha256 {
        if current != file.original_sha256 {
            return Err("fichier anglais d'origine introuvable (ni dans dat_en, ni dans le jeu)".into());
        }
        report(progress, "Sauvegarde de l'anglais", step, 0, 0);
        if let Some(parent) = saved.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(&destination, &saved).map_err(|error| error.to_string())?;
    }
    let delta = match source {
        Source::Folder(dir) => dir.join(&file.delta),
        Source::Platform { .. } => {
            let name = file.delta.rsplit('/').next().unwrap_or(&file.delta);
            let target = work.join(name);
            let result = download_authorized(&source.asset_url(name), &target, file.delta_size, step, progress, source.token());
            result?;
            target
        }
    };
    report(progress, "Application", step, 0, 0);
    let file_name = destination.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| file.file.clone());
    let temporary = destination.with_file_name(format!("{file_name}.fr.tmp"));
    let size = fs::metadata(&saved).map(|meta| meta.len()).unwrap_or(0);
    // Source window covering the whole original (the release's rule).
    let result = apply_delta(xdelta, &saved, &delta, &temporary, size);
    if !matches!(source, Source::Folder(_)) {
        let _ = fs::remove_file(&delta);
    }
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    report(progress, "Vérification du résultat", step, 0, 0);
    let result_sha = sha256(&temporary);
    if result_sha != file.patched_sha256 {
        let _ = fs::remove_file(&temporary);
        return Err("le fichier français obtenu ne correspond pas".into());
    }
    Ok(Some(temporary))
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

pub fn download_text(url: &str) -> Result<String, String> { download_text_authorized(url,None) }
pub fn download_text_authorized(url: &str, token: Option<&str>) -> Result<String, String> {
    let mut request=ureq::get(url).header("User-Agent", USER_AGENT);
    if let Some(token)=token {request=request.header("Authorization",&format!("Bearer {token}"));}
    let response = request.call().map_err(|error| format!("téléchargement : {error}"))?;
    response.into_body().with_config().limit(64 << 20).read_to_string().map_err(|error| error.to_string())
}

/// Downloads to `target`: the platform answers with a redirection to a signed
/// GitHub link (valid a few minutes), followed here.
pub fn download(url: &str, target: &Path, expected: u64, step: &str, progress: &Shared) -> Result<(), String> { download_authorized(url,target,expected,step,progress,None) }
pub fn download_authorized(url: &str, target: &Path, expected: u64, step: &str, progress: &Shared, token: Option<&str>) -> Result<(), String> {
    let mut request=ureq::get(url).header("User-Agent", USER_AGENT);
    if let Some(token)=token {request=request.header("Authorization",&format!("Bearer {token}"));}
    let response = request.call().map_err(|error| format!("téléchargement : {error}"))?;
    let mut reader = response.into_body().into_reader();
    let mut output = File::create(target).map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        // Stopped from the window (Annuler, or closed): the download ends here.
        if cancelled(progress) { return Err(CANCELLED.into()); }
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

#[cfg(test)]
mod release_selection_tests {
    use super::*;
    #[test]
    fn installation_progress_never_rewinds_between_files_or_phases() {
        let shared = Shared::default();
        {
            let mut state = shared.lock().unwrap();
            state.files = 4;
            state.install_fraction = Some(0.0);
            state.install_phase = Some("Installation");
        }
        let mut previous = 0.0;
        for index in 0..2 {
            shared.lock().unwrap().file = index;
            for (label, done, total) in [("Vérification",0,0),("Téléchargement",50,100),("Téléchargement",100,100),("Application",0,0),("Vérification du résultat",0,0)] {
                report(&shared,label,"file",done,total);
                let state = shared.lock().unwrap();
                assert_eq!(state.step,"Installation");
                assert!(state.fraction() >= previous);
                assert!(state.fraction() < 0.5);
                previous = state.fraction();
            }
            shared.lock().unwrap().file = index+1;
            report(&shared,"Installation","file",0,0);
        }
        shared.lock().unwrap().install_phase = Some("Vérification");
        for index in 2..=4 {
            shared.lock().unwrap().file = index;
            report(&shared,"Vérification","file",0,0);
            let state = shared.lock().unwrap();
            assert_eq!(state.step,"Vérification");
            assert!(state.fraction() >= previous);
            previous = state.fraction();
        }
        assert_eq!(previous,1.0);
    }
    #[test]
    fn reads_the_channels_offered() {
        let api: ApiOffers = serde_json::from_str(r#"{"versions":[{"channel":"release","label":"Release","version":"1.1.0","tag":"v2026.11.13.1053"},{"channel":"test","label":"Test publique","test":true,"version":"1.2.0-test","tag":"v2026.11.16.1200","warning":"Scénario traduit à 60 %."}],"private_available":true}"#).unwrap();
        let offers = Offers { releases: api.versions, private_available: api.private_available, signed_in: false };
        assert_eq!(offers.pick(None).unwrap().channel, "release");
        assert!(offers.pick(Some("test")).unwrap().warning.contains("60 %"));
        assert_eq!(offers.pick(Some("private")).unwrap().channel, "release");
        assert!(offers.private_available);
    }
    #[test]
    fn tokens_stay_out_of_addresses_and_files_are_pinned() {
        let source = Source::Platform { project: "sky-sc", channel: "private".into(), tag: Some("v2026.11.16.0200".into()), token: Some("secret-fixture".into()) };
        let url = source.asset_url("patch.json");
        assert!(url.contains("slot=private") && url.contains("tag=v2026.11.16.0200") && !url.contains("secret-fixture"));
        assert_eq!(source.token(), Some("secret-fixture"));
    }
}
