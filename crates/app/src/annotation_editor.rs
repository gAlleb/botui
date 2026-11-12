//! Shared range-comment editor for reflowable books and PDF text.
use crate::{AppTerminal, centered, prompt::LineInput};
use botui_store::HighlightColor;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};
use std::io;
pub(crate) fn color_style(color: HighlightColor) -> Style {
    let [r, g, b] = color.rgb();
    let light = u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114;
    Style::default()
        .bg(Color::Rgb(r, g, b))
        .fg(if light >= 128_000 {
            Color::Rgb(25, 25, 30)
        } else {
            Color::Rgb(250, 250, 250)
        })
}
fn palette(initial: Option<HighlightColor>) -> Vec<(char, Option<HighlightColor>)> {
    // Preserve the original five shortcuts and the sixth no-fill choice.
    let mut colors: Vec<_> = HighlightColor::ALL[..5]
        .iter()
        .copied()
        .map(Some)
        .chain([None])
        .chain(HighlightColor::ALL[5..].iter().copied().map(Some))
        .zip("1234567890g".chars())
        .map(|(c, key)| (key, c))
        .collect();
    if matches!(initial, Some(HighlightColor::Rgb(_))) {
        colors.push(('x', initial));
    }
    colors
}
fn color_name(color: Option<HighlightColor>) -> String {
    match color {
        Some(c @ HighlightColor::Rgb(_)) => c.hex(),
        Some(c) => c.label().into(),
        None => "No fill".into(),
    }
}
fn color_chip(key: char, color: Option<HighlightColor>, selected: bool) -> Line<'static> {
    let name = format!(
        "{}{}",
        color_name(color),
        if selected { " ✓" } else { "  " }
    );
    let style = color.map_or(Style::default(), color_style);
    Line::from(vec![
        Span::raw(format!("[{key} ")),
        Span::styled(
            name,
            if selected {
                style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                style
            },
        ),
        Span::raw(" "),
        Span::styled(
            if color.is_some() { "■" } else { "∅" },
            color.map_or(Style::default(), |c| {
                let [r, g, b] = c.rgb();
                Style::default().fg(Color::Rgb(r, g, b))
            }),
        ),
        Span::raw("]"),
    ])
}
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
    let choices = palette(color);
    loop {
        let mut buttons = Vec::new();
        let mut colors = Vec::new();
        term.draw(|f| {
            background(f);
            let chips: Vec<_> = choices
                .iter()
                .map(|(key, c)| color_chip(*key, *c, *c == color))
                .collect();
            let labels: Vec<String> = chips.iter().map(ToString::to_string).collect();
            let (mut palette_rects, color_height) = crate::reader::controls::layout(
                Rect::new(0, 0, f.area().width.min(82).saturating_sub(2), 64),
                &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            let area = centered(f.area(), 82, color_height + if color_only { 9 } else { 17 });
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
                let quote_height = 3.min(
                    inner
                        .height
                        .saturating_sub(footer_height + color_height + 2),
                );
                f.render_widget(
                    Paragraph::new(quote)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().add_modifier(Modifier::ITALIC)),
                    Rect::new(
                        inner.x + 1,
                        inner.y,
                        inner.width.saturating_sub(2),
                        quote_height,
                    ),
                );
                let y = inner.y + quote_height;
                for (((_, c), chip), r) in choices.iter().zip(chips).zip(&mut palette_rects) {
                    r.x += inner.x;
                    r.y = y + r.y.saturating_sub(64 - color_height);
                    f.render_widget(Paragraph::new(chip), *r);
                    colors.push((*r, *c));
                }
                let text_y = y + color_height + 1;
                if !color_only {
                    let height = inner
                        .height
                        .saturating_sub(quote_height + color_height + 1 + footer_height)
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
                                text_y + (index - offset) as u16,
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
                    let i = choices.iter().position(|(_, c)| *c == color).unwrap_or(0);
                    color = choices[(i + 1) % choices.len()].1;
                    false
                }
                KeyCode::BackTab => {
                    let i = choices.iter().position(|(_, c)| *c == color).unwrap_or(0);
                    color = choices[(i + choices.len() - 1) % choices.len()].1;
                    false
                }
                KeyCode::Char(c) if color_only && choices.iter().any(|(key, _)| *key == c) => {
                    color = choices.iter().find(|(key, _)| *key == c).unwrap().1;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palette_keeps_shortcuts_and_wraps_all_colored_choices() {
        let custom = HighlightColor::Rgb([12, 34, 56]);
        let choices = palette(Some(custom));
        assert_eq!(choices[1], ('2', Some(HighlightColor::Green)));
        assert_eq!(choices[5], ('6', None));
        assert_eq!(choices.last(), Some(&('x', Some(custom))));
        for width in [24, 50, 78, 120] {
            let chips: Vec<_> = choices
                .iter()
                .map(|(k, c)| color_chip(*k, *c, false))
                .collect();
            let labels: Vec<_> = chips.iter().map(ToString::to_string).collect();
            let area = Rect::new(0, 0, width, 32);
            let (rects, _) = crate::reader::controls::layout(
                area,
                &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            assert!(
                rects
                    .iter()
                    .all(|r| !r.is_empty() && r.intersection(area) == *r)
            );
            for (((_, c), chip), rect) in choices.iter().zip(chips).zip(rects) {
                if let Some(c) = c {
                    let [r, g, b] = c.rgb();
                    assert_eq!(chip.spans[1].style.bg, Some(Color::Rgb(r, g, b)));
                }
                assert_eq!(usize::from(rect.width), chip.width());
            }
        }
    }
}
