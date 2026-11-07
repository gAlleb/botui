use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub id: u64,
    pub name: String,
    pub book_count: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: u64,
    pub name: String,
    pub book_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookFile {
    pub id: u64,
    pub format: Option<String>,
    pub role: String,
    pub size_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Book {
    pub id: u64,
    pub title: Option<String>,
    pub authors: Vec<String>,
    #[serde(rename = "hasCover")]
    pub has_cover: Option<bool>,
    #[serde(default, rename = "readingProgress", deserialize_with = "read_percent")]
    pub progress: Option<u8>,
    pub files: Vec<BookFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BooksPage {
    pub items: Vec<Book>,
    pub total: u64,
    pub page: u32,
    pub size: u32,
}

fn read_percent<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u8>, D::Error> {
    Ok(Option::<f64>::deserialize(d)?
        .filter(|v| v.is_finite() && *v > 0.)
        .map(|v| v.round().clamp(1., 100.) as u8))
}
