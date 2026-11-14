//! What the installer shows: the site's public catalogue and articles
//! (/api/launcher.php, else the copy in the executable), each project joined
//! to its sheet (projects.json: description, credits) and, when the installer
//! can patch it, to its game (games::GAMES).

use crate::games::{self, GAMES, Game, Sheet};
use serde_json::Value;

pub const SITE: &str = "https://leliberlnews.fr";
pub const LAUNCHER_URL: &str = "https://leliberlnews.fr/api/launcher.php";
pub const DISCORD: &str = "https://discord.gg/3jSCpGJDSd";
pub const X: &str = "https://x.com/SoraNoKisekiFR";
pub const HELP: &str = "https://leliberlnews.fr/tutos/help.html";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Series {
    Trails,
    Retro,
    Other,
}

impl Series {
    pub fn label(self) -> &'static str {
        match self {
            Series::Trails => "Trails",
            Series::Retro => "Rétro",
            Series::Other => "Création",
        }
    }
}

#[derive(Clone)]
pub struct Project {
    pub id: String,
    /// "Trails in the Sky FC" (without "The Legend of Heroes: ").
    pub title: String,
    pub series: Series,
    pub platform: String,
    pub year: String,
    pub developer: String,
    pub status_label: String,
    pub banner: Option<String>,
    pub images: Vec<String>,
    pub youtube: Option<String>,
    pub page: String,
    /// Where a creation is played (a browser game).
    pub link: Option<String>,
    pub steam_id: Option<u32>,
    /// The game the installer patches; None: the site has the download.
    pub game: Option<&'static Game>,
    pub sheet: Sheet,
    /// Words that tie an article to the project.
    pub keywords: Vec<String>,
}

impl Project {
    /// The big picture: Steam's hero art, else the first screenshot, else the site's banner.
    pub fn key_art(&self) -> Option<String> {
        if let Some(id) = self.steam_id {
            return Some(format!("https://cdn.akamai.steamstatic.com/steam/apps/{id}/library_hero.jpg"));
        }
        self.images.first().cloned().or_else(|| self.banner.clone())
    }

    /// The small picture of lists and cards.
    pub fn thumbnail(&self) -> Option<String> {
        if let Some(id) = self.steam_id {
            return Some(format!("https://cdn.akamai.steamstatic.com/steam/apps/{id}/header.jpg"));
        }
        self.images.first().cloned().or_else(|| self.banner.clone())
    }

    pub fn matches(&self, query: &str) -> bool {
        let query = fold(query);
        query.split_whitespace().all(|word| {
            fold(&self.title).contains(word) || fold(&self.platform).contains(word) || fold(self.series.label()).contains(word) || self.year.contains(word)
        })
    }
}

/// Lowercase without accents, for the search.
pub fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            _ => c,
        })
        .collect()
}

#[derive(Clone)]
pub struct Article {
    pub title: String,
    pub author: String,
    /// "2026-10-31 12:00:00"
    pub date: String,
    pub summary: String,
    pub illustration: Option<String>,
    pub url: String,
}

impl Article {
    /// « 31 OCT. 2026 »
    pub fn short_date(&self) -> String {
        const MONTHS: [&str; 12] = ["JANV.", "FÉVR.", "MARS", "AVR.", "MAI", "JUIN", "JUIL.", "AOÛT", "SEPT.", "OCT.", "NOV.", "DÉC."];
        let parts: Vec<&str> = self.date.split(|c| c == '-' || c == ' ').collect();
        match (parts.first(), parts.get(1).and_then(|m| m.parse::<usize>().ok()), parts.get(2)) {
            (Some(year), Some(month), Some(day)) if (1..=12).contains(&month) => {
                format!("{} {} {}", day.trim_start_matches('0'), MONTHS[month - 1], year)
            }
            _ => self.date.clone(),
        }
    }
}

/// Per catalogue id: the sheet's name in projects.json, the Steam id, words for articles.
const KNOWN: &[(&str, &str, u32, &[&str])] = &[
    ("sky-fc", "Trails in the Sky FC", 251150, &["sky fc", "trails in the sky fc", " fc"]),
    ("sky-sc", "Trails in the Sky SC", 251290, &["sky sc", " sc "]),
    ("sky-3rd", "Trails in the Sky The 3rd", 436670, &["3rd"]),
    ("zero", "Trails from Zero", 1668510, &["zero"]),
    ("sen_1", "Trails of Cold Steel", 538680, &["cold steel"]),
    ("horror-house-1", "Horror House", 0, &["horror house"]),
    ("horror-house-2", "Horror House 2", 0, &["kyouki"]),
    ("demons-ring", "Demon's Ring", 0, &["demon's ring", "demon’s ring"]),
    ("dragon-slayer", "Dragon Slayer", 0, &["dragon slayer"]),
    ("xanadu", "Xanadu", 0, &["xanadu"]),
    ("asteka", "Asteka", 0, &["asteka"]),
    ("dolce-traccia", "Dolce Traccia", 0, &["dolce traccia"]),
];

