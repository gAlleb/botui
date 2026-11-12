use super::*;
use crate::AppTerminal;
use ratatui::{
    buffer::Buffer,
    layout::Constraint,
    widgets::{Clear, Row, Table, TableState, Wrap},
};

pub(super) fn background(f: &mut Frame, saved: &Buffer) {
    let r = f.area().intersection(saved.area);
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            f.buffer_mut()[(x, y)] = saved[(x, y)].clone();
        }
    }
}
impl Text {
    pub fn run(
        &mut self,
        action: Action,
        term: &mut AppTerminal,
        store: Option<&LocalState>,
        saved: &Buffer,
    ) -> io::Result<()> {
        match action {
            Action::None | Action::Key(_) => (),
            Action::Lookup(purpose) => {
                let (range, quote, regions) = self.selection().ok_or_else(|| {
                    io::Error::other("Enable the text cursor with Space c, or select a passage")
                })?;
                if let Some(body) =
                    crate::lookup::show(term, &quote, purpose, |f| background(f, saved))?
                {
                    self.notes = store
                        .ok_or_else(|| io::Error::other("Local storage unavailable"))?
                        .annotate_regions(None, range, quote, None, body, regions)?;
                    self.message = "Saved as a passage note".into();
                }
            }
            Action::Mark(c) => {
                let (range, _, _) = self
                    .selection()
                    .ok_or_else(|| io::Error::other("No PDF text selected"))?;
                store
                    .ok_or_else(|| io::Error::other("Storage unavailable"))?
                    .set_named_mark(c, range.start)?;
                self.message = format!("Mark {c} saved");
            }
            Action::Jump(c) => {
                let at = store
                    .ok_or_else(|| io::Error::other("Storage unavailable"))?
                    .named_marks()?
                    .get(&c)
                    .copied()
                    .ok_or_else(|| io::Error::other("Mark not set"))?;
                let page = at.block as u32 + 1;
                let bounds = self
                    .cache
                    .get(&page)
                    .and_then(|r| r.as_ref().ok())
                    .and_then(|t| t.words.iter().find(|w| w.bytes.contains(&at.byte)))
                    .map_or([0., 0., 1., 0.01], |w| w.bounds);
                self.reveal = Some(PdfRegion { page, bounds });
            }

            Action::Clear => self.escape(),
            Action::Copy => {
                let (_, quote, _) = self
                    .selection()
                    .ok_or_else(|| io::Error::other("Select some PDF text first"))?;
                self.message = crate::reader::clipboard::copy(&quote)?.into();
                if let (Some(a), Some(b)) = (self.start.take(), self.cursor) {
                    self.previous = Some((a, b));
                }
                if let Some(origin) = self.yank_origin.take() {
                    self.cursor = Some(origin);
                }
            }
            Action::Comment | Action::Highlight => {
                let store =
                    store.ok_or_else(|| io::Error::other("Local storage is unavailable"))?;
                let note = self.active_note().cloned();
                self.edit(
                    term,
                    store,
                    saved,
                    note.as_ref(),
                    matches!(action, Action::Highlight),
                )?;
            }
        }
        Ok(())
    }
    fn edit(
        &mut self,
        term: &mut AppTerminal,
        store: &LocalState,
        saved: &Buffer,
        note: Option<&Note>,
        color_only: bool,
    ) -> io::Result<()> {
        let (range, quote, regions) = if let Some(note) = note.filter(|n| !n.pdf_regions.is_empty())
        {
            (
                note.range()
                    .ok_or_else(|| io::Error::other("Invalid PDF annotation range"))?,
                note.quote.clone(),
                note.pdf_regions.clone(),
            )
        } else {
            self.selection()
                .ok_or_else(|| io::Error::other("Select some PDF text first"))?
        };
        let Some(edit) = crate::annotation_editor::edit(
            term,
            |f| background(f, saved),
            &quote,
            note.map_or("", |n| n.body.as_str()),
            note.map_or(Some(HighlightColor::Yellow), |n| n.color),
            color_only,
        )?
        else {
            return Ok(());
        };
        if edit.remove {
            if let Some(note) = note {
                self.notes = store.delete_note(&note.id)?;
                self.message = "Annotation deleted".into();
            } else {
                self.message = "Empty note discarded".into();
            }
        } else {
            self.notes = store.annotate_regions(
                note.map(|n| n.id.as_str()),
                range,
                quote,
                edit.color,
                edit.body,
                regions,
            )?;
            self.message = "Annotation saved on this device".into();
        }
        self.start = None;
        Ok(())
    }
    pub fn list(
        &mut self,
        term: &mut AppTerminal,
        store: &LocalState,
        path: &Path,
        saved: &Buffer,
    ) -> io::Result<Option<PdfRegion>> {
        self.notes = store.notes()?;
        self.message.clear();
        let mut query = crate::prompt::LineInput::new("");
        let mut filtering = false;
        let mut selected = 0usize;
        let mut table = TableState::default();
        loop {
            let needle = query.value().to_lowercase();
            let notes: Vec<_> = self
                .notes
                .iter()
                .filter(|n| {
                    format!("{} {}", n.quote, n.body)
                        .to_lowercase()
                        .contains(&needle)
                })
                .cloned()
                .collect();
            selected = selected.min(notes.len().saturating_sub(1));
            table.select((!notes.is_empty()).then_some(selected));
            let mut buttons = Vec::new();
            let mut rows = Rect::default();
            term.draw(|f| {
                background(f, saved);
                let area = crate::centered(f.area(), 88, 26);
                crate::overlay::paint(f, area, |f| {
                    let border = crate::appearance::block()
                        .title(format!(" PDF highlights & notes · {} ", notes.len()));
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
                    let (rects, h) = crate::reader::controls::layout(inner, &actions.map(|a| a.0));
                    for ((label, key), r) in actions.into_iter().zip(rects) {
                        f.render_widget(Paragraph::new(label), r);
                        buttons.push((r, key));
                    }
                    f.render_widget(
                        Paragraph::new(format!("Find: {}", query.value())),
                        Rect::new(inner.x, inner.y, inner.width, 1),
                    );
                    let height = inner.height.saturating_sub(h + 3);
                    f.render_widget(
                        Paragraph::new(self.message.as_str()),
                        Rect::new(
                            inner.x,
                            inner.bottom().saturating_sub(h + 1),
                            inner.width,
                            1,
                        ),
                    );
                    rows = Rect::new(inner.x, inner.y + 1, inner.width, height / 2);
                    let values = notes.iter().map(|n| {
                        Row::new([
                            format!("{}", n.anchor().block + 1),
                            if n.quote.is_empty() {
                                n.body.replace('\n', " ")
                            } else {
                                n.quote.replace('\n', " ")
                            },
                        ])
                    });
                    f.render_stateful_widget(
                        Table::new(values, [Constraint::Length(6), Constraint::Min(1)])
                            .row_highlight_style(ratatui::style::Style::new().reversed()),
                        rows,
                        &mut table,
                    );
                    let preview = notes
                        .get(selected)
                        .map(|n| format!("{}\n\n{}", n.quote, n.body))
                        .unwrap_or_else(|| "No annotations".into());
                    f.render_widget(
                        Paragraph::new(preview).wrap(Wrap { trim: false }),
                        Rect::new(
                            inner.x,
                            rows.bottom() + 1,
                            inner.width,
                            height.saturating_sub(rows.height + 1),
                        ),
                    );
                });
                crate::settings::paint_interface(f);
            })?;
            let mut e = crate::settings::read_event()?;
            if let Event::Mouse(m) = e {
                let pos = Position::new(m.column, m.row);
                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    if let Some((_, key)) = buttons.iter().find(|(r, _)| r.contains(pos)) {
                        e = Event::Key(crossterm::event::KeyEvent::new(*key, KeyModifiers::NONE));
                    } else if rows.contains(pos) {
                        selected = (table.offset() + usize::from(pos.y - rows.y))
                            .min(notes.len().saturating_sub(1));
                        continue;
                    }
                } else if m.kind == MouseEventKind::ScrollDown {
                    selected = (selected + 1).min(notes.len().saturating_sub(1));
                    continue;
                } else if m.kind == MouseEventKind::ScrollUp {
                    selected = selected.saturating_sub(1);
                    continue;
                }
            }
            if filtering && matches!(e, Event::Paste(_)) {
                query.edit(&e);
                selected = 0;
                continue;
            }
            let Event::Key(k) = e else {
                continue;
            };
            if k.kind == KeyEventKind::Release {
                continue;
            }
            if filtering {
                if matches!(k.code, KeyCode::Enter | KeyCode::Esc) {
                    filtering = false;
                } else {
                    query.edit(&e);
                    selected = 0;
                }
                continue;
            }
            let note = notes.get(selected);
            match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    selected = (selected + 1).min(notes.len().saturating_sub(1))
                }
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = notes.len().saturating_sub(1),
                KeyCode::PageUp => selected = selected.saturating_sub(rows.height as usize),
                KeyCode::PageDown => selected = selected.saturating_add(rows.height as usize),
                KeyCode::Char('/') => filtering = true,
                KeyCode::Enter => {
                    if let Some(n) = note {
                        return Ok(Some(n.pdf_regions.first().cloned().unwrap_or(PdfRegion {
                            page: n.anchor().block as u32 + 1,
                            bounds: [0., 0., 1., 0.01],
                        })));
                    }
                }
                KeyCode::Char('n' | 'N' | 'h' | 'H') => {
                    if let Some(n) = note {
                        if n.pdf_regions.is_empty() {
                            if let Some(body) = crate::prompt::edit(
                                term,
                                "Page note · clear to remove",
                                "[ Save ↵ ]",
                                &n.body,
                                |_| {},
                            )? {
                                self.notes = store.edit_note(n.anchor(), body)?;
                            }
                        } else {
                            self.edit(
                                term,
                                store,
                                saved,
                                Some(n),
                                matches!(k.code, KeyCode::Char('h' | 'H')),
                            )?;
                        }
                    }
                }
                KeyCode::Delete => {
                    if let Some(n) = note
                        && crate::prompt::choose(
                            term,
                            "Delete annotation?",
                            &["Cancel".into(), "Delete".into()],
                            0,
                        )? == Some(1)
                    {
                        self.notes = store.delete_note(&n.id)?;
                    }
                }
                KeyCode::Char('y' | 'Y') => {
                    if let Some(n) = note {
                        self.message = crate::reader::clipboard::copy(&n.quote)?.into();
                    }
                }
                KeyCode::Char('e' | 'E') => {
                    let output = crate::reader::notes::export(
                        &path.file_stem().unwrap_or_default().to_string_lossy(),
                        None,
                        &notes,
                    )?;
                    self.message = format!("Saved {}", output.display());
                }
                _ => (),
            }
        }
    }
}
