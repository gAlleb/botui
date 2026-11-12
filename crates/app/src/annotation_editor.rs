//! Shared range-comment editor for reflowable books and PDF text.
use crate::{AppTerminal, centered, prompt::LineInput};
use botui_store::HighlightColor;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Modifier,
    widgets::{Clear, Paragraph, Wrap},
};
use std::io;
pub(crate) struct Edit {
    pub body: String,
    pub color: Option<HighlightColor>,
    pub remove: bool,
}
pub(crate) fn edit(
    term: &mut AppTerminal,
    mut background: impl FnMut(&mut Frame),
    quote: &str,
    initial: &str,
    mut color: Option<HighlightColor>,
    color_only: bool,
) -> io::Result<Option<Edit>> {
    let mut lines: Vec<LineInput> = initial.split('\n').map(LineInput::new).collect();
    let mut current = lines.len().saturating_sub(1);
    loop {
        let mut buttons = Vec::new();
        let mut colors = Vec::new();
        term.draw(|f| {
            background(f);
            let area = centered(f.area(), 82, if color_only { 12 } else { 21 });
            let border = crate::appearance::block().title(if color_only {
                " Highlight "
            } else {
                " Comment "
            });
            crate::overlay::paint(f, area, |f| {
                let inner = border.inner(area);
                f.render_widget(Clear, area);
                f.render_widget(border, area);
                if inner.height < 4 {
                    return;
                }
                let actions = [
                    (
                        if color_only {
                            "[Enter Save]"
                        } else {
                            "[Ctrl+S Save]"
                        },
                        KeyCode::Char('s'),
                    ),
                    ("[Esc Cancel]", KeyCode::Esc),
                    (
                        if color_only { "" } else { "[Enter New line]" },
                        KeyCode::Enter,
                    ),
                    ("[Tab Color]", KeyCode::Tab),
                ];
                let (rects, footer_height) =
                    crate::reader::controls::layout(inner, &actions.map(|a| a.0));
                for ((label, key), rect) in actions.into_iter().zip(rects) {
                    f.render_widget(Paragraph::new(label), rect);
                    buttons.push((rect, key));
                }
                let quote_height = 4.min(inner.height.saturating_sub(footer_height + 3));
                f.render_widget(
                    Paragraph::new(quote)
                        .wrap(Wrap { trim: false })
                        .style(ratatui::style::Style::default().add_modifier(Modifier::ITALIC)),
                    Rect::new(
                        inner.x + 1,
                        inner.y,
                        inner.width.saturating_sub(2),
                        quote_height,
                    ),
                );
                let y = inner.y + quote_height;
                let choices: Vec<_> = HighlightColor::ALL
                    .into_iter()
                    .map(Some)
                    .chain([None])
                    .enumerate()
                    .map(|(i, c)| {
                        (
                            format!(
                                "[{} {}{}]",
                                i + 1,
                                c.map_or("None", HighlightColor::label),
                                if c == color { " ✓" } else { "" }
                            ),
                            c,
                        )
                    })
                    .collect();
                let (rects, _) = crate::reader::controls::layout(
                    Rect::new(inner.x, y, inner.width, 2),
                    &choices.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
                );
                for ((label, c), r) in choices.into_iter().zip(rects) {
                    let mut p = Paragraph::new(label);
                    if c == color {
                        p = p.style(
                            ratatui::style::Style::default().add_modifier(Modifier::REVERSED),
                        );
                    }
                    f.render_widget(p, r);
                    colors.push((r, c));
                }
                if !color_only {
                    let height = inner
                        .height
                        .saturating_sub(quote_height + 3 + footer_height)
                        as usize;
                    let offset = current.saturating_sub(height.saturating_sub(1));
                    for (index, line) in lines.iter().enumerate().skip(offset).take(height) {
                        let text = if index == current {
                            line.display(inner.width.saturating_sub(2), false)
                        } else {
                            line.value().into()
                        };
                        f.render_widget(
                            Paragraph::new(text),
                            Rect::new(
                                inner.x + 1,
                                y + 3 + (index - offset) as u16,
                                inner.width.saturating_sub(2),
                                1,
                            ),
                        );
                    }
                }
            });
            crate::settings::paint_dialog(f);
        })?;
        let mut event = crate::settings::read_event()?;
        if let Event::Mouse(m) = event
            && m.kind == MouseEventKind::Down(MouseButton::Left)
        {
            let p = Position::new(m.column, m.row);
            if let Some((_, key)) = buttons.iter().find(|(r, _)| r.contains(p)) {
                event = Event::Key(crossterm::event::KeyEvent::new(
                    *key,
                    if *key == KeyCode::Char('s') {
                        KeyModifiers::CONTROL
                    } else {
                        KeyModifiers::NONE
                    },
                ));
            }
        }
        let accept = match &event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Char('s') if k.modifiers.contains(KeyModifiers::CONTROL) => true,
                KeyCode::Enter if color_only || k.modifiers.contains(KeyModifiers::CONTROL) => true,
                KeyCode::Tab => {
                    let i = color
                        .and_then(|c| HighlightColor::ALL.iter().position(|v| *v == c))
                        .unwrap_or(5);
                    color = HighlightColor::ALL.get((i + 1) % 6).copied();
                    false
                }
                KeyCode::Char(c @ '1'..='6') if color_only => {
                    color = HighlightColor::ALL.get((c as u8 - b'1') as usize).copied();
                    false
                }
                KeyCode::Enter => {
                    let tail = lines[current].split();
                    current += 1;
                    lines.insert(current, tail);
                    false
                }
                KeyCode::Up => {
                    current = current.saturating_sub(1);
                    false
                }
                KeyCode::Down => {
                    current = (current + 1).min(lines.len() - 1);
                    false
                }
                KeyCode::Backspace if lines[current].at_start() && current > 0 => {
                    let tail = lines.remove(current).into_text();
                    current -= 1;
                    lines[current].append(&tail);
                    false
                }
                _ => {
                    if !color_only {
                        lines[current].edit(&event);
                    }
                    false
                }
            },
            Event::Paste(s) if !color_only => {
                for (i, part) in s.replace('\r', "").split('\n').enumerate() {
                    if i > 0 {
                        let tail = lines[current].split();
                        current += 1;
                        lines.insert(current, tail);
                    }
                    lines[current].edit(&Event::Paste(part.into()));
                }
                false
            }
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                let p = Position::new(m.column, m.row);
                if let Some((_, c)) = colors.iter().find(|(r, _)| r.contains(p)) {
                    color = *c;
                }
                false
            }
            _ => false,
        };
        if accept {
            let body = lines
                .into_iter()
                .map(LineInput::into_text)
                .collect::<Vec<_>>()
                .join("\n");
            let remove = body.trim().is_empty() && color.is_none();
            return Ok(Some(crate::annotation_editor::Edit {
                body,
                color,
                remove,
            }));
        }
    }
}
