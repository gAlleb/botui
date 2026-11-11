use super::{Reader, study::Command};
use crate::{AppTerminal, centered, prompt::LineInput};
use botui_store::{HighlightColor, LocalState, Note};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint, Position, Rect},
    style::Modifier,
    widgets::{Clear, Paragraph, Row, Table, TableState, Wrap},
};
use std::{
    io,
    process::{Command as Process, Stdio},
};

pub(super) fn run(
    term: &mut AppTerminal,
    reader: &mut Reader,
    store: Option<&LocalState>,
    command: Command,
) -> io::Result<()> {
    let store = || store.ok_or_else(|| io::Error::other("Local storage is unavailable"));
    match command {
        Command::Normal => {
            reader.study.toggle_normal(reader.pager.anchor());
        }
        Command::Clear => {
            reader.study.clear();
        }
        Command::Copy => {
            let range = reader
                .study
                .range(&reader.doc, reader.pager.anchor())
                .ok_or_else(|| io::Error::other("Select some text first"))?;
            reader.study.message =
                Some(super::clipboard::copy(&reader.doc.text_range(range))?.into());
        }
        Command::Comment | Command::Highlight | Command::Peek => {
            let note = reader.study.active_note().cloned();
            edit(
                term,
                reader,
                store()?,
                note.as_ref(),
                command == Command::Highlight,
            )?;
        }
        Command::List => list(term, reader, store()?)?,
        Command::Export => {
            let path =
                super::notes::export(&reader.doc.title, Some(&reader.doc), &store()?.notes()?)?;
            reader.study.message = Some(format!("Exported {}", path.display()));
        }
        Command::Lookup => {
            let range = reader
                .study
                .range(&reader.doc, reader.pager.anchor())
                .ok_or_else(|| io::Error::other("Select some text first"))?;
            lookup(term, reader, &reader.doc.text_range(range))?;
        }
        Command::Mark(name) => {
            let at = reader.study.cursor.unwrap_or(reader.pager.anchor());
            store()?.set_named_mark(name, at)?;
            reader.study.message = Some(format!("Mark {name} saved · '{name} to return"));
        }
        Command::Jump(name) => {
            let at = store()?
                .named_marks()?
                .get(&name)
                .copied()
                .ok_or_else(|| io::Error::other(format!("Mark {name} is not set")))?;
            reader.history.push(reader.pager.anchor());
            reader.pager.seek(at);
            reader.study.cursor = Some(at);
            reader.study.selection = None;
            reader.dirty = true;
        }
    }
    Ok(())
}

