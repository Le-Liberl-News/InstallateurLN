//! The installer updates itself: at start it asks GitHub for the latest
//! release of Le-Liberl-News/InstallateurLN (public, no token); when it is
//! newer, the window offers it, and one click downloads the executable for
//! this system, puts it in place of the running one (renamed aside, deleted at
//! the next start) and restarts.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const LATEST: &str = "https://api.github.com/repos/Le-Liberl-News/InstallateurLN/releases/latest";
#[cfg(windows)]
const ASSET: &str = "InstallateurLiberlNews.exe";
#[cfg(not(windows))]
const ASSET: &str = "InstallateurLiberlNews-linux";

#[derive(Clone)]
pub struct Update {
    pub version: String,
    url: String,
}

fn numbers(version: &str) -> Vec<u32> {
    version.trim_start_matches('v').split('.').map(|part| part.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)).collect()
}

/// The newer release, if there is one.
pub fn check() -> Option<Update> {
    let text = ureq::get(LATEST).header("User-Agent", "InstallateurLiberlNews").call().ok()?.into_body().read_to_string().ok()?;
    let release: Value = serde_json::from_str(&text).ok()?;
    let tag = release.get("tag_name")?.as_str()?.to_string();
    if numbers(&tag) <= numbers(env!("CARGO_PKG_VERSION")) {
        return None;
    }
    let url = release
        .get("assets")?
        .as_array()?
        .iter()
        .find(|asset| asset.get("name").and_then(Value::as_str) == Some(ASSET))?
        .get("browser_download_url")?
        .as_str()?
        .to_string();
    Some(Update { version: tag.trim_start_matches('v').to_string(), url })
}

fn aside(current: &std::path::Path) -> PathBuf {
    current.with_extension("ancien")
}

/// Removes what the previous update left (the old executable, set aside).
pub fn clean() {
    if let Ok(current) = std::env::current_exe() {
        let _ = fs::remove_file(aside(&current));
    }
}

/// Downloads the new executable, swaps it with the running one and starts it.
pub fn apply(update: &Update) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    let incoming = current.with_extension("nouveau");
    let response = ureq::get(&update.url).header("User-Agent", "InstallateurLiberlNews").call().map_err(|error| error.to_string())?;
    let bytes = response.into_body().with_config().limit(200 << 20).read_to_vec().map_err(|error| error.to_string())?;
    if bytes.len() < 1 << 20 {
        return Err("téléchargement incomplet".into());
    }
    fs::write(&incoming, bytes).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&incoming, fs::Permissions::from_mode(0o755)).map_err(|error| error.to_string())?;
    }
    // A running executable cannot be overwritten on Windows, but it can be renamed.
    let _ = fs::remove_file(aside(&current));
    fs::rename(&current, aside(&current)).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&incoming, &current) {
        let _ = fs::rename(aside(&current), &current);
        return Err(error.to_string());
    }
    std::process::Command::new(&current).spawn().map_err(|error| error.to_string())?;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::numbers;

    #[test]
    fn compares_versions() {
        assert!(numbers("v2.0.1") > numbers("2.0.0"));
        assert!(numbers("v2.1.0") > numbers("2.0.9"));
        assert!(numbers("v2.0.0") <= numbers("2.0.0"));
    }
}
