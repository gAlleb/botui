use crate::centered;
use botui_core::{Anchor, ContentsEntry, Document, Target};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    widgets::{Clear, List, ListState, Paragraph},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Contents,
    Search,
    Notes,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Edit {
    Back,
    Move(isize),
    Choose(usize),
    Fold(usize, bool),
    Type(char),
    Erase,
    Clear,
}
pub(super) struct Navigation {
    pub(super) index: Option<std::rc::Rc<botui_core::SearchIndex>>,
    pub(super) hits: Vec<botui_core::SearchMatch>,
    kind: Kind,
    pub(super) query: String,
    items: Vec<ContentsEntry>,
    depths: Vec<usize>,
    folded: std::collections::HashSet<usize>,
    visible: Vec<usize>,
    state: ListState,
    rows: Rect,
    back: Rect,
}
impl Navigation {
    pub fn new(kind: Kind, doc: &Document, place: Anchor) -> Self {
        let items: Vec<_> = match kind {
            Kind::Contents if !doc.contents.is_empty() => doc.contents.clone(),
            Kind::Contents => doc
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| b.heading)
                .map(|(i, b)| ContentsEntry::passage(Anchor { block: i, byte: 0 }, b.text.clone()))
                .collect(),
            Kind::Notes | Kind::Search => vec![],
        };
        let mut depths = Vec::with_capacity(items.len());
        for entry in &items {
            depths.push(entry.parent.map_or(0, |p| depths[p] + 1));
        }
        let i = current(&items, place);
        let state = ListState::default().with_selected((!items.is_empty()).then_some(i));
        Self {
            kind,
            index: None,
            hits: vec![],
            folded: Default::default(),
            visible: (0..items.len()).collect(),
            depths,
            items,
            state,
            rows: Rect::default(),
            back: Rect::default(),
            query: String::new(),
        }
    }
    pub fn draw(&mut self, f: &mut Frame, style: Style) {
        let area = centered(f.area(), 72, 22);
        self.draw_at(f, style, area, true);
    }
    pub fn follow(&mut self, place: Anchor) {
        if !self.items.is_empty() {
            let mut index = current(&self.items, place);
            loop {
                if let Ok(row) = self.visible.binary_search(&index) {
                    self.state.select(Some(row));
                    break;
                }
                let Some(parent) = self.items[index].parent else {
                    break;
                };
                index = parent;
            }
        }
    }
    fn group(&self, index: usize) -> bool {
        self.items
            .get(index + 1)
            .is_some_and(|next| next.parent == Some(index))
    }
    fn indent(&self, index: usize) -> usize {
        (self.depths[index] * 2).min(usize::from(self.rows.width.saturating_sub(5)))
    }
    fn item_index(&self, row: usize) -> Option<usize> {
        if self.kind == Kind::Contents {
            self.visible.get(row).copied()
        } else {
            (row < self.items.len()).then_some(row)
        }
    }
    pub fn supplement(&self, edit: Edit) -> Option<&ContentsEntry> {
        let Edit::Choose(row) = edit else {
            return None;
        };
        let entry = self.items.get(self.item_index(row)?)?;
        matches!(entry.target, Target::Supplement { .. }).then_some(entry)
    }
    fn fold(&mut self, row: usize, close: bool) {
        let Some(&index) = self.visible.get(row) else {
            return;
        };
        if close && (!self.group(index) || self.folded.contains(&index)) {
            if let Some(parent) = self.items[index].parent
                && let Ok(row) = self.visible.binary_search(&parent)
            {
                self.state.select(Some(row));
            }
            return;
        }
        if !self.group(index) {
            return;
        }
        if close {
            self.folded.insert(index);
        } else if !self.folded.remove(&index) {
            self.state.select(Some(row + 1));
            return;
        }
        self.visible.clear();
        let mut hidden_below = None;
        for (i, &depth) in self.depths.iter().enumerate() {
            if hidden_below.is_some_and(|d| depth > d) {
                continue;
            }
            self.visible.push(i);
            hidden_below = self.folded.contains(&i).then_some(depth);
        }
        self.state.select(Some(row));
    }
    pub fn draw_at(&mut self, f: &mut Frame, style: Style, area: Rect, popup: bool) {
        let title = match self.kind {
            Kind::Contents => " Contents ",
            Kind::Notes => " Reading notes ",
            Kind::Search => " Find in book ",
        };
        let panel = crate::appearance::block().title(title).style(style);
        let inner = panel.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(panel, area);
        let heading = if self.kind == Kind::Search {
            format!(
                " Find: {}▏   {}{}",
                self.query,
                self.items.len(),
                if self.items.len() == 5000 {
                    " · refine for more"
                } else {
                    " matches"
                }
            )
        } else {
            format!(
                " {}: {}",
                if self.kind == Kind::Contents {
                    "Sections"
                } else {
                    "Notes"
                },
                self.items.len()
            )
        };
        f.render_widget(
            Paragraph::new(heading).style(style.bold()),
            Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height)),
        );
        self.rows = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1 + u16::from(popup)),
        );
        if self.items.is_empty() {
            let help = match self.kind {
                Kind::Contents => "No chapter headings in this text.",
                Kind::Notes => "Press N on a passage to write a note.",
                Kind::Search if self.query.is_empty() => "Type a word or phrase.",
                Kind::Search => "No matches.",
            };
            f.render_widget(Paragraph::new(help).style(style), self.rows);
        } else {
            if self.kind == Kind::Contents {
                let last_start = self
                    .visible
                    .len()
                    .saturating_sub(usize::from(self.rows.height));
                *self.state.offset_mut() = self.state.offset().min(last_start);
            }
            let entries: Vec<_> = if self.kind == Kind::Contents {
                self.visible
                    .iter()
                    .map(|&i| {
                        let indent = self.indent(i);
                        let marker = if !self.group(i) {
                            " "
                        } else if self.folded.contains(&i) {
                            "▸"
                        } else {
                            "▾"
                        };
                        let label = format!(" {:indent$}{marker} {}", "", self.items[i].title);
                        let style = crate::appearance::outline_style(style, self.depths[i]);
                        ratatui::widgets::ListItem::new(label).style(if self.group(i) {
                            style.bold()
                        } else {
                            style
                        })
                    })
                    .collect()
            } else {
                self.items
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| {
                        ratatui::widgets::ListItem::new(format!(" {:>2}  {}", i + 1, entry.title))
                    })
                    .collect()
            };
            f.render_stateful_widget(
                List::new(entries)
                    .style(style)
                    .highlight_style(style.reversed()),
                self.rows,
                &mut self.state,
            );
        }
        self.back = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(1),
            inner.width.min(9),
            1.min(inner.height),
        );
        if popup {
            f.render_widget(Paragraph::new("[ Back ]").style(style), self.back);
            let hint = match self.kind {
                Kind::Search => Some("F3 next · F4 previous"),
                Kind::Contents if self.depths.iter().any(|&d| d > 0) => Some("← Fold · → Expand"),
                _ => None,
            };
            if let Some(hint) = hint
                && inner.width >= 35
                && inner.height >= 2
            {
                f.render_widget(
                    Paragraph::new(hint).right_aligned().style(style),
                    Rect::new(
                        self.back.right(),
                        self.back.y,
                        inner.width - self.back.width,
                        1,
                    ),
                );
            }
        } else {
            self.back = Rect::default();
        }
    }
    pub fn input(&self, e: Event) -> Option<Edit> {
        match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => Some(Edit::Back),
                KeyCode::Enter => self.state.selected().map(Edit::Choose),
                KeyCode::Left | KeyCode::Right if self.kind == Kind::Contents => self
                    .state
                    .selected()
                    .map(|row| Edit::Fold(row, k.code == KeyCode::Left)),
                KeyCode::Up => Some(Edit::Move(-1)),
                KeyCode::Down => Some(Edit::Move(1)),
                KeyCode::PageUp => Some(Edit::Move(-(self.rows.height as isize))),
                KeyCode::PageDown => Some(Edit::Move(self.rows.height as isize)),
                KeyCode::Home => Some(Edit::Move(isize::MIN)),
                KeyCode::End => Some(Edit::Move(isize::MAX)),
                KeyCode::Char('u')
                    if self.kind == Kind::Search
                        && k.modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
                    Some(Edit::Clear)
                }
                KeyCode::Backspace if self.kind == Kind::Search => Some(Edit::Erase),
                KeyCode::Char(c) if self.kind == Kind::Search && !c.is_control() => {
                    Some(Edit::Type(c))
                }
                _ => None,
            },
            Event::Mouse(m) => {
                let p = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) if self.rows.contains(p) => {
                        let row = self.state.offset() + (m.row - self.rows.y) as usize;
                        if self.kind == Kind::Contents
                            && let Some(&i) = self.visible.get(row)
                            && self.group(i)
                            && usize::from(m.column - self.rows.x) == 1 + self.indent(i)
                        {
                            Some(Edit::Fold(row, !self.folded.contains(&i)))
                        } else {
                            Some(Edit::Choose(row))
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) if self.back.contains(p) => {
                        Some(Edit::Back)
                    }
                    MouseEventKind::ScrollDown => Some(Edit::Move(1)),
                    MouseEventKind::ScrollUp => Some(Edit::Move(-1)),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    pub fn edit(&mut self, e: Edit) -> Option<Anchor> {
        match e {
            Edit::Choose(row) => return self.items[self.item_index(row)?].anchor(),
            Edit::Fold(row, close) => {
                self.fold(row, close);
                return None;
            }
            Edit::Move(d) => {
                if let Some(i) = self.state.selected() {
                    self.state.select(Some(i.saturating_add_signed(d).min(
                        if self.kind == Kind::Contents {
                            self.visible.len() - 1
                        } else {
                            self.items.len() - 1
                        },
                    )));
                }
                return None;
            }
            Edit::Type(c) if self.query.chars().count() < 256 => self.query.push(c),
            Edit::Clear => self.query.clear(),
            Edit::Erase => {
                self.query.pop();
            }
            _ => return None,
        }
        self.refresh_results();
        None
    }
    pub fn paste(&mut self, value: &str) {
        if self.kind == Kind::Search {
            let room = 256usize.saturating_sub(self.query.chars().count());
            self.query
                .extend(value.chars().filter(|c| !c.is_control()).take(room));
            self.refresh_results();
        }
    }
    fn refresh_results(&mut self) {
        if let Some(index) = &self.index {
            self.hits = index.matches(&self.query);
        }
        self.show_results(0);
    }
    pub(super) fn show_results(&mut self, selected: usize) {
        if let Some(index) = &self.index {
            self.items = self
                .hits
                .iter()
                .map(|hit| ContentsEntry::passage(hit.anchor, index.excerpt(hit)))
                .collect();
        }
        self.state =
            ListState::default().with_selected((!self.items.is_empty()).then_some(selected));
    }
}

