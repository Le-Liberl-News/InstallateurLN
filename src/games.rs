//! The games the installer patches, and their presentation (description,
//! status, credits) read from the site's projects.json.

use serde_json::Value;

/// A game the installer knows how to patch: the platform project that
/// publishes its patch and how to find it on disk.
pub struct Game {
    /// Platform project (plateforme/api/patch.php?project=…).
    pub project: &'static str,
    /// Name in projects.json.
    pub name: &'static str,
    pub steam_id: u32,
    pub gog_id: u64,
    /// Folder name under steamapps/common.
    pub steam_folder: &'static str,
    /// The executable the "Jouer" button starts outside Steam.
    pub exe: &'static str,
    /// A file that proves a folder is this game.
    pub marker: &'static str,
    /// Leftovers of the old patch (DLL that translated at launch and its
    /// data_fr folder): moved to "ancien_patch" before installing.
    pub old_patch: &'static [&'static str],
    /// Its Japanese voices (SoraVoice: dsound.dll + voice/) can be installed:
    /// the platform serves them when the project's "voix" release exists.
    pub voices: bool,
}

pub const GAMES: &[Game] = &[
    Game {
        project: "sky-fc",
        name: "Trails in the Sky FC",
        steam_id: 251150,
        gog_id: 1207665083,
        steam_folder: "Trails in the Sky FC",
        exe: "ed6_win_DX9.exe",
        marker: "ED6_DT01.dir",
        old_patch: &["dinput8.dll", "data_fr", "icudt71.dll", "icuuc71.dll"],
        voices: true,
    },
    Game {
        project: "sky-sc",
        name: "Trails in the Sky SC",
        steam_id: 251290,
        gog_id: 0, // not in projects.json yet

        steam_folder: "Trails in the Sky SC",
        exe: "ed6_win2_DX9.exe",
        marker: "ED6_DT21.dir",
        old_patch: &[],
        voices: true,
    },
    Game {
        project: "sky-3rd",
        name: "Trails in the Sky The 3rd",
        steam_id: 436670,
        gog_id: 1797747105,
        steam_folder: "Trails in the Sky the 3rd",
        exe: "ed6_win3_DX9.exe",
        marker: "ed6_win3_DX9.exe",
        old_patch: &[],
        voices: true,
    },
];

/// What projects.json says of a game.
#[derive(Clone, Default)]
pub struct Sheet {
    pub description: String,
    #[allow(dead_code)] // the platform's release says it better
    pub status: String,
    pub warning: String,
    pub languages: String,
    pub credits: Vec<Credit>,
    /// How a retro game's floppies are translated ("install" of projects.json).
    pub install: Option<Value>,
}

#[derive(Clone)]
pub enum Credit {
    /// A team ("Traduction"), with the colour the site gives it.
    Team(String, [u8; 3]),
    Person(String),
}

pub const PROJECTS_URL: &str = "https://leliberlnews.fr/projects.json";

/// The sheets of every game, by name, from projects.json (the site's, or the
/// copy shipped in the executable when the site cannot be reached).
pub fn sheets(json: &Value) -> Vec<(String, Sheet)> {
    let mut found = Vec::new();
    // Every category ("trails", "retro"), every series, every game.
    let categories = json.as_object().into_iter().flat_map(|map| map.values()).filter_map(Value::as_object);
    for series in categories.flat_map(|map| map.values()) {
        for game in series.get("games").and_then(Value::as_object).into_iter().flat_map(|map| map.values()) {
            let text = |key: &str| game.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
            let credits = game
                .get("staff")
                .and_then(Value::as_array)
                .map(|lines| lines.iter().filter_map(Value::as_str).filter_map(credit).collect())
                .unwrap_or_default();
            found.push((
                text("name"),
                Sheet {
                    description: text("desc"),
                    status: text("status"),
                    warning: strip_tags(&text("warning")),
                    languages: text("availableLanguages"),
                    credits,
                    install: game.get("install").filter(|value| value.is_object()).cloned(),
                },
            ));
        }
    }
    found
}

/// One line of the staff list: `<span style="color: #C70039…">- Fondateur -</span>`
/// is a team, a plain name a person, an empty line nothing.
fn credit(line: &str) -> Option<Credit> {
    let text = strip_tags(line);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if line.contains("<span") {
        let colour = line
            .find('#')
            .and_then(|at| line.get(at + 1..at + 7))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .map(|value| [(value >> 16) as u8, (value >> 8) as u8, value as u8])
            .unwrap_or([0x46, 0x33, 0x20]);
        return Some(Credit::Team(text.trim_matches(|c: char| c == '-' || c.is_whitespace()).to_string(), colour));
    }
    Some(Credit::Person(text.to_string()))
}

pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&")
}
