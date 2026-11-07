//! The server projects each bookmark onto the requested reader file where possible.
use super::{BookOrbit, Result, checked, read_json};
use reqwest::Method;
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct Bookmark {
    pub id: u64,
    pub title: String,
    pub cfi: Option<String>,
}

impl BookOrbit {
    pub fn bookmarks(&self, token: &str, book: u64, file: u64) -> Result<Vec<Bookmark>> {
        read_json(
            self.authorized(
                Method::GET,
                &format!("books/{book}/bookmarks?readerFileId={file}"),
                token,
            )?
            .timeout(Duration::from_secs(5)),
        )
    }
    pub fn add_bookmark(
        &self,
        token: &str,
        book: u64,
        file: u64,
        cfi: &str,
        title: &str,
    ) -> Result<Bookmark> {
        let body = serde_json::json!({"bookFileId":file,"cfi":cfi,"title":title});
        read_json(
            self.authorized(Method::POST, &format!("books/{book}/bookmarks"), token)?
                .timeout(Duration::from_secs(5))
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&body)?),
        )
    }
    pub fn remove_bookmark(&self, token: &str, book: u64, id: u64) -> Result<()> {
        checked(
            self.authorized(
                Method::DELETE,
                &format!("books/{book}/bookmarks/{id}"),
                token,
            )?
            .timeout(Duration::from_secs(5))
            .send()?,
        )?;
        Ok(())
    }
}
