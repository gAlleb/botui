//! Selection, cursor and annotation paint; independent of pagination and persistence.
use botui_core::{Anchor, Document, PageLine, Pager, TextRange};
use botui_store::{HighlightColor, Note};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    widgets::{Clear, Paragraph},
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Copy,
    Highlight,
    Comment,
    List,
    Peek,
    Lookup,
    Export,
    Normal,
    Clear,
    Mark(char),
    Jump(char),
}
#[derive(Clone)]
struct Cell {
    rect: Rect,
    source: TextRange,
}
#[derive(Default)]
pub(super) struct Study {
    pub cursor: Option<Anchor>,
    pub selection: Option<TextRange>,
    pub notes: Vec<Note>,
    pub message: Option<String>,
    pub normal: bool,
    pub show_notes: bool,
    visual: Option<TextRange>,
    drag: Option<TextRange>,
    pointer: Option<Position>,
    preferred_column: Option<usize>,
    dragged: bool,
    link: Option<super::Action>,
    clicks: Option<(Instant, Position, u8)>,
    pending: Option<char>,
    map: Vec<Cell>,
    buttons: Vec<(Rect, Command)>,
}
impl Study {
    pub fn clear(&mut self) -> bool {
        if self.selection.take().is_some() {
            self.visual = None;
            return true;
        }
        let changed = self.cursor.take().is_some() | self.message.take().is_some();
        self.normal = false;
        self.pending = None;
        changed
    }
    pub fn toggle_normal(&mut self, start: Anchor) {
        self.normal = !self.normal;
        self.cursor = self.normal.then_some(start);
        self.selection = None;
        self.visual = None;
        self.preferred_column = None;
        self.pending = None;
        self.message = None;
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn drag_direction(&self, area: Rect) -> i8 {
        self.pointer.filter(|_| self.drag.is_some()).map_or(0, |p| {
            if p.y < area.y {
                -1
            } else if p.y >= area.bottom() {
                1
            } else {
                0
            }
        })
    }
    pub fn extend_drag(&mut self) {
        if let Some(hit) = self.pointer.and_then(|p| self.hit(p, true)) {
            self.selection = self.drag.map(|start| start.union(hit));
            self.cursor = Some(hit.start);
            self.dragged = true;
        }
    }
    pub fn mapping(&self) -> bool {
        self.cursor.is_some() || !self.notes.is_empty()
    }
    pub fn begin_frame(&mut self) {
        self.map.clear();
        self.buttons.clear();
    }
    pub fn map_line(&mut self, doc: &Document, line: &PageLine, rect: Rect) {
        let width = ratatui::text::Line::raw(&line.text).width() as u16;
        let shift = if line.heading {
            rect.width.saturating_sub(width) / 2
        } else {
            0
        };
        for cell in line.text_cells(doc) {
            let x = rect
                .x
                .saturating_add(shift)
                .saturating_add(cell.columns.start as u16);
            let r = Rect::new(x, rect.y, cell.columns.len() as u16, 1).intersection(rect);
            if !r.is_empty() {
                self.map.push(Cell {
                    rect: r,
                    source: cell.source,
                });
            }
        }
    }
    fn hit(&self, p: Position, nearest: bool) -> Option<TextRange> {
        self.map
            .iter()
            .find(|c| c.rect.contains(p))
            .or_else(|| {
                nearest
                    .then(|| {
                        self.map.iter().min_by_key(|c| {
                            u32::from(c.rect.y.abs_diff(p.y)) * 1000
                                + u32::from(c.rect.x.abs_diff(p.x))
                        })
                    })
                    .flatten()
            })
            .map(|c| c.source)
    }
    pub fn visible(&self, at: Anchor) -> bool {
        self.map.iter().any(|c| c.source.contains(at))
    }
    pub fn active_note(&self) -> Option<&Note> {
        self.notes.iter().find(|n| {
            if let Some(range) = self.selection {
                n.range() == Some(range)
            } else {
                self.cursor
                    .is_some_and(|c| n.range().is_some_and(|r| r.contains(c)) || n.anchor() == c)
            }
        })
    }
    pub fn range(&self, doc: &Document, fallback: Anchor) -> Option<TextRange> {
        self.selection
            .or_else(|| self.active_note().and_then(Note::range))
            .or_else(|| doc.paragraph_at(self.cursor.unwrap_or(fallback)))
    }
    pub fn input(
        &mut self,
        event: &Event,
        doc: &Document,
        pager: &mut Pager,
        link: Option<super::Action>,
    ) -> Option<super::Action> {
        use super::Action;
        let fallback = pager.anchor();
        let action = |cmd| Some(Action::Study(cmd));
        match event {
            Event::Mouse(m) => {
                let p = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some((_, command)) = self.buttons.iter().find(|(r, _)| r.contains(p))
                        {
                            return action(*command);
                        }
                        let hit = self.hit(p, false)?;
                        self.normal = false;
                        self.preferred_column = None;
                        self.pointer = None;
                        self.visual = None;
                        self.message = None;
                        let clicks = self
                            .clicks
                            .filter(|(at, pos, _)| {
                                *pos == p && at.elapsed() < Duration::from_millis(450)
                            })
                            .map_or(1, |(_, _, n)| n % 3 + 1);
                        self.clicks = Some((Instant::now(), p, clicks));
                        self.cursor = Some(hit.start);
                        let range = match clicks {
                            2 => doc.word_at(hit.start),
                            3 => doc.paragraph_at(hit.start),
                            _ => None,
                        };
                        self.selection = range;
                        self.drag = Some(range.unwrap_or(hit));
                        self.dragged = clicks > 1;
                        self.link = link;
                        Some(Action::Resize)
                    }
                    MouseEventKind::Drag(MouseButton::Left) if self.drag.is_some() => {
                        self.pointer = Some(p);
                        self.extend_drag();
                        Some(Action::Resize)
                    }
                    MouseEventKind::Up(MouseButton::Left) if self.drag.take().is_some() => {
                        self.pointer = None;
                        if !self.dragged
                            && let Some(link) = self.link.take()
                        {
                            self.cursor = None;
                            return Some(link);
                        }
                        self.link = None;
                        Some(Action::Resize)
                    }
                    _ => None,
                }
            }
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                if let Some(prefix) = self.pending.take() {
                    return match k.code {
                        KeyCode::Char(c) if c.is_ascii_alphanumeric() => action(if prefix == 'm' {
                            Command::Mark(c)
                        } else {
                            Command::Jump(c)
                        }),
                        _ => Some(Action::Resize),
                    };
                }
                if k.code == KeyCode::Esc {
                    return self.clear().then_some(Action::Resize);
                }
                if k.code == KeyCode::F(7) {
                    return action(Command::Normal);
                }
                if k.code == KeyCode::Char('v') && k.modifiers.is_empty() {
                    self.cursor.get_or_insert(fallback);
                    self.normal = true;
                    if self.visual.take().is_some() {
                        self.selection = None
                    } else {
                        self.visual = doc.character_at(self.cursor.unwrap());
                        self.selection = self.visual;
                    }
                    return Some(Action::Resize);
                }
                if k.code == KeyCode::Char('V') {
                    self.cursor.get_or_insert(fallback);
                    self.normal = true;
                    self.selection = doc.paragraph_at(self.cursor.unwrap());
                    self.visual = self.selection;
                    return Some(Action::Resize);
                }
                if self.selection.is_some()
                    && (matches!(k.code, KeyCode::Char('y' | 'Y'))
                        || k.code == KeyCode::Char('c')
                            && k.modifiers.contains(KeyModifiers::CONTROL))
                {
                    return action(Command::Copy);
                }
                match k.code {
                    KeyCode::Char('H') => return action(Command::Highlight),
                    KeyCode::Char('K') => return action(Command::Lookup),
                    KeyCode::F(8) => {
                        self.show_notes = !self.show_notes;
                        return Some(Action::Resize);
                    }
                    KeyCode::Enter if self.active_note().is_some() => return action(Command::Peek),
                    _ => (),
                }
                if !self.normal {
                    return None;
                }
                let at = self.cursor.unwrap_or(fallback);
                if !matches!(
                    k.code,
                    KeyCode::Char('j' | 'k') | KeyCode::Up | KeyCode::Down
                ) {
                    self.preferred_column = None;
                }
                let to = match k.code {
                    KeyCode::Char('m' | '\'') => {
                        self.pending = Some(if k.code == KeyCode::Char('m') {
                            'm'
                        } else {
                            '\''
                        });
                        return Some(Action::Resize);
                    }
                    KeyCode::Char('h') | KeyCode::Left => doc.move_text(at, false, false),
                    KeyCode::Char('l') | KeyCode::Right => doc.move_text(at, true, false),
                    KeyCode::Char('w') => doc.move_text(at, true, true),
                    KeyCode::Char('b') => doc.move_text(at, false, true),
                    KeyCode::Char('e') => doc.word_end(at),
                    KeyCode::Char('0') | KeyCode::Home => Anchor { byte: 0, ..at },
                    KeyCode::Char('$') | KeyCode::End => doc
                        .paragraph_at(at)
                        .map_or(at, |r| doc.move_text(r.end, false, false)),
                    KeyCode::Char('j' | 'k') | KeyCode::Up | KeyCode::Down => {
                        let down = matches!(k.code, KeyCode::Char('j') | KeyCode::Down);
                        let (to, column) =
                            pager.move_text_line(doc, at, down, self.preferred_column);
                        self.preferred_column = Some(column);
                        to
                    }
                    KeyCode::Char('y' | 'Y') => return action(Command::Copy),
                    _ => return None,
                };
                self.cursor = Some(to);
                if let Some(seed) = self.visual {
                    self.selection = doc.character_at(to).map(|c| seed.union(c));
                }
                Some(Action::Resize)
            }
            _ => None,
        }
    }
    pub fn paint(&self, f: &mut Frame) {
        // Restrict the per-cell work to annotations on these visible pages.
        let extent = self.map.iter().map(|c| c.source).reduce(TextRange::union);
        let notes: Vec<_> = self
            .notes
            .iter()
            .filter(|n| extent.is_some_and(|v| n.range().is_some_and(|r| r.intersects(v))))
            .collect();
        for cell in &self.map {
            let note = notes
                .iter()
                .rev()
                .find(|n| n.range().is_some_and(|r| r.intersects(cell.source)));
            let selected = self.selection.is_some_and(|r| r.intersects(cell.source));
            let cursor = self.cursor.is_some_and(|a| cell.source.contains(a));
            let mut style = Style::default();
            if let Some(note) = note {
                if let Some(color) = note.color {
                    style = highlight(color);
                }
                if !note.body.is_empty() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
            }
            if selected {
                style = Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(203, 214, 247))
                    .remove_modifier(Modifier::REVERSED);
            }
            if cursor {
                style = style.add_modifier(Modifier::REVERSED | Modifier::UNDERLINED);
            }
            if note.is_some() || selected || cursor {
                for x in cell.rect.x..cell.rect.right() {
                    f.buffer_mut()[(x, cell.rect.y)].set_style(style);
                }
            }
        }
    }
    pub fn controls(&mut self, f: &mut Frame, style: Style) {
        if self.cursor.is_none() && self.message.is_none() {
            return;
        }
        let area = f.area();
        let label = if self.pending == Some('m') {
            "Mark: type a letter"
        } else if self.pending.is_some() {
            "Jump: type a letter"
        } else if self.selection.is_some() {
            "SELECT"
        } else if self.normal {
            "NORMAL"
        } else {
            "TEXT"
        };
        let actions = [
            (label, None),
            ("[Y Copy]", Some(Command::Copy)),
            ("[H Highlight]", Some(Command::Highlight)),
            ("[N Note]", Some(Command::Comment)),
            ("[A Notes]", Some(Command::List)),
            ("[K Lookup]", Some(Command::Lookup)),
            ("[Esc Clear]", Some(Command::Clear)),
        ];
        let (rects, height) = super::controls::layout(area, &actions.map(|a| a.0));
        let foot = Rect::new(area.x, area.bottom() - height, area.width, height);
        f.render_widget(Clear, foot);
        for ((text, command), rect) in actions.into_iter().zip(rects) {
            f.render_widget(
                Paragraph::new(text).style(style.add_modifier(Modifier::BOLD)),
                rect,
            );
            if let Some(cmd) = command {
                self.buttons.push((rect, cmd));
            }
        }
        let body = self.message.as_deref().or_else(|| {
            self.show_notes
                .then(|| self.active_note())
                .flatten()
                .map(|n| n.body.as_str())
        });
        if let Some(body) = body.filter(|s| !s.is_empty())
            && foot.y > area.y
        {
            let row = Rect::new(area.x, foot.y - 1, area.width, 1);
            f.render_widget(Clear, row);
            f.render_widget(
                Paragraph::new(body.replace('\n', " · "))
                    .style(style.add_modifier(Modifier::ITALIC)),
                row,
            );
        }
    }
}
fn highlight(color: HighlightColor) -> Style {
    let bg = match color {
        HighlightColor::Yellow => Color::Rgb(246, 213, 117),
        HighlightColor::Green => Color::Rgb(164, 206, 133),
        HighlightColor::Blue => Color::Rgb(135, 190, 227),
        HighlightColor::Pink => Color::Rgb(238, 162, 180),
        HighlightColor::Purple => Color::Rgb(198, 167, 230),
    };
    Style::default()
        .fg(Color::Rgb(30, 30, 40))
        .bg(bg)
        .remove_modifier(Modifier::REVERSED | Modifier::DIM)
}
