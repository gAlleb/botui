use super::super::{Action, links::Links};
use botui_core::{Anchor, Document, LayoutMode, Pager, Target};
use ratatui::{
    Frame,
    layout::{Alignment, Position, Rect},
    widgets::Paragraph,
};

/// The note has its own viewport, but each link destination belongs to the book.
pub(super) struct NotePage<'a> {
    doc: &'a Document,
    pager: Pager,
    links: Links,
    selected: Option<usize>,
}

impl<'a> NotePage<'a> {
    pub fn new(doc: &'a Document, area: Rect) -> Self {
        Self {
            doc,
            pager: Pager::with_reflow(doc, area.width, area.height, LayoutMode::Scroll, false),
            links: Links::new(doc),
            selected: None,
        }
    }

    pub fn visible_height(&mut self, area: Rect) -> usize {
        if self.pager.column_width() != area.width
            || self.pager.viewport() != usize::from(area.height.max(1))
        {
            self.pager
                .relayout(self.doc, area.width, area.height, LayoutMode::Scroll);
            if let Some(id) = self.selected {
                self.show_link(id);
            }
            // A larger viewport can now contain rows above the old scroll position.
            self.pager.scroll(0);
        }
        self.pager.lines(0).len()
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        self.links.clear();
        if area.is_empty() {
            return;
        }
        for (y, line) in self.pager.lines(0).iter().enumerate() {
            let row = Rect::new(area.x, area.y + y as u16, area.width, 1).intersection(area);
            let align = if line.heading {
                Alignment::Center
            } else {
                Alignment::Left
            };
            frame.render_widget(Paragraph::new(line.text.as_str()).alignment(align), row);
            self.links.mark_line(frame, self.doc, line, row);
        }
        if let Some(id) = self.selected {
            self.links.highlight(frame, id);
        }
    }

    pub fn scroll(&mut self, rows: isize) {
        self.selected = None;
        self.pager.scroll(rows);
    }

    pub fn select(&mut self, forward: bool) {
        let refs = &self.doc.references;
        if refs.is_empty() {
            return;
        }
        let visible = self.links.visible();
        let len = refs.len();
        let id = match (self.selected, forward) {
            (Some(i), true) => (i + 1) % len,
            (Some(i), false) => (i + len - 1) % len,
            (None, true) => visible.first().copied().unwrap_or_else(|| {
                refs.partition_point(|r| r.source < self.pager.anchor().block) % len
            }),
            (None, false) => visible.last().copied().unwrap_or_else(|| {
                (refs.partition_point(|r| r.source < self.pager.anchor().block) + len - 1) % len
            }),
        };
        self.selected = Some(id);
        if !visible.contains(&id) {
            self.show_link(id);
        }
    }

    fn show_link(&mut self, id: usize) {
        let link = &self.doc.references[id];
        self.pager.seek(Anchor {
            block: link.source,
            byte: link.bytes.as_ref().map_or(0, |bytes| bytes.start),
        });
        // Keep the note preceding the selected arrow in view.
        self.pager
            .scroll(-(self.pager.viewport().saturating_sub(1) as isize));
    }

    pub fn destination(&self, click: Option<Position>) -> Option<Anchor> {
        let id = match click {
            None => self.selected?,
            Some(pos) => match self.links.hit(pos)? {
                Action::Follow(id) => id,
                _ => return None,
            },
        };
        match self.doc.references[id].target {
            Target::Passage(anchor) => Some(anchor),
            _ => None,
        }
    }
}
