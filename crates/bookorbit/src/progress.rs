//! A reading position always belongs to one concrete server file.
use super::{BookOrbit, Result, read_json};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub file_id: u64,
    pub percentage: f64,
    pub updated_at: Option<String>,
    pub text: TextPosition,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TextPosition {
    pub cfi: Option<String>,
    pub page: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Target {
    Text(String),
    Page(u32),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Move {
    pub target: Target,
    pub percentage: f64,
}

impl BookOrbit {
    pub fn progress(&self, token: &str, id: u64) -> Result<Progress> {
        #[derive(Deserialize)]
        struct List {
            items: Vec<Progress>,
        }
        let mut list: List = read_json(
            self.authorized(Method::GET, &format!("reading-progress?fileId={id}"), token)?
                .timeout(Duration::from_secs(4)),
        )?;
        list.items.retain(|p| p.file_id == id);
        if list.items.len() != 1 {
            return Err("No unique progress record for this file.".into());
        }
        Ok(list.items.remove(0))
    }
    pub fn save_move(&self, token: &str, base: &Progress, change: &Move) -> Result<bool> {
        // Older servers and a concurrent change must never turn this into an unconditional percentage write.
        if &self.progress(token, base.file_id)? != base {
            return Ok(false);
        }
        let text = match &change.target {
            Target::Text(cfi) if cfi.starts_with("epubcfi(") && cfi.ends_with(')') => {
                serde_json::json!({"cfi":cfi})
            }
            Target::Page(n) if *n > 0 => serde_json::json!({"page":n}),
            _ => return Err("Missing exact file position.".into()),
        };
        let body = serde_json::json!({"writes":[{"fileId":base.file_id,"kind":"text","text":text,"percentage":change.percentage.clamp(0.,100.),"editedAt":base.updated_at.as_deref().unwrap_or("1970-01-01T00:00:00.000Z")}]});
        #[derive(Deserialize)]
        struct Reply {
            results: Vec<Outcome>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Outcome {
            file_id: u64,
            status: String,
        }
        let reply: Reply = read_json(
            self.authorized(Method::PUT, "reading-progress", token)?
                .timeout(Duration::from_secs(5))
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&body)?),
        )?;
        match reply
            .results
            .iter()
            .find(|r| r.file_id == base.file_id)
            .map(|r| r.status.as_str())
        {
            Some("saved") => Ok(true),
            Some("superseded") => Ok(false),
            _ => Err("BookOrbit did not acknowledge the saved position.".into()),
        }
    }
}
