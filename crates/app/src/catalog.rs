//! Shelf presentation data shared by OPDS, the local folder and the native adapter.
//! IDs are handles owned by the active source, not global identifiers or URLs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    pub id: u64,
    pub title: Option<String>,
    pub authors: Vec<String>,
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
