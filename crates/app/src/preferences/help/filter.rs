//! Live search in the shortcut guide. Keep section headers for the matching rows.
use super::Entry;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_truncate::UnicodeTruncateStr;

#[derive(Default)]
pub(super) struct Filter {
    query: String,
    editing: bool,
    hit: Rect,
}
impl Filter {
    pub fn active(&self) -> bool {
        self.editing || !self.query.is_empty()
    }
    pub fn finish(&mut self) {
        self.editing = false;
    }
    pub fn clear(&mut self) {
        self.query.clear();
        self.editing = false;
    }
    pub fn click(&mut self, pos: Position) -> bool {
        if !self.hit.contains(pos) {
            return false;
        }
        self.editing = true;
        true
    }
    pub fn input(&mut self, event: &Event) -> bool {
        if let Event::Paste(s) = event
            && self.editing
        {
            self.query.extend(
                s.chars()
                    .filter(|c| !c.is_control())
                    .take(128usize.saturating_sub(self.query.chars().count())),
            );
            return true;
        }
        let Event::Key(k) = event else { return false };
        if k.kind == KeyEventKind::Release {
            return false;
        }
        match k.code {
            KeyCode::Char('/') if !self.editing => self.editing = true,
            KeyCode::Esc if self.active() => self.clear(),
            KeyCode::Enter if self.editing => self.editing = false,
            KeyCode::Backspace if self.editing => {
                self.query.pop();
            }
            KeyCode::Char('u') if self.editing && k.modifiers == KeyModifiers::CONTROL => {
                self.query.clear()
            }
            KeyCode::Char(c)
                if self.editing
                    && !c.is_control()
                    && (k.modifiers - KeyModifiers::SHIFT).is_empty() =>
            {
                if self.query.chars().count() < 128 {
                    self.query.push(c);
                }
            }
            _ => return false,
        }
        true
    }

    pub fn matching<'a>(&self, entries: &'a [Entry]) -> Vec<&'a Entry> {
        let q = self.query.trim().to_lowercase();
        if q.is_empty() {
            return entries.iter().collect();
        }
        let mut found = vec![];
        let mut header = None;
        let mut section_match = false;
        for e in entries {
            match e {
                Entry::Section(s) => {
                    header = Some(e);
                    section_match = s.to_lowercase().contains(&q);
                }
                Entry::Key(k, text)
                    if section_match
                        || k.to_lowercase().contains(&q)
                        || text.to_lowercase().contains(&q) =>
                {
                    found.extend(header.take());
                    found.push(e);
                }
                _ => (),
            }
        }
        found
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, matches: usize) {
        self.hit = area;
        if area.is_empty() {
            return;
        }
        let count = if self.query.is_empty() || area.width < 32 {
            String::new()
        } else if matches == 1 {
            "1 match".into()
        } else {
            format!("{matches} matches")
        };
        let room = usize::from(area.width).saturating_sub(6 + count.len());
        let label = if self.query.is_empty() && !self.editing {
            "Find a key or action"
        } else {
            self.query.as_str()
        };
        let (visible, _) = label.unicode_truncate_start(room);
        let text = Line::from(vec![
            Span::raw(" / ").cyan().bold(),
            Span::raw(" "),
            Span::raw(visible),
            Span::raw(if self.editing { "▏" } else { "" }).cyan(),
        ]);
        f.render_widget(Paragraph::new(text), area);
        if !count.is_empty() {
            f.render_widget(
                Paragraph::new(count.as_str()).dim(),
                Rect::new(
                    area.right() - count.len() as u16,
                    area.y,
                    count.len() as u16,
                    1,
                ),
            );
        }
    }
}
