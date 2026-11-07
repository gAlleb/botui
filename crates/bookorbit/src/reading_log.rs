//! BookOrbit uses one-based pagination for a book's reading log.
use crate::{BookOrbit, Method, Result, read_json};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingLog {
    pub items: Vec<ReadingSession>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    pub stats: Stats,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total_seconds: u64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingSession {
    pub id: u64,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: u64,
    pub end_progress: Option<f64>,
    pub format: Option<String>,
    pub source: Option<String>,
    pub session_type: Option<String>,
}

impl ReadingLog {
    pub fn next(&self) -> Option<u32> {
        (!self.items.is_empty() && u64::from(self.page) * u64::from(self.page_size) < self.total)
            .then(|| self.page.checked_add(1))
            .flatten()
    }
}

impl BookOrbit {
    pub fn reading_log(&self, token: &str, book: u64, page: u32) -> Result<ReadingLog> {
        let data: ReadingLog = read_json(self.authorized(
            Method::GET,
            &format!("books/{book}/sessions?page={page}&pageSize=25&sortBy=startedAt&sortDir=desc"),
            token,
        )?)?;
        if data.page != page || data.page_size == 0 {
            return Err("BookOrbit returned an unexpected reading log page.".into());
        }
        Ok(data)
    }
}
