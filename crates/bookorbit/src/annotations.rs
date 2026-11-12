//! Reader-file projections of BookOrbit's ordinary annotations.
use super::{BookOrbit, Result, checked, read_json};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pdf {
    /// Zero-based page; unscaled points, top-left origin.
    pub page: u32,
    pub rect: Rect,
    pub rects: Vec<Rect>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: u64,
    pub cfi: Option<String>,
    pub pdf: Option<Pdf>,
    pub jump_file_id: Option<u64>,
    pub text: String,
    pub color: String,
    pub style: String,
    pub note: Option<String>,
    pub updated_at: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct New {
    pub book_file_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cfi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf: Option<Pdf>,
    pub text: String,
    pub color: String,
    pub style: String,
    pub note: Option<String>,
}
impl New {
    pub fn matches(&self, a: &Annotation) -> bool {
        self.cfi == a.cfi
            && self.pdf == a.pdf
            && self.text == a.text
            && self.color.eq_ignore_ascii_case(&a.color)
            && self.style == a.style
            && self.note.as_deref().unwrap_or("") == a.note.as_deref().unwrap_or("")
            && a.jump_file_id == Some(self.book_file_id)
    }
}
impl BookOrbit {
    pub fn annotations(&self, token: &str, book: u64, file: u64) -> Result<Vec<Annotation>> {
        read_json(
            self.authorized(
                Method::GET,
                &format!("books/{book}/annotations?readerFileId={file}"),
                token,
            )?
            .timeout(Duration::from_secs(5)),
        )
    }
    pub fn add_annotation(&self, token: &str, book: u64, annotation: &New) -> Result<Annotation> {
        read_json(
            self.authorized(Method::POST, &format!("books/{book}/annotations"), token)?
                .timeout(Duration::from_secs(5))
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(annotation)?),
        )
    }
    // Do not change the browser's style, star, position or quote when editing a comment/color.
    pub fn edit_annotation(
        &self,
        token: &str,
        book: u64,
        id: u64,
        note: &str,
        color: &str,
    ) -> Result<Annotation> {
        read_json(self.authorized(Method::PATCH, &format!("books/{book}/annotations/{id}"), token)?
            .timeout(Duration::from_secs(5)).header("Content-Type", "application/json")
            .body(serde_json::to_vec(&serde_json::json!({"note": if note.trim().is_empty() {None} else {Some(note)}, "color":color}))?))
    }
    pub fn remove_annotation(&self, token: &str, book: u64, id: u64) -> Result<()> {
        checked(
            self.authorized(
                Method::DELETE,
                &format!("books/{book}/annotations/{id}"),
                token,
            )?
            .timeout(Duration::from_secs(5))
            .send()?,
        )?;
        Ok(())
    }
}
