//! Shelf presentation data shared by OPDS, the local folder and the native adapter.
//! IDs are handles owned by the active source, not global identifiers or URLs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    pub id: u64,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub has_cover: Option<bool>,
    pub progress: Option<u8>,
    pub files: Vec<BookFile>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookFile {
    pub id: u64,
    pub format: Option<String>,
    pub role: String,
    pub size_bytes: Option<u64>,
}

/// Use the same credits for a local jacket in the library and in the reader.
pub fn local_credits(
    path: &std::path::Path,
    meta: Option<&botui_formats::BookMetadata>,
) -> (String, Vec<String>) {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let (by, name) = stem.split_once(" - ").unwrap_or(("", &stem));
    let title = meta
        .map(|m| m.title.trim())
        .filter(|t| !t.is_empty())
        .unwrap_or(name);
    let authors = match meta {
        Some(m) if !m.authors.is_empty() => m.authors.clone(),
        _ if !by.is_empty() => vec![by.to_owned()],
        _ => vec![],
    };
    (title.to_owned(), authors)
}
