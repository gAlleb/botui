//! Map BookOrbit transport objects to source-independent rows at the adapter boundary.
use super::{BooksPage, Command, PAGE_SIZE, Row, Scope, Session};
use crate::catalog::{Book, BookFile};
impl From<botui_bookorbit::Book> for Book {
    fn from(b: botui_bookorbit::Book) -> Self {
        Self {
            id: b.id,
            title: b.title,
            authors: b.authors,
            jacket: crate::catalog::Jacket::Bauhaus {
                series: b.series_name,
                index: b.series_index.and_then(|v| match v {
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    serde_json::Value::String(s) => Some(s),
                    _ => None,
                }),
            },
            progress: b.progress,
            has_cover: b.has_cover,
            files: b
                .files
                .into_iter()
                .map(|f| BookFile {
                    id: f.id,
                    format: f.format,
                    role: f.role,
                    size_bytes: f.size_bytes,
                })
                .collect(),
        }
    }
}
pub(super) fn book_rows(items: Vec<botui_bookorbit::Book>) -> Vec<Row> {
    items
        .into_iter()
        .map(|b| {
            Row::new(
                b.title.clone().unwrap_or_else(|| "Untitled".into()),
                Command::Files(b.into()),
            )
        })
        .collect()
}
pub(super) fn fetch_books(
    session: &Session,
    scope: &Scope,
    page: u32,
) -> botui_bookorbit::Result<BooksPage> {
    use botui_bookorbit::CatalogOrder;
    session.with_token(|api, t| match scope {
        Scope::Group(directory, group) => api.group_books(t, *directory, group.id, page, PAGE_SIZE),
        Scope::All => api.all_books(t, page, PAGE_SIZE),
        Scope::Library(l) => api.library_books(t, l.id, page, PAGE_SIZE),
        Scope::Collection(c) => api.books(t, c.id, page, PAGE_SIZE),
        Scope::Search(q) => {
            api.catalog_query(t, None, page, PAGE_SIZE, CatalogOrder::Title, Some(q))
        }
        Scope::Reading => api.catalog_query(t, None, page, PAGE_SIZE, CatalogOrder::Reading, None),
        Scope::Recent => api.catalog_query(t, None, page, PAGE_SIZE, CatalogOrder::Recent, None),
    })
}

pub(super) fn next_group_page(page: &super::DirectoryPage) -> Option<u32> {
    (!page.items.is_empty()
        && (u64::from(page.page) + 1) * u64::from(page.size.max(1)) < page.total)
        .then(|| page.page + 1)
}
pub(super) fn group_rows(directory: super::Directory, groups: Vec<super::Group>) -> Vec<Row> {
    groups
        .into_iter()
        .map(|group| {
            Row::new(
                group.book_count.map_or_else(
                    || group.name.clone(),
                    |n| format!("{}   ·   {n}", group.name),
                ),
                Command::Books(Scope::Group(directory, group), 0),
            )
        })
        .collect()
}
