//! Players' reviews of a translation. NOT CONNECTED YET: the service and its
//! moderation remain to be built on the site. The window already speaks this
//! contract, so only the server is missing:
//!
//!   GET  /plateforme/api/reviews.php?project=<id>
//!        -> {"reviews": [{"author": "…", "rating": 1-5, "text": "…", "date": "YYYY-MM-DD"}]}
//!        (published reviews only, newest first)
//!   POST /plateforme/api/reviews.php   {"project": "<id>", "rating": 1-5, "text": "…"}
//!        with "Authorization: Bearer <token>" from a sign-in through an existing
//!        service (Discord OAuth, as the community lives there; no installer account)
//!        -> {"status": "pending"}   (shown as « en attente de modération »)
//!
//! Until the GET answers, the window says the reviews are not open yet; it never
//! pretends to keep a review.

use serde_json::Value;

pub const URL: &str = "https://leliberlnews.fr/plateforme/api/reviews.php";

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

pub fn fetch(project: &str) -> Reviews {
    let url = format!("{URL}?project={project}");
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
