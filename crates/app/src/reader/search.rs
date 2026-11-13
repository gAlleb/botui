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
    geometry: (u16, usize),
    hits: Vec<SearchMatch>,
    selected: usize,
    query: String,
    hidden: bool,
}

impl Search {
    pub fn index(&mut self, pager: &Pager, doc: &Document) -> Rc<SearchIndex> {
        self.update(pager, doc);
        self.geometry = (pager.column_width(), pager.viewport());
        self.index
            .get_or_insert_with(|| Rc::new(pager.search_index(doc)))
            .clone()
    }
    pub fn select(&mut self, query: String, hits: Vec<SearchMatch>, selected: usize) {
        self.hidden = false;
        self.query = query;
        self.hits = hits;
        self.selected = selected;
    }
    pub fn highlighted(&self) -> bool {
        !self.hidden && !self.hits.is_empty()
    }
    pub fn hide(&mut self) {
        self.hidden = true;
    }
    pub fn update(&mut self, pager: &Pager, doc: &Document) {
        if !doc.styled.is_empty() && self.geometry != (pager.column_width(), pager.viewport()) {
            self.reflow(pager, doc);
        }
    }
    pub fn reflow(&mut self, pager: &Pager, doc: &Document) {
        self.geometry = (pager.column_width(), pager.viewport());
        if self.index.is_none() {
            return;
        }
        // A media query can hide the current match after a resize.
        let previous = self.hits.get(self.selected).map(|hit| hit.anchor);
        let index = Rc::new(pager.search_index(doc));
        self.hits = index.matches(&self.query);
        self.selected = previous.map_or(0, |at| {
            self.hits
                .partition_point(|hit| hit.anchor < at)
                .min(self.hits.len().saturating_sub(1))
        });
        self.index = Some(index);
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
        self.hidden = false;
        self.selected = if forward {
            self.hits.partition_point(|h| h.anchor <= place) % count
        } else {
            (self.hits.partition_point(|h| h.anchor < place) + count - 1) % count
        };
        Some(self.hits[self.selected].anchor)
    }
    pub fn line<'a>(&self, line: &'a PageLine, theme: crate::theme::Theme) -> Line<'a> {
        let hit = self
            .hits
            .get(self.selected)
            .filter(|_| !self.hidden)
            .and_then(|h| self.index.as_ref()?.visible_range(h, line));
        if hit.is_none() && line.spans.iter().all(|s| s.face == Default::default()) {
            return Line::raw(&line.text);
        }
        let mut styled = Vec::new();
        let mut byte = 0;
        let mut push =
            |start: usize, end: usize, style: Modifier, face: botui_core::publisher::Face| {
                if start == end {
                    return;
                }
                let mut cuts = [start, start, end, end];
                if let Some(hit) = &hit {
                    cuts[1] = hit.start.clamp(start, end);
                    cuts[2] = hit.end.clamp(start, end);
                }
                for (a, b) in cuts.iter().zip(cuts.iter().skip(1)).filter(|(a, b)| a < b) {
                    let modifier = if hit.as_ref().is_some_and(|h| h.contains(a)) {
                        style | Modifier::REVERSED
                    } else {
                        style
                    };
                    let face = super::markdown_style::face(face, theme).add_modifier(modifier);
                    styled.push(Span::styled(&line.text[*a..*b], face));
                }
            };
        for s in &line.spans {
            push(byte, s.bytes.start, Modifier::empty(), Default::default());
            let mut style = Modifier::empty();
            style.set(Modifier::BOLD, s.face.bold);
            style.set(Modifier::ITALIC, s.face.italic);
            style.set(Modifier::UNDERLINED, s.face.underline);
            style.set(Modifier::CROSSED_OUT, s.face.strike);
            push(s.bytes.start, s.bytes.end, style, s.face);
            byte = s.bytes.end;
        }
        push(byte, line.text.len(), Modifier::empty(), Default::default());
        Line::from(styled)
    }
}
