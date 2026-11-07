use crate::centered;
use botui_core::{Anchor, Document};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    widgets::{Block, Clear, List, ListState, Paragraph},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Contents,
    Search,
    Marks,
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
    kind: Kind,
    query: String,
    items: Vec<(Anchor, String)>,
    state: ListState,
    rows: Rect,
    back: Rect,
}
impl Navigation {
    pub fn new(kind: Kind, doc: &Document, place: Anchor, marks: &[Anchor]) -> Self {
        let items: Vec<_> = match kind {
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
            Kind::Search => vec![],
        };
        let i = items
            .partition_point(|(a, _)| *a <= place)
            .saturating_sub(1);
        let state = ListState::default().with_selected((!items.is_empty()).then_some(i));
        Self {
            kind,
            items,
            state,
            rows: Rect::default(),
            back: Rect::default(),
            query: String::new(),
        }
    }
    pub fn draw(&mut self, f: &mut Frame, style: Style) {
        let area = centered(f.area(), 72, 22);
        let title = match self.kind {
            Kind::Contents => " Оглавление ",
            Kind::Marks => " Закладки ",
            Kind::Search => " Поиск по книге ",
        };
        let panel = Block::bordered().title(title).style(style);
        let inner = panel.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(panel, area);
        let heading = if self.kind == Kind::Search {
            format!(" Найти: {}▏", self.query)
        } else {
            format!(
                " {}: {}",
                if self.kind == Kind::Contents {
                    "Разделов"
                } else {
                    "Закладок"
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
            inner.height.saturating_sub(2),
        );
        if self.items.is_empty() {
            let help = match self.kind {
                Kind::Contents => "В тексте нет заголовков.",
                Kind::Marks => "B на странице — поставить закладку.",
                Kind::Search if self.query.is_empty() => "Введите слово или фразу.",
                Kind::Search => "Совпадений нет.",
            };
            f.render_widget(Paragraph::new(help).style(style), self.rows);
        } else {
            let entries = self
                .items
                .iter()
                .enumerate()
                .map(|(i, (_, label))| format!(" {:>2}  {}", i + 1, label));
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
        f.render_widget(Paragraph::new("[ Назад ]").style(style), self.back);
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
    pub fn edit(&mut self, doc: &Document, e: Edit) -> Option<Anchor> {
        match e {
            Edit::Choose(i) => return self.items.get(i).map(|v| v.0),
            Edit::Move(d) => {
                if let Some(i) = self.state.selected() {
                    self.state
                        .select(Some(i.saturating_add_signed(d).min(self.items.len() - 1)));
                }
                return None;
            }
            Edit::Type(c) if self.query.len() < 256 => self.query.push(c),
            Edit::Erase => {
                self.query.pop();
            }
            _ => return None,
        }
        self.items.clear();
        if !self.query.is_empty() {
            let q = self.query.to_lowercase();
            for (i, b) in doc.blocks.iter().enumerate() {
                let lower = b.text.to_lowercase();
                if let Some(pos) = lower.find(&q) {
                    // Unicode lowercasing may change UTF-8 lengths (İ etc).
                    let byte = b
                        .text
                        .char_indices()
                        .scan(0, |offset, (idx, c)| {
                            let start = *offset;
                            *offset += c.to_lowercase().map(char::len_utf8).sum::<usize>();
                            Some((start, idx))
                        })
                        .take_while(|(s, _)| *s <= pos)
                        .last()
                        .map_or(0, |(_, i)| i);
                    self.items.push((
                        Anchor { block: i, byte },
                        b.text[byte..].chars().take(120).collect(),
                    ));
                }
            }
        }
        self.state = ListState::default().with_selected((!self.items.is_empty()).then_some(0));
        None
    }
}
