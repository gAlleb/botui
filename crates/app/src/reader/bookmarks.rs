use botui_bookorbit::{Session, bookmarks::Bookmark};
use botui_core::{Anchor, Document};
use std::sync::Arc;

pub(crate) struct ServerMarks {
    session: Arc<Session>,
    book: u64,
    file: u64,
    items: Vec<Bookmark>,
}
impl ServerMarks {
    pub fn load(session: Arc<Session>, book: u64, file: u64) -> botui_bookorbit::Result<Self> {
        let items = session.with_token(|api, t| api.bookmarks(t, book, file))?;
        Ok(Self {
            session,
            book,
            file,
            items,
        })
    }
    pub fn entries(&self, doc: &Document) -> Vec<(u64, Option<Anchor>, String)> {
        self.items
            .iter()
            .map(|b| {
                (
                    b.id,
                    b.cfi.as_deref().and_then(|c| doc.bookmark_anchor(c)),
                    b.title.clone(),
                )
            })
            .collect()
    }
    pub fn remove(&mut self, id: u64) -> botui_bookorbit::Result<()> {
        let index = self
            .items
            .iter()
            .position(|b| b.id == id)
            .ok_or("Bookmark is no longer in this list.")?;
        match self
            .session
            .with_token(|api, t| api.remove_bookmark(t, self.book, id))
        {
            Ok(()) => {
                self.items.remove(index);
            }
            Err(e) if botui_bookorbit::is_not_found(e.as_ref()) => {
                // The book or route can also return 404. Only a fresh list proves that
                // another device already removed this particular bookmark.
                let current = self
                    .session
                    .with_token(|api, t| api.bookmarks(t, self.book, self.file))?;
                if current.iter().any(|b| b.id == id) {
                    return Err(e);
                }
                self.items = current;
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }
    pub fn toggle(&mut self, doc: &Document, place: Anchor) -> botui_bookorbit::Result<()> {
        let mut matches = self
            .items
            .iter()
            .filter(|b| b.cfi.as_deref().and_then(|c| doc.bookmark_anchor(c)) == Some(place));
        if let Some(bookmark) = matches.next() {
            let id = bookmark.id;
            if matches.next().is_some() {
                return Err(
                    "Several bookmarks share this position. Press G and select the one to delete."
                        .into(),
                );
            }
            self.remove(id)?;
        } else {
            let cfi = doc
                .cfi(place)
                .ok_or("This passage has no exact BookOrbit address")?;
            let title = doc
                .blocks
                .get(place.block)
                .and_then(|b| b.text.get(place.byte..))
                .filter(|s| !s.is_empty())
                .unwrap_or(&doc.title);
            let title: String = title.chars().take(100).collect();
            let b = self
                .session
                .with_token(|api, t| api.add_bookmark(t, self.book, self.file, &cfi, &title))?;
            self.items.push(b);
        }
        Ok(())
    }
}
