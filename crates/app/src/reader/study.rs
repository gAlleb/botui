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
    ContextMenu(Position),
    Copy,
    Highlight,
    Comment,
    List,
    Peek,
    Lookup,
    Translate,
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
    keys: crate::vim::Keys,
    previous_selection: Option<TextRange>,
    search_forward: bool,
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
        self.keys.reset();
        changed
    }
    pub fn escape(&mut self) {
        if self.keys.pending() {
            self.keys.reset();
        } else if self.selection.is_some() || self.visual.is_some() {
            self.copied();
        } else {
            self.clear();
        }
    }
    pub fn toggle_normal(&mut self, start: Anchor) {
        self.normal = !self.normal;
        self.cursor = self.normal.then_some(start);
        self.selection = None;
        self.visual = None;
        self.preferred_column = None;
        self.keys.reset();
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
        if let Some(range) = self.selection
            && let Some(note) = self.notes.iter().find(|n| n.range() == Some(range))
        {
            return Some(note);
        }
        self.notes.iter().find(|n| {
            if let Some(range) = self.selection {
                n.range()
                    .is_some_and(|r| r.start <= range.start && range.end <= r.end)
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
        let action = |cmd| Some(Action::Study(cmd));
        match event {
            Event::Mouse(m) => {
                let p = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::Down(MouseButton::Right) => {
                        let hit = self.hit(p, false)?;
                        if !self.selection.is_some_and(|r| r.contains(hit.start)) {
                            self.selection = doc.word_at(hit.start);
                            self.cursor = Some(hit.start);
                            self.visual = None;
                        }
                        self.keys.reset();
                        action(Command::ContextMenu(p))
                    }
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
                        if !self.dragged && self.active_note().is_some() {
                            self.link = None;
                            return action(Command::Peek);
                        }
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
                if self.normal {
                    if k.code == KeyCode::Esc {
                        self.escape();
                        return Some(Action::Resize);
                    }
                    if k.code == KeyCode::Tab {
                        return Some(Action::JumpForward);
                    }
                    if k.modifiers.contains(KeyModifiers::CONTROL) {
                        return match k.code {
                            KeyCode::Char('o') => Some(Action::JumpBack),
                            KeyCode::Char('i') | KeyCode::Tab => Some(Action::JumpForward),
                            KeyCode::Char('c') => action(Command::Copy),
                            _ => None,
                        };
                    }
                    if k.code == KeyCode::Char('y') && self.selection.is_some() {
                        return action(Command::Copy);
                    }
                    if let Some(edit) = self.keys.input(k.code, self.visual.is_some()) {
                        return self.vim(edit, doc, pager);
                    }
                    // Unbound letters must not execute conflicting legacy reader commands.
                    return match k.code {
                        KeyCode::F(1..=5) | KeyCode::PageUp | KeyCode::PageDown => None,
                        KeyCode::Char('K') => action(Command::Lookup),
                        _ => Some(Action::Resize),
                    };
                }
                match k.code {
                    KeyCode::Esc => self.clear().then_some(Action::Resize),
                    KeyCode::Char('y' | 'Y') if self.selection.is_some() => action(Command::Copy),
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        action(Command::Copy)
                    }
                    KeyCode::Char('H') => action(Command::Highlight),
                    KeyCode::Char('K') => action(Command::Lookup),
                    KeyCode::Enter if self.active_note().is_some() => action(Command::Peek),
                    KeyCode::F(8) => {
                        self.show_notes = !self.show_notes;
                        Some(Action::Resize)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
    pub fn copied(&mut self) {
        if self.normal {
            self.previous_selection = self.selection.take().or(self.previous_selection);
            self.visual = None;
        }
        self.keys.reset();
    }
    pub fn finish_annotation(&mut self) {
        self.previous_selection = self.selection.take().or(self.previous_selection);
        self.visual = None;
        self.keys.reset();
    }
    fn vim(
        &mut self,
        edit: crate::vim::Edit,
        doc: &Document,
        pager: &mut Pager,
    ) -> Option<super::Action> {
        use super::{Action, navigation::Kind};
        use crate::vim::{Edit as V, Motion as M};
        let at = self.cursor.unwrap_or(pager.anchor());
        match edit {
            V::Wait => (),
            V::Cancel => {
                self.previous_selection = self.selection.take().or(self.previous_selection);
                self.visual = None;
            }
            V::Mark(c) => return Some(Action::Study(Command::Mark(c))),
            V::Jump(c) => return Some(Action::Study(Command::Jump(c))),
            V::Visual => {
                if self.visual.take().is_some() {
                    self.selection = None;
                } else {
                    self.visual = doc.character_at(at);
                    self.selection = self.visual;
                }
            }
            V::VisualLine => {
                self.visual = doc.paragraph_at(at);
                self.selection = self.visual;
            }
            V::Reselect => {
                self.selection = self.previous_selection;
                self.visual = self.selection.and_then(|r| doc.character_at(r.start));
                if let Some(r) = self.selection {
                    self.cursor = Some(doc.move_text(r.end, false, false));
                }
            }
            V::Search(forward) => {
                self.search_forward = forward;
                return Some(Action::Navigate(Kind::Search));
            }
            V::Next(forward) => {
                return Some(Action::FindAgain(if forward {
                    self.search_forward
                } else {
                    !self.search_forward
                }));
            }
            V::Move(m, count) | V::Yank(m, count) => {
                let yank = matches!(edit, V::Yank(..));
                if !matches!(m, M::Down | M::Up | M::ScreenDown | M::ScreenUp) {
                    self.preferred_column = None;
                }
                let mut to = at;
                let mut object = None;
                for _ in 0..count {
                    to = match m {
                        M::Left => doc.move_text(to, false, false),
                        M::Right => doc.move_text(to, true, false),
                        M::Word => {
                            let next = doc.move_text(to, true, true);
                            if yank && next == to {
                                doc.paragraph_at(to).map_or(to, |r| r.end)
                            } else {
                                next
                            }
                        }
                        M::BackWord => doc.move_text(to, false, true),
                        M::EndWord => doc.word_end(to),
                        M::Start => botui_core::Anchor { byte: 0, ..to },
                        M::First => botui_core::Anchor {
                            byte: doc.blocks[to.block].text.len()
                                - doc.blocks[to.block].text.trim_start().len(),
                            ..to
                        },
                        M::End => doc
                            .paragraph_at(to)
                            .map_or(to, |r| doc.move_text(r.end, false, false)),
                        M::NextParagraph | M::PreviousParagraph => {
                            let block = if m == M::NextParagraph {
                                ((to.block + 1)..doc.blocks.len())
                                    .find(|&i| !doc.blocks[i].text.is_empty())
                            } else {
                                (0..to.block)
                                    .rev()
                                    .find(|&i| !doc.blocks[i].text.is_empty())
                            };
                            block.map_or(to, |block| Anchor { block, byte: 0 })
                        }
                        M::Down | M::Up | M::ScreenDown | M::ScreenUp => {
                            let (next, col) = pager.move_text_line(
                                doc,
                                to,
                                matches!(m, M::Down | M::ScreenDown),
                                self.preferred_column,
                            );
                            self.preferred_column = Some(col);
                            next
                        }
                        M::ScreenStart | M::ScreenEnd => {
                            pager.text_line_range(doc, to).map_or(to, |r| {
                                if m == M::ScreenStart {
                                    r.start
                                } else {
                                    doc.move_text(r.end, false, false)
                                }
                            })
                        }
                        M::Top => doc
                            .blocks
                            .iter()
                            .position(|b| !b.text.is_empty())
                            .map_or(at, |block| botui_core::Anchor { block, byte: 0 }),
                        M::Bottom => doc
                            .blocks
                            .iter()
                            .rposition(|b| !b.text.is_empty())
                            .map_or(at, |block| botui_core::Anchor { block, byte: 0 }),
                        M::InnerWord | M::AroundWord => {
                            if let Some(mut r) = doc.word_at(to) {
                                if m == M::AroundWord {
                                    let text = &doc.blocks[to.block].text;
                                    let tail = &text[r.end.byte..];
                                    let spaces = tail.len() - tail.trim_start().len();
                                    if spaces > 0 {
                                        r.end.byte += spaces;
                                    } else {
                                        let head = &text[..r.start.byte];
                                        r.start.byte = head.trim_end().len();
                                    }
                                }
                                object = Some(object.map_or(r, |o: TextRange| o.union(r)));
                                to = doc.move_text(r.end, false, false);
                                doc.move_text(to, true, true)
                            } else {
                                to
                            }
                        }
                        M::Paragraph | M::Line => {
                            if let Some(r) = doc.paragraph_at(to) {
                                object = Some(object.map_or(r, |o: TextRange| o.union(r)));
                            }
                            botui_core::Anchor {
                                block: (to.block + 1).min(doc.blocks.len().saturating_sub(1)),
                                byte: 0,
                            }
                        }
                    };
                }
                if yank {
                    let range = object.unwrap_or_else(|| {
                        let inclusive = matches!(m, M::EndWord | M::End | M::ScreenEnd);
                        let end = if inclusive {
                            doc.character_at(to).map_or(to, |r| r.end)
                        } else {
                            to
                        };
                        TextRange::new(at, end)
                    });
                    self.selection = Some(range);
                    return Some(Action::Study(Command::Copy));
                }
                if let Some(r) = object {
                    self.selection = Some(r);
                    self.visual = Some(r);
                    self.cursor = Some(r.start);
                } else {
                    self.cursor = Some(to);
                    if let Some(seed) = self.visual {
                        self.selection = doc.character_at(to).map(|r| seed.union(r));
                    }
                }
            }
        }
        Some(Action::Resize)
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
        let label = if self.keys.pending() {
            "NORMAL · command…"
        } else if self.visual.is_some() {
            "VISUAL"
        } else if self.selection.is_some() {
            "SELECT"
        } else if self.normal {
            "NORMAL"
        } else {
            "TEXT"
        };
        let actions = [
            (label, None),
            ("[Space a y Copy]", Some(Command::Copy)),
            ("[Space a h Highlight]", Some(Command::Highlight)),
            ("[Space a n Note]", Some(Command::Comment)),
            ("[Space a a Notes]", Some(Command::List)),
            ("[Space d d Dictionary]", Some(Command::Lookup)),
            (
                if self.selection.is_some() || self.keys.pending() {
                    "[Esc Clear]"
                } else {
                    "[Esc Read]"
                },
                Some(Command::Clear),
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_inside_highlight_edits_it_but_crossing_selection_does_not() {
        let mut study = Study::default();
        study.notes.push(serde_json::from_value(serde_json::json!({
            "id": "saved", "block": 0, "byte": 6, "end": [0, 19], "body": "comment", "updated": 0
        })).unwrap());
        let at = |byte| Anchor { block: 0, byte };
        study.selection = Some(TextRange::new(at(6), at(11)));
        assert_eq!(study.active_note().unwrap().id, "saved");
        study.selection = Some(TextRange::new(at(0), at(11)));
        assert!(study.active_note().is_none());
        study.selection = None;
        study.cursor = Some(at(18));
        assert!(study.active_note().is_some());
        study.cursor = Some(at(19));
        assert!(study.active_note().is_none());
    }
    #[test]
    fn saving_annotation_leaves_normal_and_gv_restores_exact_range() {
        use crate::vim::{Edit, Motion};
        let doc = Document {
            blocks: vec![botui_core::Block {
                text: "Alpha bravo charlie".into(),
                heading: false,
            }],
            ..Default::default()
        };
        let mut pager = Pager::new(&doc, 30, 10, botui_core::LayoutMode::Single);
        let mut study = Study::default();
        study.toggle_normal(Anchor { block: 0, byte: 0 });
        study.vim(Edit::Visual, &doc, &mut pager);
        study.vim(Edit::Move(Motion::EndWord, 1), &doc, &mut pager);
        let selection = study.selection;
        assert_eq!(doc.text_range(selection.unwrap()), "Alpha");
        study.finish_annotation();
        assert!(study.normal);
        study.vim(Edit::Move(Motion::Right, 1), &doc, &mut pager);
        assert!(
            study.selection.is_none(),
            "saved selection must not reappear on movement"
        );
        study.vim(Edit::Reselect, &doc, &mut pager);
        assert_eq!(study.selection, selection);
        assert_eq!(
            study.cursor.unwrap().byte,
            4,
            "gv must land inside the selection"
        );
        study.vim(Edit::Move(Motion::Left, 1), &doc, &mut pager);
        assert_eq!(doc.text_range(study.selection.unwrap()), "Alph");
    }
    fn quote(command: &str, at: Anchor) -> String {
        let doc = Document {
            blocks: vec![
                botui_core::Block {
                    text: "Alpha bravo charlie".into(),
                    heading: false,
                },
                botui_core::Block {
                    text: "Привет мир".into(),
                    heading: false,
                },
            ],
            ..Default::default()
        };
        let mut pager = Pager::new(&doc, 30, 10, botui_core::LayoutMode::Single);
        let mut study = Study::default();
        study.toggle_normal(at);
        for c in command.chars() {
            let edit = study.keys.input(KeyCode::Char(c), false).unwrap();
            study.vim(edit, &doc, &mut pager);
        }
        doc.text_range(study.selection.unwrap())
    }
    #[test]
    fn jk_follow_wrapped_rows_and_braces_follow_paragraphs() {
        use crate::vim::{Edit, Motion};
        let doc = Document {
            blocks: vec![
                botui_core::Block {
                    text:
                        "Alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima."
                            .into(),
                    heading: false,
                },
                botui_core::Block {
                    text: "Second paragraph".into(),
                    heading: false,
                },
            ],
            ..Default::default()
        };
        let mut pager = Pager::new(&doc, 24, 12, botui_core::LayoutMode::Single);
        let start = Anchor { block: 0, byte: 0 };
        let mut study = Study::default();
        study.toggle_normal(start);
        study.vim(Edit::Move(Motion::Down, 1), &doc, &mut pager);
        let second = study.cursor.unwrap();
        assert_eq!(second.block, 0);
        assert!(second.byte > 0);
        study.vim(Edit::Move(Motion::Up, 1), &doc, &mut pager);
        assert_eq!(study.cursor, Some(start));
        study.vim(Edit::Move(Motion::NextParagraph, 1), &doc, &mut pager);
        assert_eq!(study.cursor, Some(Anchor { block: 1, byte: 0 }));
        study.vim(Edit::Move(Motion::PreviousParagraph, 1), &doc, &mut pager);
        assert_eq!(study.cursor, Some(start));
    }
    #[test]
    fn yank_objects_counts_and_last_word() {
        assert_eq!(quote("yiw", Anchor { block: 0, byte: 6 }), "bravo");
        assert_eq!(quote("2yw", Anchor { block: 0, byte: 0 }), "Alpha bravo ");
        assert_eq!(
            quote("yy", Anchor { block: 0, byte: 6 }),
            "Alpha bravo charlie"
        );
        assert_eq!(quote("y$", Anchor { block: 0, byte: 6 }), "bravo charlie");
        assert_eq!(
            quote(
                "yw",
                Anchor {
                    block: 1,
                    byte: "Привет ".len()
                }
            ),
            "мир"
        );
        assert_eq!(
            quote("y2ip", Anchor { block: 0, byte: 0 }),
            "Alpha bravo charlie\n\nПривет мир"
        );
    }
}
