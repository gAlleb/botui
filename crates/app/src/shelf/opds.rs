//! Translate link-based OPDS entries at the edge; the shelf uses session-local handles.
use super::{Book, BookFile, Command, Loaded, Pane, Row, Section, Shelf, login};
use botui_opds::{Address, Catalog, Entry, Feed};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(super) struct Links {
    ids: HashMap<Address, u64>,
    files: HashMap<u64, Address>,
    pub covers: HashMap<u64, Address>,
}
impl Links {
    fn id(&mut self, href: &Address) -> u64 {
        let n = self.ids.len() as u64 + 1;
        *self.ids.entry(href.clone()).or_insert(n)
    }
    pub fn file(&self, id: u64) -> Option<Address> {
        self.files.get(&id).cloned()
    }
}
impl Shelf {
    pub(crate) fn opds(address: String) -> Self {
        let mut shelf = Self::new(String::new(), String::new());
        shelf.is_opds = true;
        let mut form = login::Login::new(address, String::new());
        form.opds = true;
        shelf.login = Some(form);
        shelf
    }
    pub(super) fn submit_opds(&mut self) {
        let Some(form) = &mut self.login else { return };
        let address = form.server.trim().to_owned();
        let user = form.username.clone();
        let pass = std::mem::take(&mut form.password);
        self.fetch("Открываем каталог OPDS…", move |_| {
            let cat = Arc::new(Catalog::new(&address, user, pass)?);
            let feed = cat.feed(cat.root())?;
            Ok(Loaded::Feed(cat, feed, false))
        });
    }
    pub(super) fn fetch_feed(&mut self, href: Address, replace: bool) {
        let Some(cat) = self.catalog.clone() else {
            return;
        };
        self.fetch("Загружаем раздел OPDS…", move |_| {
            let feed = cat.feed(href)?;
            Ok(Loaded::Feed(cat, feed, replace))
        });
    }
    pub(super) fn show_feed(&mut self, cat: Arc<Catalog>, feed: Feed, replace: bool) {
        self.login = None;
        self.catalog = Some(cat);
        if replace && self.stack.len() > 1 {
            self.stack.pop();
        }
        let mut rows = vec![];
        for e in feed.items {
            match e {
                Entry::Directory(name, href) => {
                    rows.push(Row::new(name, Command::Feed(href, false)))
                }
                Entry::Book {
                    title,
                    authors,
                    files,
                    cover,
                } => {
                    let id = self.links.id(&files[0].href);
                    let has_cover = Some(cover.is_some());
                    if let Some(href) = cover {
                        self.links.covers.insert(id, href);
                    }
                    let files = files
                        .into_iter()
                        .map(|f| {
                            let file_id = self.links.id(&f.href);
                            self.links.files.insert(file_id, f.href);
                            BookFile {
                                id: file_id,
                                format: Some(f.format),
                                role: "content".into(),
                                size_bytes: f.size,
                            }
                        })
                        .collect();
                    let book = Book {
                        id,
                        title: Some(title.clone()),
                        authors,
                        files,
                        has_cover,
                        progress: None,
                    };
                    rows.push(Row::new(
                        format!(
                            "[{}]  {}   ·   {}",
                            super::gallery::formats(&book),
                            title,
                            book.authors.join(", ")
                        ),
                        Command::Files(book),
                    ));
                }
            }
        }
        if let Some(prev) = feed.previous.clone() {
            rows.push(Row::new("‹ Предыдущая страница", Command::Feed(prev, true)));
        }
        if let Some(next) = feed.next.clone() {
            rows.push(Row::new("Следующая страница ›", Command::Feed(next, true)));
        }
        self.stack.push(Pane::new(
            Section::Feed {
                next: feed.next,
                prev: feed.previous,
            },
            feed.title,
            rows,
        ));
    }
}