fn edit(
    term: &mut AppTerminal,
    reader: &mut Reader,
    store: &LocalState,
    note: Option<&Note>,
    color_only: bool,
) -> io::Result<()> {
    let range = note
        .and_then(|n| n.range().or_else(|| reader.doc.paragraph_at(n.anchor())))
        .or_else(|| reader.study.range(&reader.doc, reader.pager.anchor()))
        .ok_or_else(|| io::Error::other("Select some text first"))?;
    let quote = note
        .filter(|n| !n.quote.is_empty())
        .map_or_else(|| reader.doc.text_range(range), |n| n.quote.clone());
    let mut color = note.map_or(Some(HighlightColor::Yellow), |n| n.color);
    let initial = note.map_or("", |n| n.body.as_str());
    let mut lines: Vec<LineInput> = initial.split('\n').map(LineInput::new).collect();
    let mut current = lines.len().saturating_sub(1);
    loop {
        let mut buttons = Vec::new();
        let mut colors = Vec::new();
        term.draw(|f| {
            reader.draw(f, None);
            let area = centered(f.area(), 82, if color_only { 12 } else { 21 });
            let border = crate::appearance::block().title(if color_only {
                " Highlight "
            } else {
                " Comment "
            });
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
            let (rects, footer_height) = super::controls::layout(inner, &actions.map(|a| a.0));
            for ((label, key), rect) in actions.into_iter().zip(rects) {
                f.render_widget(Paragraph::new(label), rect);
                buttons.push((rect, key));
            }
            let quote_height = 4.min(inner.height.saturating_sub(footer_height + 3));
            f.render_widget(
                Paragraph::new(quote.as_str())
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
                        format!("{} {}", i + 1, c.map_or("None", HighlightColor::label)),
                        c,
                    )
                })
                .collect();
            let (rects, _) = super::controls::layout(
                Rect::new(inner.x, y, inner.width, 2),
                &choices.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
            );
            for ((label, c), r) in choices.into_iter().zip(rects) {
                let mut p = Paragraph::new(label);
                if c == color {
                    p = p.style(ratatui::style::Style::default().add_modifier(Modifier::REVERSED));
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
            crate::settings::paint_interface(f);
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
                KeyCode::Esc => return Ok(()),
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
            let remove = body.trim().is_empty() && (!color_only || color.is_none());
            let message = if remove {
                if let Some(note) = note {
                    reader.study.notes = store.delete_note(&note.id)?;
                    "Annotation deleted"
                } else {
                    "Empty note discarded"
                }
            } else {
                reader.study.notes =
                    store.annotate(note.map(|n| n.id.as_str()), range, quote, color, body)?;
                "Annotation saved on this device"
            };
            reader.study.selection = None;
            reader.study.cursor = Some(range.start);
            reader.study.message = Some(message.into());
            return Ok(());
        }
    }
}

fn list(term: &mut AppTerminal, reader: &mut Reader, store: &LocalState) -> io::Result<()> {
    let mut query = LineInput::new("");
    let mut filtering = false;
    let mut selected = 0usize;
    let mut table = TableState::default();
    loop {
        let query_text = query.value().to_lowercase();
        let notes: Vec<Note> = reader
            .study
            .notes
            .iter()
            .filter(|n| {
                format!("{} {}", n.quote, n.body)
                    .to_lowercase()
                    .contains(&query_text)
            })
            .cloned()
            .collect();
        selected = selected.min(notes.len().saturating_sub(1));
        table.select((!notes.is_empty()).then_some(selected));
        let mut rows_area = Rect::default();
        let mut buttons = Vec::new();
        term.draw(|f| {
            reader.draw(f, None);
            let area = centered(f.area(), 90, 28);
            let border =
                crate::appearance::block().title(format!(" Highlights & notes · {} ", notes.len()));
            let inner = border.inner(area);
            f.render_widget(Clear, area);
            f.render_widget(border, area);
            if inner.height < 4 {
                return;
            }
            let actions = [
                ("[Enter Go]", KeyCode::Enter),
                ("[N Edit]", KeyCode::Char('n')),
                ("[H Color]", KeyCode::Char('h')),
                ("[Y Copy]", KeyCode::Char('y')),
                ("[Del Delete]", KeyCode::Delete),
                ("[/ Find]", KeyCode::Char('/')),
                ("[E Export]", KeyCode::Char('e')),
                ("[Esc Back]", KeyCode::Esc),
            ];
            let (rects, footer_height) = super::controls::layout(inner, &actions.map(|a| a.0));
            for ((label, key), rect) in actions.into_iter().zip(rects) {
                f.render_widget(Paragraph::new(label), rect);
                buttons.push((rect, key));
            }
            f.render_widget(
                Paragraph::new(format!(
                    " Find: {}",
                    if filtering {
                        query.display(inner.width.saturating_sub(8), false)
                    } else {
                        query.value().into()
                    }
                )),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            let preview_height = inner.height.saturating_sub(3 + footer_height).min(6);
            rows_area = Rect::new(
                inner.x,
                inner.y + 2,
                inner.width,
                inner
                    .height
                    .saturating_sub(preview_height + 3 + footer_height),
            );
            let rows = notes.iter().map(|n| {
                let quote = if n.quote.is_empty() {
                    reader
                        .doc
                        .paragraph_at(n.anchor())
                        .map_or_else(String::new, |r| reader.doc.text_range(r))
                } else {
                    n.quote.clone()
                };
                Row::new([
                    n.color.map_or("Note", HighlightColor::label).to_owned(),
                    quote.replace('\n', " "),
                ])
            });
            f.render_stateful_widget(
                Table::new(rows, [Constraint::Length(8), Constraint::Fill(1)]).row_highlight_style(
                    ratatui::style::Style::default().add_modifier(Modifier::REVERSED),
                ),
                rows_area,
                &mut table,
            );
            if let Some(note) = notes.get(selected) {
                let preview = format!("{}\n\n{}", note.quote, note.body);
                f.render_widget(
                    Paragraph::new(preview).wrap(Wrap { trim: false }),
                    Rect::new(inner.x, rows_area.bottom() + 1, inner.width, preview_height),
                );
            }
            crate::settings::paint_interface(f);
        })?;
        let event = crate::settings::read_event()?;
        let key = match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => k.code,
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                let p = Position::new(m.column, m.row);
                if rows_area.contains(p) {
                    selected = table.offset() + usize::from(m.row - rows_area.y);
                    filtering = false;
                    continue;
                }
                let Some((_, key)) = buttons.iter().find(|(r, _)| r.contains(p)) else {
                    continue;
                };
                *key
            }
            Event::Paste(_) if filtering => {
                query.edit(&event);
                selected = 0;
                continue;
            }
            _ => continue,
        };
        if filtering {
            match key {
                KeyCode::Enter | KeyCode::Esc => filtering = false,
                _ => {
                    query.edit(&event);
                    selected = 0;
                }
            }
            continue;
        }
        match key {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => selected = selected.saturating_add(1),
            KeyCode::Home => selected = 0,
            KeyCode::End => selected = notes.len().saturating_sub(1),
            KeyCode::PageUp => selected = selected.saturating_sub(rows_area.height as usize),
            KeyCode::PageDown => selected = selected.saturating_add(rows_area.height as usize),
            KeyCode::Char('/') => filtering = true,
            KeyCode::Char('e' | 'E') => {
                let path = super::notes::export(&reader.doc.title, Some(&reader.doc), &notes)?;
                crate::prompt::message(term, "Exported annotations", &path.display().to_string())?;
            }
            _ => {
                if let Some(note) = notes.get(selected) {
                    match key {
                        KeyCode::Enter => {
                            reader.history.push(reader.pager.anchor());
                            reader.pager.seek(note.anchor());
                            reader.dirty = true;
                            reader.study.cursor = Some(note.anchor());
                            reader.study.selection = note.range();
                            return Ok(());
                        }
                        KeyCode::Char('n' | 'N' | 'h' | 'H') => edit(
                            term,
                            reader,
                            store,
                            Some(note),
                            matches!(key, KeyCode::Char('h' | 'H')),
                        )?,
                        KeyCode::Char('y' | 'Y') => {
                            let quote = if note.quote.is_empty() {
                                reader
                                    .doc
                                    .paragraph_at(note.anchor())
                                    .map_or_else(String::new, |r| reader.doc.text_range(r))
                            } else {
                                note.quote.clone()
                            };
                            super::clipboard::copy(&quote)?;
                        }
                        KeyCode::Delete
                            if crate::prompt::choose(
                                term,
                                "Delete this annotation?",
                                &["Cancel".into(), "Delete annotation".into()],
                                0,
                            )? == Some(1) =>
                        {
                            reader.study.notes = store.delete_note(&note.id)?;
                        }
                        _ => (),
                    }
                }
            }
        }
    }
}

