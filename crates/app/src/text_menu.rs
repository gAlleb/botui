//! A small text action menu at the pointer, over the existing reader frame.
use crate::AppTerminal;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
    widgets::{Clear, Paragraph},
};
use std::io;

#[derive(Clone, Copy)]
pub enum Action {
    Copy,
    Highlight,
    Search,
    Translate,
    Define,
    AddNote,
}
const ITEMS: [(&str, Action); 6] = [
    ("Copy", Action::Copy),
    ("Highlight", Action::Highlight),
    ("Search", Action::Search),
    ("Translate", Action::Translate),
    ("Define", Action::Define),
    ("Add note", Action::AddNote),
];
fn placement(screen: Rect, at: Position) -> Rect {
    let width = 15.min(screen.width);
    let height = 8.min(screen.height);
    let x = if at.x.saturating_add(1 + width) <= screen.right() {
        at.x.saturating_add(1)
    } else {
        at.x.saturating_sub(width)
    };
    let y = if at.y.saturating_add(1 + height) <= screen.bottom() {
        at.y.saturating_add(1)
    } else {
        at.y.saturating_sub(height)
    };
    Rect::new(
        x.clamp(screen.x, screen.right().saturating_sub(width)),
        y.clamp(screen.y, screen.bottom().saturating_sub(height)),
        width,
        height,
    )
}
pub fn choose(
    term: &mut AppTerminal,
    at: Position,
    mut underneath: impl FnMut(&mut Frame),
) -> io::Result<Option<Action>> {
    let mut selected = 0usize;
    loop {
        let mut panel = Rect::default();
        let mut rows = Rect::default();
        let mut offset = 0;
        term.draw(|f| {
            underneath(f);
            panel = placement(f.area(), at);
            crate::overlay::paint(f, panel, |f| {
                f.render_widget(Clear, panel);
                let block = crate::appearance::block();
                rows = block.inner(panel);
                f.render_widget(block, panel);
                offset = (selected + 1).saturating_sub(usize::from(rows.height));
                for (row, (label, _)) in ITEMS
                    .iter()
                    .enumerate()
                    .skip(offset)
                    .take(rows.height as usize)
                {
                    let p = Paragraph::new(format!(" {label}"));
                    f.render_widget(
                        if row == selected { p.reversed() } else { p },
                        Rect::new(rows.x, rows.y + (row - offset) as u16, rows.width, 1),
                    );
                }
                crate::settings::paint_dialog(f);
            });
        })?;
        match crate::settings::read_event()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Enter => return Ok(Some(ITEMS[selected].1)),
                KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1).min(ITEMS.len() - 1)
                }
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = ITEMS.len() - 1,
                _ => (),
            },
            Event::Mouse(m) => {
                let p = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if !panel.contains(p) {
                            return Ok(None);
                        }
                        if rows.contains(p) {
                            let i = offset + usize::from(m.row - rows.y);
                            if let Some((_, action)) = ITEMS.get(i) {
                                return Ok(Some(*action));
                            }
                        }
                    }
                    MouseEventKind::Moved if rows.contains(p) => {
                        selected = (offset + usize::from(m.row - rows.y)).min(ITEMS.len() - 1)
                    }
                    MouseEventKind::ScrollUp => selected = selected.saturating_sub(1),
                    MouseEventKind::ScrollDown => selected = (selected + 1).min(ITEMS.len() - 1),
                    _ => (),
                }
            }
            _ => (),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_is_small_adjacent_and_stays_visible() {
        let screen = Rect::new(0, 0, 120, 40);
        assert_eq!(
            placement(screen, Position::new(30, 10)),
            Rect::new(31, 11, 15, 8)
        );
        assert_eq!(
            placement(screen, Position::new(119, 39)),
            Rect::new(104, 31, 15, 8)
        );
        for size in [(120, 40), (24, 10), (10, 4)] {
            let screen = Rect::new(0, 0, size.0, size.1);
            let popup = placement(screen, Position::new(119, 39));
            assert_eq!(popup.intersection(screen), popup);
        }
    }
}
