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
#[serde(rename_all = "camelCase")]
pub struct SeriesMembership {
    pub series_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Book {
    pub subtitle: Option<String>,
    #[serde(default)]
    pub narrators: Vec<String>,
    #[serde(default, rename = "seriesMemberships")]
    pub series_memberships: Vec<SeriesMembership>,
    pub id: u64,
    pub title: Option<String>,
    pub authors: Vec<String>,
    #[serde(rename = "seriesName")]
    pub series_name: Option<String>,
    #[serde(rename = "seriesIndex")]
    pub series_index: Option<serde_json::Value>,
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
        .filter(|v| v.is_finite() && *v >= 0.)
        .map(|v| {
            if v >= 100. {
                100
            } else {
                v.round().clamp(0., 99.) as u8
            }
        }))
}