fn lookup(term: &mut AppTerminal, reader: &mut Reader, text: &str) -> io::Result<()> {
    let config = crate::settings::Settings::load()?.lookup_command;
    let Some(program) = config.first() else {
        return Err(io::Error::other(
            "Set lookup_command in config.toml, e.g. [\"sdcv\", \"-n\", \"{text}\"]",
        ));
    };
    let args: Vec<_> = config[1..]
        .iter()
        .map(|arg| arg.replace("{text}", text))
        .collect();
    let mut child = Process::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    use std::io::Read;
    let collect = |pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut data = String::new();
            pipe.take(256 * 1024)
                .read_to_string(&mut data)
                .map(|_| data)
        })
    };
    let out = collect(Box::new(stdout));
    let err = collect(Box::new(stderr));
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let output = out
                .join()
                .map_err(|_| io::Error::other("Lookup worker stopped"))??;
            let error = err
                .join()
                .map_err(|_| io::Error::other("Lookup worker stopped"))??;
            let message = if status.success() {
                output
            } else {
                format!("Command failed: {error}")
            };
            crate::prompt::message(term, "Lookup", &message)?;
            return Ok(());
        }
        term.draw(|f| {
            reader.draw(f, None);
            let area = centered(f.area(), 45, 3);
            f.render_widget(Clear, area);
            f.render_widget(
                Paragraph::new("Looking up selection… Esc Cancel")
                    .block(crate::appearance::block()),
                area,
            );
            crate::settings::paint_interface(f);
        })?;
        if start.elapsed().as_secs() >= 20
            || matches!(crate::settings::read_event_for(Some(std::time::Duration::from_millis(80)))?, Some(Event::Key(k)) if k.code == KeyCode::Esc)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        }
    }
}
