//! Players' reviews of a translation, through the platform (plateforme/api/reviews.php,
//! moderated on plateforme/avis.php). No account: a pseudonym, a rating, a text.
//!
//!   GET  /plateforme/api/reviews.php?project=<id>
//!        -> {"reviews": [{"author": "…", "rating": 1-5, "text": "…", "date": "YYYY-MM-DD"}]}
//!        (published reviews only, newest first)
//!   POST /plateforme/api/reviews.php   {"project", "author", "rating": 1-5, "text", "website": ""}
//!        -> 202 {"status": "pending"}   (shown as « en attente de modération »)
//!        -> 422/429 {"error": "…"}      (the reason, shown under the form)
//!
//! When the service cannot be reached, the window says so; it never pretends to keep a review.

use serde_json::Value;

pub const URL: &str = "https://leliberlnews.fr/plateforme/api/reviews.php";

/// The service (LN_REVIEWS_URL: a local site, to try it).
fn url() -> String {
    std::env::var("LN_REVIEWS_URL").unwrap_or_else(|_| URL.to_string())
}

#[derive(Clone)]
pub struct Review {
    pub author: String,
    pub rating: u8,
    pub text: String,
    pub date: String,
}

pub enum Reviews {
    /// The service does not exist yet (or cannot be reached).
    Closed,
    Open(Vec<Review>),
}

/// Sends a review; Err: the reason to show the player.
pub fn submit(project: &str, author: &str, rating: u8, text: &str) -> Result<(), String> {
    let body = serde_json::json!({ "project": project, "author": author, "rating": rating, "text": text, "website": "" });
    let response = ureq::post(&url())
        .header("User-Agent", "InstallateurLiberlNews")
        .header("Content-Type", "application/json")
        .config()
        .http_status_as_error(false)
        .build()
        .send(body.to_string())
        .map_err(|_| "Le service d'avis ne répond pas : réessayez plus tard.".to_string())?;
    let status = response.status().as_u16();
    let answer: Value = response.into_body().read_to_string().ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    if status == 202 || status == 200 {
        Ok(())
    } else {
        Err(answer.get("error").and_then(Value::as_str).unwrap_or("Envoi impossible.").to_string())
    }
}

/// The projects this player already sent a review for (kept in the installer's data folder).
pub fn sent() -> std::collections::HashSet<String> {
    std::fs::read_to_string(crate::patch::data_dir().join("avis-envoyes.txt")).unwrap_or_default().lines().map(str::to_string).collect()
}

pub fn remember(project: &str) {
    let mut all = sent();
    all.insert(project.to_string());
    let dir = crate::patch::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("avis-envoyes.txt"), all.into_iter().collect::<Vec<_>>().join("\n"));
}

pub fn fetch(project: &str) -> Reviews {
    let url = format!("{}?project={project}", url());
    let Ok(response) = ureq::get(&url).header("User-Agent", "InstallateurLiberlNews").call() else { return Reviews::Closed };
    let Ok(text) = response.into_body().read_to_string() else { return Reviews::Closed };
    let Ok(json) = serde_json::from_str::<Value>(&text) else { return Reviews::Closed };
    let Some(items) = json.get("reviews").and_then(Value::as_array) else { return Reviews::Closed };
    Reviews::Open(
        items
            .iter()
            .map(|item| Review {
                author: item.get("author").and_then(Value::as_str).unwrap_or_default().to_string(),
                rating: item.get("rating").and_then(Value::as_u64).unwrap_or(0).min(5) as u8,
                text: item.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
                date: item.get("date").and_then(Value::as_str).unwrap_or_default().to_string(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    /// Against a local site (LN_REVIEWS_URL): a review is accepted as pending, a bad one refused with its reason.
    #[test]
    #[ignore]
    fn sends_and_reads() {
        assert!(super::submit("sky-sc", "Essai", 4, "Avis envoyé par le test de l'installateur.").is_ok());
        let refused = super::submit("sky-sc", "E", 9, "court").unwrap_err();
        assert!(refused.contains("pseudo"), "{refused}");
        assert!(matches!(super::fetch("sky-fc"), super::Reviews::Open(_)));
    }
}
