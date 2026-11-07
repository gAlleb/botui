//! Map BookOrbit transport objects to source-independent rows at the adapter boundary.
use super::{BooksPage, Command, PAGE_SIZE, Row, Scope, Session};
use crate::catalog::{Book, BookFile};
impl From<botui_bookorbit::Book> for Book {
    fn from(b: botui_bookorbit::Book) -> Self {
        Self {
            id: b.id,
            title: b.title,
            authors: b.authors,
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
