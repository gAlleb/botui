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

/// A keyboard and mouse list used by page bookmarks, notes and reading statistics.
pub fn choose(
    term: &mut AppTerminal,
    title: &str,
    labels: &[String],
    selected: usize,
) -> io::Result<Option<usize>> {
    use ratatui::style::Style;
    use ratatui::widgets::{List, ListState};
    let mut list = ListState::default().with_selected(
        (!labels.is_empty()).then_some(selected.min(labels.len().saturating_sub(1))),
    );
    loop {
        let mut rows = Rect::default();
        let mut back = Rect::default();
        term.draw(|f| {
            let area = centered(f.area(), 70, (labels.len().clamp(3, 18) + 3) as u16);
            let block = Block::bordered().title(format!(" {title} "));
            let inner = block.inner(area);
            f.render_widget(Clear, area);
            f.render_widget(block, area);
            rows = Rect {
                height: inner.height.saturating_sub(1),
                ..inner
            };
            if labels.is_empty() {
                f.render_widget(Paragraph::new(" No entries yet."), rows);
            } else {
                f.render_stateful_widget(
                    List::new(labels.iter().map(String::as_str))
                        .highlight_style(Style::new().reversed()),
                    rows,
                    &mut list,
                );
            }
            back = Rect::new(
                inner.x,
                inner.bottom().saturating_sub(1),
                inner.width.min(10),
                inner.height.min(1),
            );
            f.render_widget(Paragraph::new("[ Back ]").cyan(), back);
            crate::settings::paint(f);
        })?;
        let move_by = match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Enter => return Ok(list.selected()),
                KeyCode::Up | KeyCode::Char('k') => -1,
                KeyCode::Down | KeyCode::Char('j') => 1,
                KeyCode::PageDown => rows.height as isize,
                KeyCode::PageUp => -(rows.height as isize),
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
                    let i = list.offset() + usize::from(m.row - rows.y);
                    if i < labels.len() {
                        return Ok(Some(i));
                    }
                    continue;
                }
                MouseEventKind::ScrollDown => 1,
                MouseEventKind::ScrollUp => -1,
                _ => continue,
            },
            _ => continue,
        };
        if let Some(i) = list.selected() {
            list.select(Some(i.saturating_add_signed(move_by).min(labels.len() - 1)));
        }
    }
}
