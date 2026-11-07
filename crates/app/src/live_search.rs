//! Editable, ranked results shared by the filesystem and BookOrbit catalogs.
use crate::{AppTerminal, prompt::LineInput};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    widgets::{Clear, Padding, Paragraph},
};
use std::{borrow::Cow, collections::HashMap, hash::Hash, io, time::Duration};
use unicode_truncate::UnicodeTruncateStr;

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
    fn detail(_action: &Self::Action) -> Option<Cow<'_, str>> {
        None
    }
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
    fn draw(&mut self, f: &mut Frame, popup: Rect) -> (Rect, Rect) {
        let panel = crate::appearance::block()
            .padding(Padding::horizontal(1))
            .title(format!(" {} ", self.title));
        let inner = panel.inner(popup);
        f.render_widget(Clear, popup);
        f.render_widget(panel, popup);
        let (x, y, w, h) = (inner.x, inner.y, inner.width, inner.height);
        if h < 3 || w == 0 {
            return (Rect::default(), Rect::default());
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
        let detail = self
            .hits
            .get(self.cursor)
            .filter(|_| h >= 6)
            .and_then(|id| S::detail(&self.items[*id].action));
        let rows = Rect::new(
            x,
            y + 2,
            w,
            h.saturating_sub(3 + u16::from(detail.is_some())),
        );
        if let Some(detail) = detail {
            let (tail, _) = detail.unicode_truncate_start(w.into());
            let text = if tail.len() == detail.len() {
                detail
            } else {
                let (tail, _) = detail.unicode_truncate_start(usize::from(w - 1));
                Cow::Owned(format!("…{tail}"))
            };
            f.render_widget(
                Paragraph::new(text).dim(),
                Rect::new(x, rows.bottom(), w, 1),
            );
        }
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
            let label = &self.items[id].label;
            let (shown, _) = label.unicode_truncate(w.into());
            let row = if shown.len() == label.len() {
                Paragraph::new(shown)
            } else {
                let (shown, _) = label.unicode_truncate(usize::from(w - 1));
                Paragraph::new(format!("{shown}…"))
            };
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
        let close = Rect::new(x, inner.bottom() - 1, w.min(10), 1);
        f.render_widget(
            Paragraph::new(if w >= 35 {
                "[Esc Back]   ↑↓ Select   Enter Open"
            } else {
                "[Esc Back]  ↑↓  Enter Open"
            })
            .cyan(),
            Rect::new(x, close.y, w, 1),
        );
        (rows, close)
    }
    pub fn choose(
        &mut self,
        term: &mut AppTerminal,
        mut shelf: impl FnMut(&mut Frame),
    ) -> io::Result<Option<S::Action>> {
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
            let mut popup = Rect::default();
            term.draw(|f| {
                shelf(f);
                let area = f.area();
                // Keep some shelf visible, even on a narrow terminal.
                let width = area.width.saturating_sub(area.width / 6).clamp(38, 80);
                let height = area.height.saturating_sub(area.height / 3).clamp(7, 16);
                popup =
                    crate::centered(area, width.min(area.width.saturating_sub(2).max(1)), height);
                crate::overlay::paint(f, popup, |f| {
                    (rows, close) = self.draw(f, popup);
                });
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
                        if !popup.contains((m.column, m.row).into())
                            || close.contains((m.column, m.row).into()) =>
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
                    MouseEventKind::ScrollUp if popup.contains((m.column, m.row).into()) => -3,
                    MouseEventKind::ScrollDown if popup.contains((m.column, m.row).into()) => 3,
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