fn current(items: &[ContentsEntry], place: Anchor) -> usize {
    items
        .iter()
        .enumerate()
        .filter_map(|(i, e)| Some((i, e.anchor()?)))
        .filter(|(_, a)| *a <= place)
        .max_by_key(|(_, a)| *a)
        .map_or(0, |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn expanded_contents_reveals_earlier_chapters_without_changing_selection() {
        let anchor = |block| Anchor { block, byte: 0 };
        let doc = Document {
            contents: (0..12)
                .map(|i| ContentsEntry::passage(anchor(i), format!("Heading {i}")))
                .collect(),
            ..Document::default()
        };
        let mut nav = Navigation::new(Kind::Contents, &doc, anchor(9));
        let mut term = Terminal::new(TestBackend::new(40, 24)).unwrap();
        let mut draw = |nav: &mut Navigation, height| {
            term.draw(|f| nav.draw_at(f, Style::default(), Rect::new(0, 0, 40, height), false))
                .unwrap();
        };
        draw(&mut nav, 8);
        assert!(nav.state.offset() > 0);
        nav.edit(Edit::Move(1));
        draw(&mut nav, 15);
        assert_eq!(nav.rows.height, 12);
        assert_eq!((nav.state.offset(), nav.state.selected()), (0, Some(10)));
        nav.follow(anchor(9));
        draw(&mut nav, 15);
        assert_eq!((nav.state.offset(), nav.state.selected()), (0, Some(9)));
    }
}
