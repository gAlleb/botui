mod line;
use crate::{AppTerminal, centered};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
pub(crate) use line::LineInput;
use ratatui::{
    layout::{Position, Rect},
    style::Stylize,
    widgets::{Clear, Paragraph},
};
use std::io;

/// One-line prompt used by catalog search and the local file browser.
pub fn edit(
    term: &mut AppTerminal,
    title: &str,
    submit: &str,
    initial: &str,
    underneath: impl FnMut(&mut ratatui::Frame),
) -> io::Result<Option<String>> {
    field(term, title, submit, initial, false, underneath)
}
pub fn field(
    term: &mut AppTerminal,
    title: &str,
    submit: &str,
    initial: &str,
    secret: bool,
    mut underneath: impl FnMut(&mut ratatui::Frame),
) -> io::Result<Option<String>> {
    let mut value = LineInput::new(initial);
    loop {
        let mut accept = Rect::default();
        let mut cancel = Rect::default();
        term.draw(|f| {
            underneath(f);
            let area = centered(f.area(), 64, 5);
            let border = crate::appearance::block().title(format!(" {title} "));
            let inner = border.inner(area);
            f.render_widget(Clear, area);
            f.render_widget(border, area);
            if !inner.is_empty() {
                let label = value.display(inner.width.saturating_sub(1), secret);
                f.render_widget(Paragraph::new(format!(" {label}")), inner);
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
            crate::settings::paint_interface(f);
        })?;
        let event = crate::settings::read_event()?;
        match &event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                KeyCode::Enter => return Ok(Some(value.finish(secret))),
                _ => value.edit(&event),
            },
            Event::Paste(_) => value.edit(&event),
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                let p = Position::new(m.column, m.row);
                if accept.contains(p) {
                    return Ok(Some(value.finish(secret)));
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
    choose_rows(term, title, labels, selected, false).map(|r| r.map(|(i, _)| i))
}

pub fn choose_removable(
    term: &mut AppTerminal,
    title: &str,
    labels: &[String],
    selected: usize,
) -> io::Result<Option<(usize, bool)>> {
    choose_rows(term, title, labels, selected, true)
}

fn choose_rows(
    term: &mut AppTerminal,
    title: &str,
    labels: &[String],
    selected: usize,
    removable: bool,
) -> io::Result<Option<(usize, bool)>> {
    use ratatui::style::Style;
    use ratatui::widgets::{List, ListState};
    let mut list = ListState::default().with_selected(
        (!labels.is_empty()).then_some(selected.min(labels.len().saturating_sub(1))),
    );
    loop {
        let mut rows = Rect::default();
        let mut back = Rect::default();
        let mut delete = Rect::default();
        let mut delete_rows = Rect::default();
        term.draw(|f| {
            let area = centered(f.area(), 70, (labels.len().clamp(3, 18) + 3) as u16);
            let block = crate::appearance::block().title(format!(" {title} "));
            let inner = block.inner(area);
            f.render_widget(Clear, area);
            f.render_widget(block, area);
            rows = Rect {
                height: inner.height.saturating_sub(1),
                ..inner
            };
            if removable && !labels.is_empty() && rows.width >= 18 {
                delete_rows = Rect::new(rows.right() - 3, rows.y, 3, rows.height);
                rows.width -= 4;
            }
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
            if !delete_rows.is_empty() {
                for row in 0..usize::from(delete_rows.height)
                    .min(labels.len().saturating_sub(list.offset()))
                {
                    f.render_widget(
                        Paragraph::new("[×]").dim(),
                        Rect::new(delete_rows.x, delete_rows.y + row as u16, 3, 1),
                    );
                }
            }
            back = Rect::new(
                inner.x,
                inner.bottom().saturating_sub(1),
                inner.width.min(10),
                inner.height.min(1),
            );
            f.render_widget(Paragraph::new("[ Back ]").cyan(), back);
            if removable && list.selected().is_some() {
                let x = back.right();
                delete = Rect::new(
                    x,
                    back.y,
                    inner.right().saturating_sub(x).min(12),
                    back.height,
                );
                f.render_widget(Paragraph::new("[Del Delete]").cyan(), delete);
            }
            crate::settings::paint_interface(f);
        })?;
        let event = crate::settings::read_event()?;
        if removable {
            let target = match &event {
                Event::Key(k) if k.kind != KeyEventKind::Release && k.code == KeyCode::Delete => {
                    list.selected()
                }
                Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                    let at = Position::new(m.column, m.row);
                    if delete.contains(at) {
                        list.selected()
                    } else if delete_rows.contains(at) {
                        Some(list.offset() + usize::from(m.row - delete_rows.y))
                            .filter(|&i| i < labels.len())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(i) = target {
                list.select(Some(i));
                if document_panel(
                    term,
                    "Delete bookmark?",
                    labels[i].trim(),
                    Some("[ Delete bookmark ]"),
                )? {
                    return Ok(Some((i, true)));
                }
                continue;
            }
        }
        let move_by = match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Enter => return Ok(list.selected().map(|i| (i, false))),
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
                        return Ok(Some((i, false)));
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

/// Wrapped text has a scrollable viewport. A state reset starts on Cancel and requires
/// choosing the explicit action; opening the card or pressing Enter once cannot reset it.
fn document_panel(
    term: &mut AppTerminal,
    title: &str,
    body: &str,
    action: Option<&str>,
) -> io::Result<bool> {
    use ratatui::style::Stylize;
    use ratatui::widgets::{Paragraph, Wrap};
    let reset = action.is_some();
    let mut offset = 0u16;
    let mut danger = false;
    loop {
        let mut back = Rect::default();
        let mut accept = Rect::default();
        let mut max = 0;
        term.draw(|f| {
            let screen = f.area();
            let area = crate::centered(
                screen,
                82,
                if reset {
                    12
                } else {
                    screen.height.saturating_sub(2)
                },
            );
            let block = crate::appearance::block().title(format!(" {title} "));
            let r = block.inner(area);
            f.render_widget(block, area);
            if r.height >= 2 {
                let view = Rect::new(r.x + 1, r.y, r.width.saturating_sub(2), r.height - 1);
                let p = Paragraph::new(body).wrap(Wrap { trim: true });
                max = (p.line_count(view.width) as u16).saturating_sub(view.height);
                offset = offset.min(max);
                f.render_widget(p.scroll((offset, 0)), view);
                let label = if reset { "[ Cancel ]" } else { "[ Close ]" };
                let y = r.bottom() - 1;
                back = Rect::new(r.x, y, r.width.min(label.len() as u16), 1);
                let p = Paragraph::new(label);
                f.render_widget(if !danger { p.reversed() } else { p }, back);
                if let Some(l) = action {
                    let x = back.right() + 1;
                    accept = Rect::new(x, y, r.right().saturating_sub(x).min(l.len() as u16), 1);
                    let p = Paragraph::new(l);
                    f.render_widget(if danger { p.reversed() } else { p }, accept);
                }
            }
            crate::settings::paint_interface(f);
        })?;
        match crate::settings::read_event()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(false),
                KeyCode::Enter => return Ok(reset && danger),
                KeyCode::Right | KeyCode::Tab if reset => danger = !danger,
                KeyCode::Left | KeyCode::BackTab if reset => danger = !danger,
                KeyCode::Up => offset = offset.saturating_sub(1),
                KeyCode::Down => offset = offset.saturating_add(1).min(max),
                KeyCode::PageUp => offset = offset.saturating_sub(8),
                KeyCode::PageDown => offset = offset.saturating_add(8).min(max),
                _ => {}
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => offset = offset.saturating_sub(3),
                MouseEventKind::ScrollDown => offset = offset.saturating_add(3).min(max),
                MouseEventKind::Down(MouseButton::Left)
                    if back.contains(Position::new(m.column, m.row)) =>
                {
                    return Ok(false);
                }
                MouseEventKind::Down(MouseButton::Left)
                    if reset && accept.contains(Position::new(m.column, m.row)) =>
                {
                    return Ok(true);
                }
                _ => {}
            },
            _ => {}
        }
    }
}
pub fn confirm_reset(term: &mut AppTerminal, title: &str) -> io::Result<bool> {
    document_panel(
        term,
        "Reset reading state?",
        &format!(
            "{}\n\nBookOrbit will mark this book unread and clear progress for every edition, reading sessions, attempts and reading dates, including synced Kobo and KOReader progress. Files, notes, highlights and bookmarks are kept. This cannot be undone.",
            crate::shelf::single_line(title.to_owned())
        ),
        Some("[ Reset reading state ]"),
    )
}
pub fn message(term: &mut AppTerminal, title: &str, body: &str) -> io::Result<()> {
    document_panel(term, title, body, None).map(|_| ())
}
