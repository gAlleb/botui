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
    pub fn entries(&self, doc: &Document) -> Vec<(Option<Anchor>, String)> {
        self.items
            .iter()
            .map(|b| {
                (
                    b.cfi.as_deref().and_then(|c| doc.bookmark_anchor(c)),
                    b.title.clone(),
                )
            })
            .collect()
    }
    pub fn toggle(&mut self, doc: &Document, place: Anchor) -> botui_bookorbit::Result<()> {
        if let Some(i) = self
            .items
            .iter()
            .position(|b| b.cfi.as_deref().and_then(|c| doc.bookmark_anchor(c)) == Some(place))
        {
            let id = self.items[i].id;
            self.session
                .with_token(|api, t| api.remove_bookmark(t, self.book, id))?;
            self.items.remove(i);
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
