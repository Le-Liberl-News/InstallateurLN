//! The games the installer patches, and their presentation (description,
//! languages, credits) given by the site's catalogue (api/launcher.php).

use serde_json::Value;

/// A game the installer knows how to patch: the platform project that
/// publishes its patch and how to find it on disk.
pub struct Game {
    /// Platform project (plateforme/api/patch.php?project=…).
    pub project: &'static str,
    /// Its name (dialogs).
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
        gog_id: 0, // no GOG version known yet

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

/// What the site's project page says of a game (api/launcher.php).
#[derive(Clone, Default)]
pub struct Sheet {
    pub description: String,
    pub warning: String,
    pub languages: String,
    pub credits: Vec<Credit>,
}

#[derive(Clone)]
pub enum Credit {
    /// A team ("Traduction"), with the colour the site gives it.
    Team(String, [u8; 3]),
    Person(String),
}

/// The sheet of one project of the catalogue: its description, languages, warning, and its
/// credits as teams ({team, colour "#RRGGBB", people}) laid out one after the other.
pub fn sheet(item: &Value) -> Sheet {
    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let mut credits = Vec::new();
    for team in item.get("credits").and_then(Value::as_array).into_iter().flatten() {
        let name = team.get("team").and_then(Value::as_str).unwrap_or_default().trim();
        if !name.is_empty() {
            let colour = team
                .get("colour")
                .and_then(Value::as_str)
                .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
                .map(|value| [(value >> 16) as u8, (value >> 8) as u8, value as u8])
                .unwrap_or([0x46, 0x33, 0x20]);
            credits.push(Credit::Team(name.to_string(), colour));
        }
        for person in team.get("people").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            credits.push(Credit::Person(person.to_string()));
        }
    }
    Sheet { description: text("description"), warning: strip_tags(&text("warning")), languages: text("languages"), credits }
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
