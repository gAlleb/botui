//! Live search keeps one recursive scan alive while the query changes.
use super::index::Index;
use crate::{AppTerminal, prompt::LineInput};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{layout::Rect, style::Stylize, widgets::Paragraph};
use std::{collections::HashMap, io, path::PathBuf, time::Duration};

mod matcher;
use matcher::{Candidate, fold, roman};

pub(super) struct Search {
    index: Index,
    title: &'static str,
    input: LineInput,
    items: Vec<Candidate>,
    ids: HashMap<PathBuf, usize>,
    hits: Vec<usize>,
    cursor: usize,
    first: usize,
    user_selected: bool,
    error: Option<String>,
}
impl Search {
    pub fn new(index: Index, title: &'static str) -> Self {
        Self {
            index,
            title,
            input: LineInput::new(""),
            items: vec![],
            ids: HashMap::new(),
            hits: vec![],
            cursor: 0,
            first: 0,
            user_selected: false,
            error: None,
        }
    }
    fn rank(&mut self) {
        let selected = self
            .user_selected
            .then(|| self.hits.get(self.cursor).copied())
            .flatten();
        let words = fold(self.input.value())
            .split_whitespace()
            .map(|w| (w.to_owned(), roman(w)))
            .collect::<Vec<_>>();
        let mut ranked = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Some((i, c.score(&words)?)))
            .collect::<Vec<_>>();
        ranked.sort_unstable_by(|(a, x), (b, y)| {
            y.cmp(x)
                .then_with(|| self.items[*a].entry.name.cmp(&self.items[*b].entry.name))
        });
        self.hits = ranked.into_iter().map(|(i, _)| i).collect();
        self.cursor = selected
            .and_then(|id| self.hits.iter().position(|i| *i == id))
            .unwrap_or(0);
    }
    pub fn choose(&mut self, term: &mut AppTerminal) -> io::Result<Option<PathBuf>> {
        loop {
            let mut batch = vec![];
            if let Some(Err(e)) = self.index.collect(&mut batch) {
                self.error = Some(e.to_string());
            }
            if !batch.is_empty() {
                for entry in batch {
                    let id = *self
                        .ids
                        .entry(entry.path.clone())
                        .or_insert(self.items.len());
                    let candidate = Candidate::new(entry);
                    if id == self.items.len() {
                        self.items.push(candidate)
                    } else {
                        self.items[id] = candidate
                    }
                }
                self.rank();
            }
            let mut rows = Rect::default();
            let mut close = Rect::default();
            term.draw(|f| {
                let panel = crate::appearance::block().title(format!(" {} ", self.title));
                let inner = panel.inner(f.area());
                f.render_widget(panel, f.area());
                let (x, y, w, h) = (inner.x, inner.y, inner.width, inner.height);
                if h < 3 || w == 0 {
                    return;
                }
                f.render_widget(
                    Paragraph::new(format!(
                        " / {}",
                        self.input.display(w.saturating_sub(3), false)
                    ))
                    .cyan(),
                    Rect::new(x, y, w, 1),
                );
                let status = if self.index.busy() {
                    " · indexing…"
                } else {
                    ""
                };
                f.render_widget(
                    Paragraph::new(self.error.clone().unwrap_or_else(|| {
                        format!(
                            " {} result{}{status}",
                            self.hits.len(),
                            if self.hits.len() == 1 { "" } else { "s" }
                        )
                    }))
                    .dim(),
                    Rect::new(x, y + 1, w, 1),
                );
                rows = Rect::new(x, y + 2, w, h.saturating_sub(3));
                self.first = self
                    .first
                    .min(self.cursor)
                    .max((self.cursor + 1).saturating_sub(usize::from(rows.height)))
                    .min(self.hits.len().saturating_sub(rows.height.into()));
                for (i, &id) in self
                    .hits
                    .iter()
                    .enumerate()
                    .skip(self.first)
                    .take(rows.height.into())
                {
                    let e = &self.items[id].entry;
                    let mut line = e
                        .meta
                        .as_ref()
                        .filter(|m| !m.title.is_empty())
                        .map_or(String::new(), |m| {
                            format!("{} · {}  —  ", m.title, m.authors.join(", "))
                        });
                    line.push_str(&e.name);
                    let row = Paragraph::new(line);
                    f.render_widget(
                        if i == self.cursor {
                            row.reversed()
                        } else {
                            row
                        },
                        Rect::new(x, rows.y + (i - self.first) as u16, w, 1),
                    );
                }
                if self.hits.is_empty() && !self.index.busy() {
                    f.render_widget(Paragraph::new(" No matching books.").dim(), rows);
                }
                close = Rect::new(x, inner.bottom() - 1, w.min(10), 1);
                f.render_widget(
                    Paragraph::new("[Esc Back]   ↑↓ Select   Enter Open").cyan(),
                    Rect::new(x, close.y, w, 1),
                );
                crate::settings::paint_interface(f);
            })?;
            let Some(e) = crate::settings::read_event_for(
                self.index.busy().then_some(Duration::from_millis(40)),
            )?
            else {
                continue;
            };
            let open = || {
                self.hits
                    .get(self.cursor)
                    .map(|id| self.items[*id].entry.path.clone())
            };
            let delta = match &e {
                Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Enter => {
                        if let Some(p) = open() {
                            self.user_selected = true;
                            return Ok(Some(p));
                        }
                        continue;
                    }
                    KeyCode::Up => -1,
                    KeyCode::Down => 1,
                    KeyCode::PageDown => i32::from(rows.height),
                    KeyCode::PageUp => -i32::from(rows.height),
                    _ => 0,
                },
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::Down(MouseButton::Left)
                        if close.contains((m.column, m.row).into()) =>
                    {
                        return Ok(None);
                    }
                    MouseEventKind::Down(MouseButton::Left)
                        if rows.contains((m.column, m.row).into()) =>
                    {
                        let i = self.first + usize::from(m.row - rows.y);
                        if let Some(id) = self.hits.get(i) {
                            self.cursor = i;
                            self.user_selected = true;
                            return Ok(Some(self.items[*id].entry.path.clone()));
                        }
                        continue;
                    }
                    MouseEventKind::ScrollUp => -3,
                    MouseEventKind::ScrollDown => 3,
                    _ => 0,
                },
                _ => 0,
            };
            if delta != 0 {
                self.user_selected = true;
                self.cursor = self
                    .cursor
                    .saturating_add_signed(delta as isize)
                    .min(self.hits.len().saturating_sub(1));
            } else {
                let old = self.input.value().to_owned();
                self.input.edit(&e);
                if old != self.input.value() {
                    self.user_selected = false;
                    self.first = 0;
                    self.rank();
                }
            }
        }
    }
}
