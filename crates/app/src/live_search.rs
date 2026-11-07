//! Editable, ranked results shared by the filesystem and BookOrbit catalogs.
use crate::{AppTerminal, prompt::LineInput};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{layout::Rect, style::Stylize, widgets::Paragraph};
use std::{collections::HashMap, hash::Hash, io, time::Duration};

mod matcher;
use matcher::{Words, fold, roman};

pub(crate) struct Item<K, V> {
    pub key: K,
    pub action: V,
    pub name: String,
    pub label: String,
    pub text: String,
}

struct RankedItem<V> {
    action: V,
    name: String,
    label: String,
    words: Words,
}

pub(crate) trait Source {
    type Key: Hash + Eq;
    type Action: Clone;
    fn collect(&mut self, batch: &mut Vec<Item<Self::Key, Self::Action>>) -> io::Result<()>;
    fn busy(&self) -> bool;
}

pub(crate) struct Search<S: Source> {
    source: S,
    title: &'static str,
    input: LineInput,
    items: Vec<RankedItem<S::Action>>,
    ids: HashMap<S::Key, usize>,
    hits: Vec<usize>,
    cursor: usize,
    first: usize,
    user_selected: bool,
    error: Option<String>,
}
impl<S: Source> Search<S> {
    pub fn new(source: S, title: &'static str) -> Self {
        Self {
            source,
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
    pub fn reset_query(&mut self) {
        self.input = LineInput::new("");
        self.user_selected = false;
        self.first = 0;
        self.rank();
    }
    pub fn failed(&self) -> bool {
        self.error.is_some()
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
            .filter_map(|(i, c)| Some((i, c.words.score(&words)?)))
            .collect::<Vec<_>>();
        ranked.sort_unstable_by(|(a, x), (b, y)| {
            y.cmp(x)
                .then_with(|| self.items[*a].name.cmp(&self.items[*b].name))
        });
        self.hits = ranked.into_iter().map(|(i, _)| i).collect();
        self.cursor = selected
            .and_then(|id| self.hits.iter().position(|i| *i == id))
            .unwrap_or(0);
    }
    pub fn choose(&mut self, term: &mut AppTerminal) -> io::Result<Option<S::Action>> {
        loop {
            let mut batch = vec![];
            if let Err(e) = self.source.collect(&mut batch) {
                self.error = Some(e.to_string());
            }
            if !batch.is_empty() {
                for item in batch {
                    let id = *self.ids.entry(item.key).or_insert(self.items.len());
                    let candidate = RankedItem {
                        action: item.action,
                        label: item.label,
                        name: item.name,
                        words: Words::new(&item.text),
                    };
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
                let status = if self.source.busy() {
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
                    let row = Paragraph::new(self.items[id].label.as_str());
                    f.render_widget(
                        if i == self.cursor {
                            row.reversed()
                        } else {
                            row
                        },
                        Rect::new(x, rows.y + (i - self.first) as u16, w, 1),
                    );
                }
                if self.hits.is_empty() && !self.source.busy() && self.error.is_none() {
                    f.render_widget(Paragraph::new(" No matches.").dim(), rows);
                }
                close = Rect::new(x, inner.bottom() - 1, w.min(10), 1);
                f.render_widget(
                    Paragraph::new("[Esc Back]   ↑↓ Select   Enter Open").cyan(),
                    Rect::new(x, close.y, w, 1),
                );
                crate::settings::paint_interface(f);
            })?;
            let Some(e) = crate::settings::read_event_for(
                self.source.busy().then_some(Duration::from_millis(40)),
            )?
            else {
                continue;
            };
            let open = || {
                self.hits
                    .get(self.cursor)
                    .map(|id| self.items[*id].action.clone())
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
                            return Ok(Some(self.items[*id].action.clone()));
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
