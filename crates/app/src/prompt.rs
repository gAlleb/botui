use crate::{AppTerminal, centered};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    style::Stylize,
    widgets::{Block, Clear, Paragraph},
};
use std::io;

/// One-line prompt used by catalog search and the local file browser.
pub fn edit(
    term: &mut AppTerminal,
    title: &str,
    submit: &str,
    initial: &str,
    mut underneath: impl FnMut(&mut ratatui::Frame),
) -> io::Result<Option<String>> {
    let mut value = initial.to_owned();
    let mut cursor = value.len();
    loop {
        let mut accept = Rect::default();
        let mut cancel = Rect::default();
        term.draw(|f| {
            underneath(f);
            let area = centered(f.area(), 64, 5);
            let border = Block::bordered().title(format!(" {title} "));
            let inner = border.inner(area);
            f.render_widget(Clear, area);
            f.render_widget(border, area);
            if !inner.is_empty() {
                let mut visible = &value[..cursor];
                use ratatui::text::Line;
                while Line::from(visible).width() + 2 > inner.width as usize && !visible.is_empty()
                {
                    visible = &visible[visible.chars().next().unwrap().len_utf8()..];
                }
                f.render_widget(
                    Paragraph::new(format!(" {visible}▏{}", &value[cursor..])),
                    inner,
                );
                accept = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(12), 1);
                cancel = Rect::new(
                    accept.right(),
                    accept.y,
                    inner.right().saturating_sub(accept.right()).min(10),
                    1,
                );
                f.render_widget(Paragraph::new(submit).cyan(), accept);
                f.render_widget(Paragraph::new("[ Cancel ]"), cancel);
            }
            crate::settings::paint(f);
        })?;
        match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                KeyCode::Enter => return Ok(Some(value.trim().into())),
                KeyCode::Home => cursor = 0,
                KeyCode::End => cursor = value.len(),
                KeyCode::Left => {
                    cursor = value[..cursor]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(i, _)| i)
                }
                KeyCode::Right => {
                    cursor += value[cursor..].chars().next().map_or(0, char::len_utf8)
                }
                KeyCode::Backspace if cursor > 0 => {
                    let i = value[..cursor].char_indices().next_back().unwrap().0;
                    value.drain(i..cursor);
                    cursor = i;
                }
                KeyCode::Delete if cursor < value.len() => {
                    value.remove(cursor);
                }
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    value.clear();
                    cursor = 0;
                }
                KeyCode::Char(c)
                    if !c.is_control()
                        && !k.modifiers.contains(KeyModifiers::CONTROL)
                        && value.chars().count() < 2048 =>
                {
                    value.insert(cursor, c);
                    cursor += c.len_utf8();
                }
                _ => (),
            },
            Event::Paste(s) => {
                let s: String = s
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(2048usize.saturating_sub(value.chars().count()))
                    .collect();
                value.insert_str(cursor, &s);
                cursor += s.len();
            }
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                let p = Position::new(m.column, m.row);
                if accept.contains(p) {
                    return Ok(Some(value.trim().into()));
                }
                if cancel.contains(p) {
                    return Ok(None);
                }
            }
            _ => (),
        }
    }
}
