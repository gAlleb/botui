//! A file can be opened while metadata is loading. Only explicit actions change server state.
use super::{
    Book, Command, Section, Session, Shelf,
    background::{Ticket, Worker},
};
use botui_bookorbit::detail::Detail;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Style, Stylize},
    widgets::{List, ListItem, Paragraph, Wrap},
};
use std::{
    io,
    sync::{Arc, mpsc::TryRecvError},
};
#[derive(Default)]
pub(super) struct Card {
    pub id: u64,
    pub data: Option<Detail>,
    job: Option<Ticket<botui_bookorbit::Result<Detail>>>,
    error: Option<String>,
    hits: Vec<(Rect, Act)>,
}
#[derive(Clone, Copy)]
enum Act {
    Read,
    Save,
    Hold,
    Reset,
    More,
}
pub(super) fn status_label(s: &str) -> &str {
    match s {
        "unread" => "Unread",
        "reading" => "Reading",
        "rereading" => "Rereading",
        "read" => "Read",
        "on_hold" => "On hold",
        "want_to_read" => "Want to read",
        "abandoned" => "Abandoned",
        "skimmed" => "Skimmed",
        _ => s,
    }
}
impl Card {
    pub fn open(id: u64, session: Option<Arc<Session>>) -> Self {
        Self {
            id,
            job: session
                .map(|s| Worker::new().start(move |_| s.with_token(|a, t| a.detail(t, id)))),
            ..Default::default()
        }
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn collect(&mut self) {
        if let Some(t) = &self.job {
            match t.receiver.try_recv() {
                Ok(Ok(d)) => self.data = Some(d),
                Ok(Err(_)) | Err(TryRecvError::Disconnected) => {
                    self.error =
                        Some("BookOrbit details unavailable · editions can still be opened.".into())
                }
                Err(TryRecvError::Empty) => return,
            }
            self.job = None;
        }
    }
}
impl Shelf {
    pub(super) fn draw_card(&mut self, f: &mut Frame, inner: Rect, book: &Book) {
        self.card.hits.clear();
        if inner.is_empty() {
            return;
        }
        let area = Rect::new(
            inner.x + u16::from(inner.width > 20),
            inner.y,
            inner
                .width
                .saturating_sub(if inner.width > 20 { 2 } else { 0 }),
            inner.height,
        );
        let wide = area.width >= 65 && area.height >= 14;
        let mut body = area;
        if wide {
            let w = (area.width / 5).clamp(15, 24);
            self.artwork(
                f,
                Rect::new(area.x, area.y + 1, w, area.height.saturating_sub(4).min(18)),
                book,
            );
            body.x += w + 3;
            body.width -= w + 3;
        }
        let d = self.card.data.as_ref();
        let line = |row: u16| Rect::new(body.x, row, body.width, 1);
        let footer = area.bottom().saturating_sub(1);
        let mut y = body.y;
        let title = d
            .and_then(|v| v.title.as_deref())
            .or(book.title.as_deref())
            .unwrap_or("Untitled");
        let title = Paragraph::new(super::single_line(title.into()))
            .bold()
            .wrap(Wrap { trim: true });
        let h = (title.line_count(body.width) as u16)
            .clamp(1, 2)
            .min(body.height);
        f.render_widget(title, Rect::new(body.x, y, body.width, h));
        y += h;
        let by = d.filter(|v| !v.authors.is_empty()).map_or_else(
            || book.authors.join(", "),
            |v| {
                v.authors
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        );
        let mut by = vec![by];
        if let Some(v) = d
            && let Some(series) = &v.series_name
        {
            let idx = v.series_index.as_ref().and_then(|x| {
                if let Some(s) = x.as_str() {
                    Some(s.to_owned())
                } else if x.is_number() {
                    Some(x.to_string())
                } else {
                    None
                }
            });
            by.push(format!(
                "{}{}",
                series,
                idx.map_or(String::new(), |i| format!(" #{i}"))
            ));
        }
        if y < footer {
            f.render_widget(
                Paragraph::new(super::single_line(
                    by.into_iter()
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<_>>()
                        .join("  ·  "),
                ))
                .cyan(),
                line(y),
            );
            y += 1;
        }
        let status = d
            .and_then(|v| v.read_status.as_ref())
            .map(|s| status_label(&s.status));
        if y < footer && (d.is_some() || book.progress.is_some()) {
            let s = [
                status.or(d.map(|_| "Unread")).map(str::to_owned),
                book.progress.map(|p| format!("{p}% read")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("  ·  ");
            f.render_widget(Paragraph::new(s), line(y));
            y += 1;
        }
        // Anchor editions before the network reply: a click on a file must never move to a different control.
        let files_y = (body.y + 18)
            .min(footer.saturating_sub((book.files.len() as u16).clamp(1, 5) + 1))
            .max(body.y);
        let actions_y = files_y
            .saturating_sub(if body.width >= 87 {
                2
            } else if body.width >= 45 {
                3
            } else {
                4
            })
            .max(body.y);
        let reserve = footer.saturating_sub(actions_y);
        if let Some(v) = d
            && footer.saturating_sub(y) > reserve
        {
            let mut facts = vec![];
            if let Some(t) = &v.publisher {
                facts.push(t.clone());
            }
            if let Some(t) = &v.published_date {
                facts.push(t.clone());
            } else if let Some(t) = v.published_year {
                facts.push(t.to_string());
            }
            if let Some(t) = &v.language {
                facts.push(t.to_uppercase());
            }
            if let Some(n) = v.page_count {
                facts.push(format!("{n} pages"));
            }
            if let Some(t) = v.isbn13.as_ref().or(v.isbn10.as_ref()) {
                facts.push(format!("ISBN {t}"));
            }
            for label in [
                facts.join("  ·  "),
                [
                    v.library_name.as_deref().map(|t| format!("Library: {t}")),
                    v.added_at
                        .as_deref()
                        .map(|t| format!("Added: {}", t.split('T').next().unwrap_or(t))),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("  ·  "),
                v.genres
                    .iter()
                    .chain(&v.tags)
                    .take(10)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" · "),
            ] {
                if !label.is_empty() && footer.saturating_sub(y) > reserve {
                    f.render_widget(Paragraph::new(super::single_line(label)).dim(), line(y));
                    y += 1;
                }
            }
            if let Some(s) = &v.description
                && footer.saturating_sub(y) > reserve + 2
            {
                y += 1;
                f.render_widget(Paragraph::new("Synopsis").bold(), line(y));
                y += 1;
                let p = Paragraph::new(botui_formats::synopsis(s)).wrap(Wrap { trim: true });
                let h = (p.line_count(body.width) as u16)
                    .min(6)
                    .min(footer.saturating_sub(y + reserve));
                f.render_widget(p, Rect::new(body.x, y, body.width, h));
            }
        } else if footer.saturating_sub(y) > reserve
            && let Some(msg) = self
                .card
                .error
                .as_deref()
                .or_else(|| self.card.busy().then_some("Loading book details…"))
        {
            f.render_widget(Paragraph::new(msg).dim(), line(y));
        }
        y = actions_y;
        let has_files = !book.files.is_empty();
        let mut x = body.x;
        let mut controls = vec![];
        if has_files {
            controls.extend([("[R Read]", Act::Read), ("[D Save offline]", Act::Save)]);
        }
        if matches!(status, Some("Reading" | "Rereading" | "On hold")) {
            controls.push((
                if status == Some("On hold") {
                    "[H Resume reading]"
                } else {
                    "[H Put on hold]"
                },
                Act::Hold,
            ));
        }
        if d.is_some() {
            controls.push(("[X Reset reading state]", Act::Reset));
        }
        if d.and_then(|v| v.description.as_ref()).is_some() {
            controls.push(("[I Synopsis]", Act::More));
        }
        for (label, act) in controls {
            let w = label.len() as u16;
            if x + w > body.right() {
                x = body.x;
                y += 1;
            }
            if y >= files_y.saturating_sub(1) || w > body.width {
                break;
            }
            let r = Rect::new(x, y, w, 1);
            f.render_widget(Paragraph::new(label).cyan(), r);
            self.card.hits.push((r, act));
            x += w + 1;
        }
        y = files_y;
        if y > body.y {
            f.render_widget(Paragraph::new("Editions").bold(), line(y - 1));
        }
        self.list_area = Rect::new(
            body.x,
            y.min(footer),
            body.width,
            footer
                .saturating_sub(y)
                .saturating_sub(u16::from(self.notice.is_some())),
        );
        let list = self.list_area;
        let pane = self.current_mut();
        let items = pane
            .rows
            .iter()
            .map(|r| ListItem::new(format!(" {}", r.label)));
        f.render_stateful_widget(
            List::new(items).highlight_style(Style::new().reversed()),
            list,
            &mut pane.selection,
        );
        if pane.rows.is_empty() {
            f.render_widget(Paragraph::new("No editions available"), list);
        }
        if let Some(msg) = &self.notice
            && footer > body.y
        {
            f.render_widget(Paragraph::new(msg.as_str()).yellow(), line(footer - 1));
        }
        self.draw_buttons(f, inner, "[ Back ]", true);
    }

    pub(super) fn card_input(
        &mut self,
        term: &mut crate::AppTerminal,
        e: &Event,
    ) -> io::Result<bool> {
        let action = match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Char('r' | 'R') => Some(Act::Read),
                KeyCode::Char('h' | 'H') => Some(Act::Hold),
                KeyCode::Char('x' | 'X') => Some(Act::Reset),
                KeyCode::Char('i' | 'I') => Some(Act::More),
                _ => None,
            },
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => self
                .card
                .hits
                .iter()
                .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
                .map(|(_, a)| *a),
            _ => None,
        };
        let Some(act) = action else { return Ok(false) };
        if matches!(act, Act::Read | Act::Save) {
            if let Some(Command::File(file)) = self.current().action() {
                if matches!(act, Act::Save) {
                    self.transfer(file, true);
                } else if let Some(kind) = file
                    .format
                    .as_deref()
                    .and_then(botui_formats::FileKind::from_extension)
                    && !kind.readable()
                {
                    self.download(file);
                } else {
                    self.open_file(file);
                }
            }
            return Ok(true);
        }
        let Some(d) = self.card.data.as_ref() else {
            return Ok(true);
        };
        if matches!(act, Act::More) {
            if let Some(s) = &d.description {
                crate::prompt::message(term, "Synopsis", &botui_formats::synopsis(s))?;
            }
            return Ok(true);
        }
        let Some(session) = self.session.clone() else {
            return Ok(true);
        };
        let Section::Files(book) = &self.current().section else {
            return Ok(true);
        };
        let id = book.id;
        let files = book.files.iter().map(|f| f.id).collect::<Vec<_>>();
        let account = crate::offline::account(&self.server, &self.username);
        let reset = matches!(act, Act::Reset);
        let status = d.read_status.as_ref().map(|v| v.status.as_str());
        let resume = status == Some("on_hold");
        if !reset && !matches!(status, Some("reading" | "rereading" | "on_hold")) {
            return Ok(true);
        }
        if reset
            && !crate::prompt::confirm_reset(term, book.title.as_deref().unwrap_or("this book"))?
        {
            return Ok(true);
        }
        self.fetch("Updating reading state…", move |_| {
            let updated = session.with_token(|a, t| {
                if reset {
                    a.reset_reading(t, id)
                } else {
                    a.hold(t, id, resume)
                }
            })?;
            let warning = if reset {
                crate::offline::reset_positions(&account, &files)
                    .err()
                    .map(|e| format!("Server reset; offline copy: {e}"))
            } else {
                None
            };
            Ok(super::Loaded::Status(id, updated, reset, warning))
        });
        if let Some(p) = &mut self.pending {
            p.saving = true;
        }
        Ok(true)
    }
}
