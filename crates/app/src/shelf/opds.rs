//! Translate link-based OPDS entries at the edge; the shelf uses session-local handles.
use super::{Book, BookFile, Command, Loaded, Pane, Row, Section, Shelf, login};
use botui_opds::{Address, Catalog, Entry, Feed};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(super) struct Links {
    ids: HashMap<Address, u64>,
    files: HashMap<u64, Address>,
    pub covers: HashMap<u64, Address>,
    pub originals: HashMap<u64, Address>,
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
    pub(crate) fn open_opds(&mut self, c: crate::opds_settings::Catalog) {
        if let Some(form) = &mut self.login {
            form.server = c.url.clone();
            form.username = c.username.clone();
        }
        self.fetch("Opening OPDS catalog…", move |cancel| {
            let pass = c.password(cancel)?;
            let cat = Arc::new(Catalog::new(&c.url, c.username, pass)?);
            let href = cat.root();
            let feed = cat.feed(href.clone())?;
            Ok(Loaded::Feed(cat, Box::new(feed), href))
        });
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
    pub(super) fn feed_search(&self) -> Option<&botui_opds::Search> {
        self.stack.iter().rev().find_map(|p| p.search.as_ref())
    }
    pub(super) fn search_feed(&mut self, query: String) {
        let (Some(cat), Some(search)) = (self.catalog.clone(), self.feed_search().cloned()) else {
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
        let search = feed.search;
        let rows = self.feed_rows(feed.items);
        self.stack.push(Pane::new(
            Section::Feed {
                next: feed.next,
                visited: [href].into_iter().collect(),
            },
            feed.title,
            rows,
        ));
        self.current_mut().search = search;
    }
    pub(super) fn feed_rows(&mut self, items: Vec<Entry>) -> Vec<Row> {
        let mut rows = vec![];
        for e in items {
            match e {
                Entry::Group(name, items) => rows.push(Row::new(
                    name.clone(),
                    Command::InlineFeed(name, Arc::new(items)),
                )),
                Entry::Directory(name, href) => rows.push(Row::new(name, Command::Feed(href))),
                Entry::Book {
                    title,
                    authors,
                    files,
                    cover,
                    full_cover,
                } => {
                    let Some(first) = files.first() else {
                        rows.push(Row::new(
                            format!("{title} · no direct download"),
                            Command::NoAcquisition,
                        ));
                        continue;
                    };
                    let id = self.links.id(&first.href);
                    let has_cover = Some(cover.is_some());
                    if let Some(href) = full_cover {
                        self.links.originals.insert(id, href);
                    }
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
                        series: None,
                        series_index: None,
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
