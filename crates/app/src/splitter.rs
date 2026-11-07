//! A terminal-cell splitter shared by the library sidebar and both contents panes.
use crate::settings::Settings;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
    widgets::Paragraph,
};
use std::io;

#[derive(Clone, Copy)]
pub enum Pane {
    Library,
    Contents,
}

struct Drag {
    from_column: u16,
    from_width: u16,
    width: u16,
}

pub struct Splitter {
    pane: Pane,
    pub preferred: Option<u16>,
    bounds: Rect,
    left: Rect,
    drag: Option<Drag>,
}

impl Splitter {
    pub fn new(pane: Pane, preferred: Option<u16>) -> Self {
        Self {
            pane,
            preferred,
            bounds: Rect::default(),
            left: Rect::default(),
            drag: None,
        }
    }

    fn max_width(&self) -> u16 {
        let main = match self.pane {
            Pane::Library => 60,
            Pane::Contents => 40,
        };
        self.bounds.width.saturating_sub(main + 1).min(100)
    }

    /// The spare cell between the panels also belongs to the drag handle.
    pub fn layout(&mut self, area: Rect, visible: bool) -> (Rect, Rect) {
        if area != self.bounds || !visible {
            self.drag = None;
        }
        self.bounds = area;
        let max = self.max_width();
        if !visible || area.is_empty() || max < 12 {
            self.left = Rect::default();
            return (self.left, area);
        }
        let auto_min = match self.pane {
            Pane::Library => 25,
            Pane::Contents => 22,
        };
        let auto = (area.width / 5).clamp(auto_min, 32);
        let width = self
            .drag
            .as_ref()
            .map(|d| d.width)
            .or(self.preferred)
            .unwrap_or(auto)
            .clamp(12, max);
        self.left = Rect { width, ..area };
        let main = Rect::new(
            self.left.right() + 1,
            area.y,
            area.width - width - 1,
            area.height,
        );
        (self.left, main)
    }

    pub fn draw_grip(&self, frame: &mut Frame) {
        if !self.left.is_empty() {
            let area = Rect::new(self.left.right(), self.left.y + self.left.height / 2, 1, 1);
            let grip = Paragraph::new("↔");
            frame.render_widget(
                if self.drag.is_some() {
                    grip.bold()
                } else {
                    grip.dim()
                },
                area,
            );
        }
    }

    /// Consume both ends of a grip gesture. Only its release can update config.toml.
    pub fn input(&mut self, event: &Event) -> Option<io::Result<()>> {
        let Event::Mouse(m) = event else {
            if self.drag.is_some()
                && matches!(event, Event::Key(k) if k.kind != KeyEventKind::Release)
            {
                self.drag = None;
                if matches!(event, Event::Key(k) if k.code == KeyCode::Esc) {
                    return Some(Ok(()));
                }
            }
            return None;
        };
        if m.kind == MouseEventKind::Down(MouseButton::Left) {
            self.drag = None;
            let handle = Rect::new(
                self.left.right().saturating_sub(1),
                self.left.y,
                2,
                self.left.height,
            );
            if self.left.is_empty() || !handle.contains(Position::new(m.column, m.row)) {
                return None;
            }
            self.drag = Some(Drag {
                from_column: m.column,
                from_width: self.left.width,
                width: self.left.width,
            });
        } else if matches!(
            m.kind,
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
        ) {
            let max = self.max_width();
            let drag = self.drag.as_mut()?;
            let delta = i32::from(m.column) - i32::from(drag.from_column);
            drag.width = (i32::from(drag.from_width) + delta).clamp(12, i32::from(max)) as u16;
            if m.kind == MouseEventKind::Up(MouseButton::Left) {
                let width = drag.width;
                let changed = width != drag.from_width;
                self.drag = None;
                if changed {
                    let saved = Settings::update(|settings| match self.pane {
                        Pane::Library => settings.sidebar_width = Some(width),
                        Pane::Contents => settings.contents_width = Some(width),
                    });
                    if let Err(error) = saved {
                        return Some(Err(error));
                    }
                    self.preferred = Some(width);
                }
            }
        } else {
            return None;
        }
        Some(Ok(()))
    }
}
