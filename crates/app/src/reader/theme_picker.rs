use crate::{centered, theme::Theme};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Edit {
    Row(usize),
    Dark(bool),
    Accept(Theme),
    Cancel,
}

pub(super) struct Picker {
    row: usize,
    dark: bool,
    offset: usize,
    popup: Rect,
    rows: Rect,
    footer: Rect,
    swatch_width: u16,
}

impl Picker {
    pub(super) fn new(theme: Theme) -> Self {
        Self {
            row: theme.row(),
            dark: theme.dark(),
            offset: 0,
            popup: Rect::default(),
            rows: Rect::default(),
            footer: Rect::default(),
            swatch_width: 0,
        }
    }

    pub(super) fn preview(&self) -> Theme {
        Theme::at(self.row, self.dark)
    }

    pub(super) fn edit(&mut self, edit: Edit) {
        match edit {
            Edit::Row(row) => self.row = row,
            Edit::Dark(dark) => self.dark = dark,
            Edit::Cancel | Edit::Accept(_) => {}
        }
    }

    pub(super) fn draw(&mut self, f: &mut Frame) {
        let screen = f.area();
        let popup = centered(screen, 46, Theme::COUNT as u16 + 4);
        self.popup = popup;
        let style = self.preview().style();
        f.render_widget(Clear, popup);
        let block = if popup.height >= 5 && popup.width >= 16 {
            crate::appearance::block().title(" Reader theme ")
        } else {
            Block::new()
        }
        .style(style);
        let body = block.inner(popup);
        f.render_widget(block, popup);
        let header = u16::from(body.height >= 3);
        let footer = u16::from(body.height >= 2);
        self.rows = Rect::new(
            body.x,
            body.y + header,
            body.width,
            body.height.saturating_sub(header + footer),
        );
        self.footer = Rect::new(body.x, self.rows.bottom(), body.width, footer);
        self.swatch_width = (body.width / 3).min(9);
        let light_x = body.right() - self.swatch_width * 2;
        let dark_x = light_x + self.swatch_width;
        if header > 0 {
            f.render_widget(
                Paragraph::new("↑↓ Palette"),
                Rect::new(body.x, body.y, light_x - body.x, 1),
            );
            let labels = if self.swatch_width >= 7 {
                ["Light", "Dark"]
            } else {
                ["L", "D"]
            };
            for (x, label) in [light_x, dark_x].into_iter().zip(labels) {
                f.render_widget(
                    Paragraph::new(label).centered(),
                    Rect::new(x, body.y, self.swatch_width, 1),
                );
            }
        }
        let height = usize::from(self.rows.height);
        self.offset = self
            .offset
            .min(self.row)
            .max((self.row + 1).saturating_sub(height))
            .min(Theme::COUNT.saturating_sub(height));
        for (y, row) in (self.offset..Theme::COUNT).take(height).enumerate() {
            let y = self.rows.y + y as u16;
            let current = self.row == row;
            let light = Theme::at(row, false);
            let row_style = if current {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            };
            let name = if row == 0 {
                body.width
            } else {
                light_x - body.x
            };
            f.render_widget(
                Paragraph::new(light.palette_name()).style(row_style),
                Rect::new(body.x, y, name, 1),
            );
            if row == 0 {
                continue;
            }
            for (dark, x) in [(false, light_x), (true, dark_x)] {
                let theme = Theme::at(row, dark);
                let cursor = if current && self.dark == dark {
                    "›"
                } else {
                    " "
                };
                let sample = if self.swatch_width >= 4 {
                    Line::from(vec![
                        Span::styled(cursor, theme.style().fg(theme.link())),
                        Span::raw("Aa"),
                    ])
                } else {
                    Line::raw(if current && self.dark == dark {
                        "›"
                    } else {
                        "A"
                    })
                };
                f.render_widget(
                    Paragraph::new(sample).style(theme.style()).centered(),
                    Rect::new(x, y, self.swatch_width, 1),
                );
            }
        }
        if footer > 0 {
            let half = self.footer.width / 2;
            let labels = if body.width >= 31 {
                ["✓ Apply (Enter)", "× Cancel (Esc)"]
            } else if body.width >= 17 {
                ["✓ Enter", "× Esc"]
            } else {
                ["✓", "×"]
            };
            f.render_widget(
                Paragraph::new(labels[0]).centered(),
                Rect::new(body.x, self.footer.y, half, 1),
            );
            f.render_widget(
                Paragraph::new(labels[1]).centered(),
                Rect::new(body.x + half, self.footer.y, body.width - half, 1),
            );
        }
    }

    pub(super) fn input(&self, event: Event) -> Option<Edit> {
        let shift = |delta| Edit::Row(self.row.saturating_add_signed(delta).min(Theme::COUNT - 1));
        match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q' | 'Q' | 't' | 'T') => Some(Edit::Cancel),
                KeyCode::Enter | KeyCode::Char(' ') => Some(Edit::Accept(self.preview())),
                KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => Some(shift(-1)),
                KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => Some(shift(1)),
                KeyCode::PageUp => Some(shift(
                    -isize::try_from(self.rows.height.max(1)).unwrap_or(1),
                )),
                KeyCode::PageDown => {
                    Some(shift(isize::try_from(self.rows.height.max(1)).unwrap_or(1)))
                }
                KeyCode::Home => Some(Edit::Row(0)),
                KeyCode::End => Some(Edit::Row(Theme::COUNT - 1)),
                KeyCode::Left | KeyCode::Char('h') => Some(Edit::Dark(false)),
                KeyCode::Right | KeyCode::Char('l') => Some(Edit::Dark(true)),
                _ => None,
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => Some(shift(-1)),
                MouseEventKind::ScrollDown => Some(shift(1)),
                MouseEventKind::Down(MouseButton::Left) => {
                    let pos = Position::new(m.column, m.row);
                    if !self.popup.contains(pos) {
                        return Some(Edit::Cancel);
                    }
                    if self.footer.contains(pos) {
                        return Some(if m.column < self.footer.x + self.footer.width / 2 {
                            Edit::Accept(self.preview())
                        } else {
                            Edit::Cancel
                        });
                    }
                    if !self.rows.contains(pos) {
                        return None;
                    }
                    let row = self.offset + usize::from(m.row - self.rows.y);
                    if row >= Theme::COUNT {
                        return None;
                    }
                    let dark = if m.column >= self.rows.right() - self.swatch_width {
                        true
                    } else if m.column >= self.rows.right() - 2 * self.swatch_width {
                        false
                    } else {
                        self.dark
                    };
                    Some(Edit::Accept(Theme::at(row, dark)))
                }
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseEvent};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn click_uses_the_displayed_row_after_scrolling_a_short_list() {
        let mut terminal = Terminal::new(TestBackend::new(24, 6)).unwrap();
        let mut picker = Picker::new(Theme::TERMINAL);
        picker.edit(Edit::Row(Theme::COUNT - 1));
        terminal.draw(|frame| picker.draw(frame)).unwrap();
        assert!(picker.offset > 0);
        // Click the top visible row's dark swatch, not the highlighted bottom row.
        let event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: picker.rows.right() - 1,
            row: picker.rows.y,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(
            picker.input(event),
            Some(Edit::Accept(Theme::at(picker.offset, true)))
        );
        let cancel = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: picker.footer.right() - 1,
            row: picker.footer.y,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(picker.input(cancel), Some(Edit::Cancel));
    }
}
