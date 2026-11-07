//! The native details and personal reading state APIs are separate from file positions.
use super::{BookOrbit, Method, Result, read_json};
use serde::Deserialize;
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Detail {
    pub title: Option<String>,
    pub authors: Vec<Name>,
    pub description: Option<String>,
    pub series_name: Option<String>,
    pub series_index: Option<serde_json::Value>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub isbn13: Option<String>,
    pub isbn10: Option<String>,
    pub page_count: Option<u64>,
    pub published_date: Option<String>,
    pub published_year: Option<i32>,
    pub library_name: Option<String>,
    pub tags: Vec<String>,
    pub genres: Vec<String>,
    pub added_at: Option<String>,
    pub read_status: Option<UserStatus>,
}
#[derive(Deserialize)]
pub struct Name {
    pub name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStatus {
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}
impl BookOrbit {
    pub fn detail(&self, token: &str, id: u64) -> Result<Detail> {
        read_json(self.authorized(Method::GET, &format!("books/{id}"), token)?)
    }
    pub fn hold(&self, token: &str, id: u64, resume: bool) -> Result<UserStatus> {
        let status = if resume { "reading" } else { "on_hold" };
        read_json(
            self.authorized(Method::PATCH, &format!("books/{id}/status"), token)?
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&serde_json::json!({"status":status}))?),
        )
    }
    pub fn reset_reading(&self, token: &str, id: u64) -> Result<UserStatus> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Reset {
            read_status: UserStatus,
        }
        let r: Reset = read_json(self.authorized(
            Method::POST,
            &format!("books/{id}/reset-reading-state"),
            token,
        )?)?;
        Ok(r.read_status)
    }
}
