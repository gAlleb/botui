//! Stable-size connection editor with a plain-language explanation above its fields.
use crate::{AppTerminal, centered};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    style::{Style, Stylize},
    widgets::{Clear, List, ListState, Paragraph, Wrap},
};
use std::io;

pub fn choose(
    term: &mut AppTerminal,
    title: &str,
    description: &str,
    labels: &[String],
    selected: usize,
) -> io::Result<Option<usize>> {
    let mut state =
        ListState::default().with_selected(Some(selected.min(labels.len().saturating_sub(1))));
    loop {
        let mut rows = Rect::default();
        let mut back = Rect::default();
        term.draw(|f| {
            let area = centered(f.area(), 92, 25);
            let block = crate::appearance::block().title(format!(" {title} "));
            let inner = block.inner(area);
            f.render_widget(Clear, area);
            f.render_widget(block, area);
            if inner.height < 3 {
                return;
            }
            let description_height = if description.trim().is_empty() {
                0
            } else {
                4.min(inner.height.saturating_sub(3))
            };
            f.render_widget(
                Paragraph::new(description).wrap(Wrap { trim: true }).dim(),
                Rect::new(
                    inner.x + 1,
                    inner.y,
                    inner.width.saturating_sub(2),
                    description_height,
                ),
            );
            rows = Rect::new(
                inner.x + 1,
                inner.y + description_height + 1,
                inner.width.saturating_sub(2),
                inner.height.saturating_sub(description_height + 3),
            );
            f.render_stateful_widget(
                List::new(crate::prompt::columns(labels, rows.width))
                    .highlight_style(Style::new().reversed()),
                rows,
                &mut state,
            );
            back = Rect::new(
                inner.x + 1,
                inner.bottom() - 1,
                10.min(inner.width.saturating_sub(2)),
                1,
            );
            f.render_widget(
                Paragraph::new("[Esc Back]  [Enter / click Select]").dim(),
                Rect::new(back.x, back.y, inner.width.saturating_sub(2), 1),
            );
            crate::settings::paint_dialog(f);
        })?;
        let delta = match crate::settings::read_event()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Enter => return Ok(state.selected()),
                KeyCode::Up | KeyCode::Char('k') => -1,
                KeyCode::Down | KeyCode::Char('j') => 1,
                KeyCode::PageUp => -(rows.height as isize),
                KeyCode::PageDown => rows.height as isize,
                KeyCode::Home => isize::MIN,
                KeyCode::End => isize::MAX,
                _ => continue,
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left)
                    if back.contains(Position::new(m.column, m.row)) =>
                {
                    return Ok(None);
                }
                MouseEventKind::Down(MouseButton::Left)
                    if rows.contains(Position::new(m.column, m.row)) =>
                {
                    let index = state.offset() + usize::from(m.row - rows.y);
                    if index < labels.len() {
                        return Ok(Some(index));
                    }
                    continue;
                }
                MouseEventKind::ScrollUp => -1,
                MouseEventKind::ScrollDown => 1,
                _ => continue,
            },
            _ => continue,
        };
        state.select(Some(
            state
                .selected()
                .unwrap_or(0)
                .saturating_add_signed(delta)
                .min(labels.len().saturating_sub(1)),
        ));
    }
}