pub struct Catalog {
    pub projects: Vec<Project>,
    pub articles: Vec<Article>,
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

/// The catalogue from the launcher JSON and the sheets of projects.json.
pub fn build(launcher: &Value, sheets_json: &Value) -> Catalog {
    let sheets = games::sheets(sheets_json);
    let mut projects: Vec<Project> = launcher
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| project(&text(item, "id"), item, &sheets))
        .collect();
    // The 3rd has no public page yet but the installer patches it: after SC.
    if !projects.iter().any(|project| project.id == "sky-3rd") {
        let third = serde_json::json!({
            "id": "sky-3rd", "title": "Trails in the Sky the 3rd", "platform": "PC", "year": "2007",
            "developer": "Nihon Falcom", "status_label": "Version de test", "images": [],
            "page": format!("{SITE}/projects.php"),
        });
        let at = projects.iter().position(|project| project.id == "sky-sc").map(|index| index + 1).unwrap_or(projects.len());
        projects.insert(at, project("sky-3rd", &third, &sheets));
    }
    let articles = launcher
        .get("articles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| Article {
            title: text(item, "title").replace('\u{202f}', " ").replace('\u{a0}', " "),
            author: text(item, "author"),
            date: text(item, "date"),
            summary: text(item, "summary").replace('\u{202f}', " ").replace('\u{a0}', " "),
            illustration: item.get("illustration").and_then(Value::as_str).map(str::to_string),
            url: text(item, "url"),
        })
        .collect();
    Catalog { projects, articles }
}

fn project(id: &str, item: &Value, sheets: &[(String, Sheet)]) -> Project {
    let known = KNOWN.iter().find(|(key, ..)| *key == id);
    let title = text(item, "title").trim_start_matches("The Legend of Heroes:").trim().to_string();
    let platform = text(item, "platform");
    let series = if id.starts_with("sky") || id == "zero" || id.starts_with("sen") {
        Series::Trails
    } else if platform == "PC-88" {
        Series::Retro
    } else {
        Series::Other
    };
    let sheet = known
        .and_then(|(_, name, ..)| sheets.iter().find(|(sheet_name, _)| sheet_name == name))
        .map(|(_, sheet)| sheet.clone())
        .unwrap_or_default();
    let mut keywords: Vec<String> = known.map(|(.., words)| words.iter().map(|word| word.to_string()).collect()).unwrap_or_default();
    keywords.push(fold(&title));
    Project {
        id: id.to_string(),
        title,
        series,
        platform,
        year: text(item, "year"),
        developer: text(item, "developer"),
        status_label: text(item, "status_label"),
        banner: item.get("banner").and_then(Value::as_str).map(str::to_string),
        images: item.get("images").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
        youtube: item.get("youtube").and_then(Value::as_str).map(str::to_string),
        page: text(item, "page"),
        link: item
            .get("link")
            .and_then(Value::as_str)
            .filter(|link| !link.is_empty())
            .map(str::to_string)
            .or_else(|| (id == "dolce-traccia").then(|| format!("{SITE}/aventure/dolce-traccia/"))),
        steam_id: known.map(|(_, _, steam, _)| *steam).filter(|steam| *steam != 0),
        game: GAMES.iter().find(|game| game.project == id),
        sheet,
        keywords,
    }
}

/// The latest article about a project, if any.
pub fn article_about<'a>(articles: &'a [Article], project: &Project) -> Option<&'a Article> {
    articles.iter().find(|article| {
        let haystack = format!(" {} {} ", fold(&article.title), fold(&article.summary));
        project.keywords.iter().any(|word| haystack.contains(word.as_str()))
    })
}

/// The catalogue bundled in the executable (shown at once, replaced by the site's).
pub fn bundled() -> Catalog {
    let launcher: Value = serde_json::from_str(include_str!("../assets/launcher.json")).unwrap_or_default();
    let sheets: Value = serde_json::from_str(include_str!("../assets/projects.json")).unwrap_or_default();
    build(&launcher, &sheets)
}

/// The site's catalogue (both requests in this thread); None when the site cannot be reached.
pub fn fetch() -> Option<Catalog> {
    let get = |url: &str| -> Option<Value> {
        let text = ureq::get(url).header("User-Agent", "InstallateurLiberlNews").call().ok()?.into_body().read_to_string().ok()?;
        serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
    };
    // LN_LAUNCHER_URL: a local site, to try a catalogue before it is online.
    let launcher = get(&std::env::var("LN_LAUNCHER_URL").unwrap_or_else(|_| LAUNCHER_URL.to_string()))?;
    let sheets = get(games::PROJECTS_URL).unwrap_or_else(|| serde_json::from_str(include_str!("../assets/projects.json")).unwrap_or_default());
    Some(build(&launcher, &sheets))
}
