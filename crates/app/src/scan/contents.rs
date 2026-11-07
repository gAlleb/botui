use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
    widgets::Paragraph,
};

#[derive(Default)]
pub struct Contents {
    pub items: Vec<(u32, String)>,
    selected: usize,
    first: usize,
    rows: Rect,
    focus: bool,
}
impl Contents {
    fn count(&self, total: u32) -> usize {
        if self.items.is_empty() {
            total as usize
        } else {
            self.items.len()
        }
    }
    fn page(&self, i: usize) -> u32 {
        self.items.get(i).map_or(i as u32 + 1, |v| v.0)
    }
    pub fn draw(&mut self, f: &mut Frame, area: Rect, page: u32, total: u32) {
        self.rows = Rect::default();
        if area.is_empty() {
            self.focus = false;
            return;
        }
        let border = crate::appearance::block().title(if self.items.is_empty() {
            " Pages "
        } else {
            " Contents "
        });
        self.rows = border.inner(area);
        f.render_widget(border, area);
        let count = self.count(total);
        if !self.focus {
            self.selected = if self.items.is_empty() {
                page.saturating_sub(1) as usize
            } else {
                self.items
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.0 <= page)
                    .max_by_key(|(_, v)| v.0)
                    .map_or(0, |(i, _)| i)
            };
        }
        self.selected = self.selected.min(count.saturating_sub(1));
        self.first = self
            .first
            .min(self.selected)
            .max((self.selected + 1).saturating_sub(self.rows.height as usize));
        for (y, i) in (self.first..count)
            .take(self.rows.height as usize)
            .enumerate()
        {
            let label = self.items.get(i).map_or_else(
                || format!(" Page {}", i + 1),
                |(n, title)| format!(" {n}  {title}"),
            );
            let p = Paragraph::new(label);
            f.render_widget(
                if i == self.selected { p.reversed() } else { p },
                Rect::new(self.rows.x, self.rows.y + y as u16, self.rows.width, 1),
            );
        }
    }
    /// Some(None) consumes a focus or selection movement without changing page.
    pub fn input(&mut self, e: &Event, total: u32) -> Option<Option<u32>> {
        if self.rows.is_empty() {
            return None;
        }
        let step = match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Tab => {
                    self.focus = !self.focus;
                    return Some(None);
                }
                KeyCode::Enter if self.focus => {
                    self.focus = false;
                    return Some(Some(self.page(self.selected)));
                }
                KeyCode::Down if self.focus => 1,
                KeyCode::Up if self.focus => -1,
                KeyCode::PageDown if self.focus => self.rows.height as isize,
                KeyCode::PageUp if self.focus => -(self.rows.height as isize),
                _ => return None,
            },
            Event::Mouse(m) if self.rows.contains(Position::new(m.column, m.row)) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let i = self.first + usize::from(m.row - self.rows.y);
                    self.focus = false;
                    return Some((i < self.count(total)).then(|| self.page(i)));
                }
                MouseEventKind::ScrollDown => 1,
                MouseEventKind::ScrollUp => -1,
                _ => return None,
            },
            _ => return None,
        };
        self.focus = true;
        self.selected = self
            .selected
            .saturating_add_signed(step)
            .min(self.count(total).saturating_sub(1));
        Some(None)
    }
}
