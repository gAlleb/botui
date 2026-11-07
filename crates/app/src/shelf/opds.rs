//! Translate link-based OPDS entries at the edge; the shelf uses session-local handles.
use super::{Book, BookFile, Command, Loaded, Pane, Row, Section, Shelf, login};
use botui_opds::{Address, Catalog, Entry, Feed};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(super) struct Links {
    pub search: Option<botui_opds::Search>,
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
        self.fetch("Opening OPDS catalog…", move |_| {
            let cat = Arc::new(Catalog::new(&address, user, pass)?);
            let href = cat.root();
            let feed = cat.feed(href.clone())?;
            Ok(Loaded::Feed(cat, Box::new(feed), href))
        });
    }
    pub(super) fn search_feed(&mut self, query: String) {
        let (Some(cat), Some(search)) = (self.catalog.clone(), self.links.search.clone()) else {
            return;
        };
        self.fetch("Searching OPDS…", move |_| {
            let href = cat.search_url(&search, &query)?;
            let mut feed = cat.feed(href.clone())?;
            feed.title = format!("Search · {query}");
            Ok(Loaded::Feed(cat, Box::new(feed), href))
        });
    }
    pub(super) fn fetch_feed(&mut self, href: Address) {
        let Some(cat) = self.catalog.clone() else {
            return;
        };
        self.fetch("Loading OPDS feed…", move |_| {
            let feed = cat.feed(href.clone())?;
            Ok(Loaded::Feed(cat, Box::new(feed), href))
        });
    }
    pub(super) fn show_feed(&mut self, cat: Arc<Catalog>, feed: Feed, href: Address) {
        self.login = None;
        self.catalog = Some(cat);
        if feed.search.is_some() {
            self.links.search = feed.search;
        }
        let rows = self.feed_rows(feed.items);
        self.stack.push(Pane::new(
            Section::Feed {
                next: feed.next,
                visited: [href].into_iter().collect(),
            },
            feed.title,
            rows,
        ));
    }
    pub(super) fn feed_rows(&mut self, items: Vec<Entry>) -> Vec<Row> {
        let mut rows = vec![];
        for e in items {
            match e {
                Entry::Directory(name, href) => rows.push(Row::new(name, Command::Feed(href))),
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
        rows
    }
}
