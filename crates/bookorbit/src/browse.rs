//! The author and series directories use the signed-in user's library permissions.
use crate::{BookOrbit, BooksPage, Method, Result, read_json};
use serde::Deserialize;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Directory {
    Authors,
    Series,
}

impl Directory {
    fn path(self) -> &'static str {
        match self {
            Self::Authors => "authors",
            Self::Series => "series",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: u64,
    pub name: String,
    pub book_count: u64,
}

#[derive(Deserialize)]
pub struct DirectoryPage {
    pub items: Vec<Group>,
    pub total: u64,
    pub page: u32,
    pub size: u32,
}
impl BookOrbit {
    pub fn directory(
        &self,
        token: &str,
        directory: Directory,
        query: &str,
        page: u32,
        size: u32,
    ) -> Result<DirectoryPage> {
        let mut url = self.root.join(directory.path())?;
        url.query_pairs_mut()
            .append_pair("page", &page.to_string())
            .append_pair("size", &size.to_string())
            .append_pair("sort", "name")
            .append_pair("order", "asc");
        if !query.is_empty() {
            url.query_pairs_mut().append_pair("q", query);
        }
        read_json(self.authorized(Method::GET, url.as_str(), token)?)
    }

    pub fn group_books(
        &self,
        token: &str,
        directory: Directory,
        id: u64,
        page: u32,
        size: u32,
    ) -> Result<BooksPage> {
        let sort = match directory {
            Directory::Authors => "title",
            Directory::Series => "seriesIndex",
        };
        let flat = if directory == Directory::Authors {
            "&collapseSeries=false"
        } else {
            ""
        };
        let path = format!(
            "{}/{id}/books?page={page}&size={size}&sort={sort}&order=asc{flat}",
            directory.path()
        );
        read_json(self.authorized(Method::GET, &path, token)?)
    }
}
