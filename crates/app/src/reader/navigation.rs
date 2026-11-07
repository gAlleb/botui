use crate::centered;
use botui_core::{Anchor, Document};
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
    Marks,
    Notes,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Edit {
    Back,
    Move(isize),
    Choose(usize),
    Type(char),
    Erase,
}
pub(super) struct Navigation {
    pub(super) index: Option<botui_core::SearchIndex>,
    kind: Kind,
    query: String,
    items: Vec<(Anchor, String)>,
    unavailable: Vec<String>,
    state: ListState,
    rows: Rect,
    back: Rect,
}
impl Navigation {
    pub fn new(kind: Kind, doc: &Document, place: Anchor, marks: &[Anchor]) -> Self {
        let items: Vec<_> = match kind {
            Kind::Contents if !doc.contents.is_empty() => doc.contents.clone(),
            Kind::Contents => doc
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| b.heading)
                .map(|(i, b)| (Anchor { block: i, byte: 0 }, b.text.clone()))
                .collect(),
            Kind::Marks => marks
                .iter()
                .filter_map(|&a| Some((a, doc.blocks.get(a.block)?.text.get(a.byte..)?.to_owned())))
                .collect(),
            Kind::Notes | Kind::Search => vec![],
        };
        let i = current(&items, place);
        let state = ListState::default().with_selected((!items.is_empty()).then_some(i));
        Self {
            kind,
            index: None,
            unavailable: vec![],
            items,
            state,
            rows: Rect::default(),
            back: Rect::default(),
            query: String::new(),
        }
    }
    pub fn add_notes(&mut self, notes: &[botui_store::Note]) {
        self.items = notes.iter().map(|n| (n.anchor(), n.body.clone())).collect();
        self.state.select((!self.items.is_empty()).then_some(0));
    }
    pub fn add_server_marks(&mut self, marks: Vec<(Option<Anchor>, String)>) {
        for (place, label) in marks {
            let label = label.replace(char::is_control, " ");
            if let Some(place) = place {
                self.items.push((place, label));
            } else {
                self.unavailable
                    .push(format!("{label} · unavailable in this file"));
            }
        }
        if !self.items.is_empty() || !self.unavailable.is_empty() {
            self.state.select(Some(0));
        }
    }
    pub fn draw(&mut self, f: &mut Frame, style: Style) {
        let area = centered(f.area(), 72, 22);
        self.draw_at(f, style, area, true);
    }
    pub fn follow(&mut self, place: Anchor) {
        if !self.items.is_empty() {
            self.state.select(Some(current(&self.items, place)));
        }
    }
    pub fn draw_at(&mut self, f: &mut Frame, style: Style, area: Rect, popup: bool) {
        let title = match self.kind {
            Kind::Contents => " Contents ",
            Kind::Marks => " Bookmarks ",
            Kind::Notes => " Reading notes ",
            Kind::Search => " Find in book ",
        };
        let panel = if popup {
            crate::appearance::block()
        } else {
            crate::appearance::divider()
        }
        .title(title)
        .style(style);
        let inner = panel.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(panel, area);
        let heading = if self.kind == Kind::Search {
            format!(" Find: {}▏", self.query)
        } else {
            format!(
                " {}: {}",
                if self.kind == Kind::Contents {
                    "Chapters"
                } else if self.kind == Kind::Notes {
                    "Notes"
                } else {
                    "Bookmarks"
                },
                self.items.len() + self.unavailable.len()
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
            inner.height.saturating_sub(2),
        );
        if self.items.is_empty() && self.unavailable.is_empty() {
            let help = match self.kind {
                Kind::Contents => "No chapter headings in this text.",
                Kind::Notes => "Press N on a passage to write a note.",
                Kind::Marks => "Press B on a page to bookmark it.",
                Kind::Search if self.query.is_empty() => "Type a word or phrase.",
                Kind::Search => "No matches.",
            };
            f.render_widget(Paragraph::new(help).style(style), self.rows);
        } else {
            let entries = self
                .items
                .iter()
                .enumerate()
                .map(|(i, (_, label))| format!(" {:>2}  {}", i + 1, label))
                .chain(self.unavailable.iter().map(|s| format!("  –  {s}")));
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
        } else {
            self.back = Rect::default();
        }
    }
    pub fn input(&self, e: Event) -> Option<Edit> {
        match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => Some(Edit::Back),
                KeyCode::Enter => self.state.selected().map(Edit::Choose),
                KeyCode::Up => Some(Edit::Move(-1)),
                KeyCode::Down => Some(Edit::Move(1)),
                KeyCode::PageUp => Some(Edit::Move(-(self.rows.height as isize))),
                KeyCode::PageDown => Some(Edit::Move(self.rows.height as isize)),
                KeyCode::Home => Some(Edit::Move(isize::MIN)),
                KeyCode::End => Some(Edit::Move(isize::MAX)),
                KeyCode::Backspace if self.kind == Kind::Search => Some(Edit::Erase),
                KeyCode::Char(c) if self.kind == Kind::Search && !c.is_control() => {
                    Some(Edit::Type(c))
                }
                _ => None,
            },
            Event::Mouse(m) => {
                let p = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) if self.rows.contains(p) => Some(
                        Edit::Choose(self.state.offset() + (m.row - self.rows.y) as usize),
                    ),
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
            Edit::Choose(i) => return self.items.get(i).map(|v| v.0),
            Edit::Move(d) => {
                if let Some(i) = self.state.selected() {
                    self.state.select(Some(
                        i.saturating_add_signed(d)
                            .min(self.items.len() + self.unavailable.len() - 1),
                    ));
                }
                return None;
            }
            Edit::Type(c) if self.query.chars().count() < 256 => self.query.push(c),
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
        self.items = self
            .index
            .as_ref()
            .map_or_else(Vec::new, |index| index.find(&self.query));
        self.state = ListState::default().with_selected((!self.items.is_empty()).then_some(0));
    }
}

fn current(items: &[(Anchor, String)], place: Anchor) -> usize {
    items
        .iter()
        .enumerate()
        .filter(|(_, (a, _))| *a <= place)
        .max_by_key(|(_, (a, _))| *a)
        .map_or(0, |(i, _)| i)
}
