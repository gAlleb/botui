//! Keep book-wide text search available after its results dialog closes.
use botui_core::{Anchor, Document, PageLine, Pager, SearchIndex, SearchMatch};
use ratatui::{
    style::Modifier,
    text::{Line, Span},
};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct Search {
    index: Option<Rc<SearchIndex>>,
    hits: Vec<SearchMatch>,
    selected: usize,
    query: String,
}

impl Search {
    pub fn index(&mut self, pager: &Pager, doc: &Document) -> Rc<SearchIndex> {
        self.index
            .get_or_insert_with(|| Rc::new(pager.search_index(doc)))
            .clone()
    }
    pub fn select(&mut self, query: String, hits: Vec<SearchMatch>, selected: usize) {
        self.query = query;
        self.hits = hits;
        self.selected = selected;
    }
    pub fn restore(&self, nav: &mut super::navigation::Navigation) {
        nav.query.clone_from(&self.query);
        nav.hits.clone_from(&self.hits);
        nav.show_results(self.selected);
    }
    pub fn advance(&mut self, place: Anchor, forward: bool) -> Option<Anchor> {
        let count = self.hits.len();
        if count == 0 {
            return None;
        }
        self.selected = if forward {
            self.hits.partition_point(|h| h.anchor <= place) % count
        } else {
            (self.hits.partition_point(|h| h.anchor < place) + count - 1) % count
        };
        Some(self.hits[self.selected].anchor)
    }
    pub fn line<'a>(&self, line: &'a PageLine) -> Line<'a> {
        if let Some(hit) = self.hits.get(self.selected)
            && let Some(bytes) = self.index.as_ref().and_then(|i| i.visible_range(hit, line))
        {
            Line::from(vec![
                Span::raw(&line.text[..bytes.start]),
                Span::styled(&line.text[bytes.clone()], Modifier::REVERSED),
                Span::raw(&line.text[bytes.end..]),
            ])
        } else {
            Line::raw(&line.text)
        }
    }
}
