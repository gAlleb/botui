//! Hit regions follow the rendered byte span of each link through reflow.
use botui_core::{Anchor, Document, PageLine, Reference};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::Line,
};
use std::collections::BTreeMap;

struct Return {
    top: Anchor,
    from: Anchor,
}
pub(super) struct Links {
    by_block: BTreeMap<usize, Vec<usize>>,
    hits: Vec<(Rect, usize)>,
    returns: Vec<Return>,
    back: Rect,
    keep: Rect,
}
impl Links {
    pub fn new(doc: &Document) -> Self {
        let mut by_block: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (i, r) in doc.references.iter().enumerate() {
            if r.bytes.is_some() {
                by_block.entry(r.source).or_default().push(i);
            }
        }
        Self {
            by_block,
            hits: vec![],
            returns: vec![],
            back: Rect::default(),
            keep: Rect::default(),
        }
    }
    pub fn clear(&mut self) {
        self.hits.clear();
        self.back = Rect::default();
        self.keep = Rect::default();
    }
    pub fn mark_line(&mut self, f: &mut Frame, doc: &Document, line: &PageLine, rect: Rect) {
        if self.by_block.is_empty() {
            return;
        }
        let mut blocks = vec![line.anchor().block];
        for at in line.spans.iter().filter_map(|span| span.source) {
            if blocks.last() != Some(&at.block) {
                blocks.push(at.block);
            }
        }
        blocks.sort_unstable();
        blocks.dedup();
        let offset = if line.heading {
            rect.width
                .saturating_sub(Line::raw(&line.text).width() as u16)
                / 2
        } else {
            0
        };
        for &i in blocks.iter().filter_map(|b| self.by_block.get(b)).flatten() {
            if let Some(bytes) = line.link_range(doc, &doc.references[i]) {
                let x = rect.x + offset + Line::raw(&line.text[..bytes.start]).width() as u16;
                let w = Line::raw(&line.text[bytes]).width() as u16;
                let hit = Rect::new(x, rect.y, w, 1).intersection(rect);
                if !hit.is_empty() {
                    f.buffer_mut()
                        .set_style(hit, Style::new().add_modifier(Modifier::UNDERLINED));
                    self.hits.push((hit, i));
                }
            }
        }
    }
    pub fn image(&mut self, rect: Rect, id: usize) {
        self.hits.push((rect, id));
    }
    pub fn has_visible(&self) -> bool {
        !self.hits.is_empty()
    }
    pub fn visible(&self) -> Vec<usize> {
        let mut ids = self.hits.iter().map(|(_, id)| *id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
    pub fn hit(&self, pos: Position) -> Option<super::Action> {
        if self.back.contains(pos) {
            return Some(super::Action::ReturnLink);
        }
        if self.keep.contains(pos) {
            return Some(super::Action::KeepLink);
        }
        self.hits
            .iter()
            .find(|(rect, _)| rect.contains(pos))
            .map(|(_, id)| super::Action::Follow(*id))
    }
    pub fn visiting(&self) -> bool {
        !self.returns.is_empty()
    }
    pub fn origin(&self, current: Anchor) -> Anchor {
        self.returns.first().map_or(current, |r| r.top)
    }
    pub fn destination(&mut self, top: Anchor, link: &Reference, to: Anchor) -> Anchor {
        if let Some(i) = self.returns.iter().rposition(|r| {
            r.from == to || (link.backlink && r.from.block == to.block && to.byte == 0)
        }) {
            let previous = self.returns[i].top;
            self.returns.truncate(i);
            return previous;
        }
        self.returns.push(Return {
            top,
            from: Anchor {
                block: link.source,
                byte: link.bytes.as_ref().map_or(0, |r| r.start),
            },
        });
        to
    }
    pub fn back(&mut self) -> Option<Anchor> {
        self.returns.pop().map(|r| r.top)
    }
    pub fn keep(&mut self) {
        self.returns.clear();
    }
    pub fn controls(&mut self, f: &mut Frame, area: Rect, style: Style) {
        use ratatui::widgets::Paragraph;
        let mut x = area.x;
        for (label, rect) in [
            ("[ Back · Esc ]", &mut self.back),
            (" [ Read here · Enter ]", &mut self.keep),
        ] {
            let width = (Line::raw(label).width() as u16).min(area.right().saturating_sub(x));
            *rect = Rect::new(x, area.y, width, area.height);
            f.render_widget(
                Paragraph::new(label).style(style.add_modifier(Modifier::BOLD)),
                *rect,
            );
            x += width;
        }
    }
}
