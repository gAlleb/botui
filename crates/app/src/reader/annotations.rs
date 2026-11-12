use super::{Reader, study::Command};
use crate::{AppTerminal, centered, prompt::LineInput};
use botui_store::{HighlightColor, LocalState, Note};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint, Position, Rect},
    style::Modifier,
    widgets::{Clear, Paragraph, Row, Table, TableState, Wrap},
};
use std::io;

pub(super) fn run(
    term: &mut AppTerminal,
    reader: &mut Reader,
    store: Option<&LocalState>,
    command: Command,
) -> io::Result<()> {
    let store = || store.ok_or_else(|| io::Error::other("Local storage is unavailable"));
    match command {
        Command::ContextMenu => {
            use crate::text_menu::Action as M;
            let Some(action) = crate::text_menu::choose(term)? else {
                return Ok(());
            };
            if matches!(action, M::Search) {
                let range = reader
                    .study
                    .range(&reader.doc, reader.pager.anchor())
                    .ok_or_else(|| io::Error::other("Select some text first"))?;
                let query = reader.doc.text_range(range);
                reader.apply(super::Action::Navigate(super::navigation::Kind::Search));
                if let Some(nav) = reader.nav.as_mut() {
                    nav.paste(&query);
                }
            } else {
                let command = match action {
                    M::Copy => Command::Copy,
                    M::Highlight => Command::Highlight,
                    M::Translate => Command::Translate,
                    M::Define => Command::Lookup,
                    M::AddNote => Command::Comment,
                    M::Search => unreachable!(),
                };
                run(term, reader, store().ok(), command)?;
            }
        }
        Command::Normal => {
            reader.study.toggle_normal(reader.pager.anchor());
        }
        Command::Clear => reader.study.escape(),
        Command::Copy => {
            let range = reader
                .study
                .range(&reader.doc, reader.pager.anchor())
                .ok_or_else(|| io::Error::other("Select some text first"))?;
            reader.study.message =
                Some(super::clipboard::copy(&reader.doc.text_range(range))?.into());
            reader.study.copied();
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
        Command::Lookup | Command::Translate => {
            let at = reader.study.cursor.unwrap_or(reader.pager.anchor());
            let range = reader
                .study
                .selection
                .or_else(|| reader.doc.word_at(at))
                .ok_or_else(|| io::Error::other("Select a word or passage first"))?;
            let quote = reader.doc.text_range(range);
            let purpose = if command == Command::Translate {
                crate::lookup::Purpose::Translation
            } else {
                crate::lookup::Purpose::Dictionary
            };
            if let Some(body) =
                crate::lookup::show(term, &quote, purpose, |f| reader.draw(f, None))?
            {
                reader.study.notes = store()?.annotate(None, range, quote, None, body)?;
                reader.study.message = Some("Saved as a passage note".into());
            }
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
    let Some(edit) = crate::annotation_editor::edit(
        term,
        |f| reader.draw(f, None),
        &quote,
        note.map_or("", |n| n.body.as_str()),
        note.map_or(Some(HighlightColor::Yellow), |n| n.color),
        color_only,
    )?
    else {
        return Ok(());
    };
    let message = if edit.remove {
        if let Some(note) = note {
            reader.study.notes = store.delete_note(&note.id)?;
            "Annotation deleted"
        } else {
            "Empty note discarded"
        }
    } else {
        reader.study.notes = store.annotate(
            note.map(|n| n.id.as_str()),
            range,
            quote,
            edit.color,
            edit.body,
        )?;
        "Annotation saved on this device"
    };
    reader.study.finish_annotation();
    reader.study.cursor = Some(range.start);
    reader.study.message = Some(message.into());
    Ok(())
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
            crate::settings::paint_dialog(f);
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
