//! The offline source chooser and its contextual keyboard reference.
mod masthead;
use super::{Command, Shelf, Tab};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

fn source(cmd: &Command, connected: bool) -> (char, &'static str, &'static str) {
    match cmd {
        Command::Connect if connected => (
            'C',
            "Sign out of the current account and enter another server address or username.",
            "Enter Change connection   Esc Back",
        ),
        Command::Connect => (
            'B',
            "Sign in to a BookOrbit server to read your books and sync your position and bookmarks.",
            "0  Dashboard      /  Search",
        ),
        Command::OpenTab(Tab::Dashboard) => (
            'B',
            "Return to your account dashboard, current books and newest additions.",
            "0  Dashboard      /  Search",
        ),
        Command::Local => (
            'L',
            "Browse a folder on this device. Open an EPUB, FB2, PDF or DjVu and pick up where you left off.",
            "P  Choose folder      /  Search",
        ),
        Command::Opds => (
            'P',
            "Connect to an OPDS catalog, browse its feeds and choose an edition to read or download.",
            "Enter  Open feed      /  Search",
        ),
        Command::Downloads(crate::cache::Source::BookOrbit) => (
            'D',
            "Continue your BookOrbit books offline. Your reading position syncs when your account reconnects.",
            "Enter  Open book      /  Search",
        ),
        Command::Downloads(crate::cache::Source::Opds) => (
            'O',
            "Read books you saved from OPDS catalogs. Reading progress is stored on this device.",
            "Enter  Open book      /  Search",
        ),
        _ => ('\0', "", ""),
    }
}

impl Shelf {
    pub(super) fn home_shortcut(&self, key: char) -> Option<usize> {
        self.current().rows.iter().position(|row| {
            key.is_ascii_alphabetic()
                && key.eq_ignore_ascii_case(&source(&row.action, self.session.is_some()).0)
        })
    }

    pub(crate) fn focus_bookorbit(&mut self) {
        let bookorbit = self.home_shortcut('B');
        self.current_mut().selection.select(bookorbit);
    }

    pub(super) fn draw_home(&mut self, f: &mut Frame) {
        let screen = f.area();
        let frame = crate::appearance::block();
        let area = frame.inner(screen);
        f.render_widget(frame, screen);
        if area.is_empty() {
            return;
        }
        let sources = self.current().rows.len() as u16;
        // Both halves, the masthead and the footer belong to one reading-width panel.
        let canvas = crate::centered(area, area.width.min(92), 22 + sources);
        let ink = Style::new().fg(Color::Cyan);
        let row = |y, h| Rect::new(canvas.x, y, canvas.width, h);
        let masthead_h = masthead::draw(f, area, canvas);
        let illustrated = masthead::illustrated(canvas);
        let short = illustrated && canvas.height <= masthead_h + sources + 1;
        let mut y = canvas.y + masthead_h + u16::from(illustrated && !short);
        if canvas.height >= 10 && !illustrated {
            let tagline = if canvas.width >= 64 {
                "Your TUI book reader.   EPUB · FB2 · PDF · DjVu"
            } else {
                "Your TUI book reader."
            };
            f.render_widget(Paragraph::new(tagline).centered().dim(), row(y, 1));
            y += 1 + u16::from(canvas.height >= 16);
        }
        if illustrated && y + sources + 3 < canvas.bottom() {
            f.render_widget(
                Paragraph::new("─".repeat(canvas.width.into())).style(ink.dim()),
                row(y, 1),
            );
            y += 2;
        }
        let wide = canvas.width >= 70 && canvas.height >= 17;
        let footer = row(canvas.bottom().saturating_sub(1), 1);
        let left = if wide { 35 } else { canvas.width };
        let guide_x = if wide { canvas.x + left + 3 } else { canvas.x };
        if !short && y < footer.y {
            f.render_widget(
                Paragraph::new("Choose a library").bold(),
                Rect::new(canvas.x, y, left, 1),
            );
            if wide {
                f.render_widget(
                    Paragraph::new("Inside this library").bold(),
                    Rect::new(guide_x, y, canvas.right() - guide_x, 1),
                );
            }
            y += 1;
        }
        let notice_h = u16::from(self.notice.is_some() && footer.y.saturating_sub(y) > sources + 1);
        let bottom = footer.y.saturating_sub(notice_h);
        let room = bottom.saturating_sub(y);
        let stride = if wide && room >= sources * 2 { 2 } else { 1 };
        let slots = usize::from(room.div_ceil(stride)).min(usize::from(sources));
        let selected = self.current().selection.selected().unwrap_or(0);
        let first = (selected + 1).saturating_sub(slots);
        *self.current_mut().selection.offset_mut() = first;
        self.home_hits.clear();
        self.list_area = Rect::new(canvas.x, y, left, room.min(sources * stride));
        for i in first..first + slots {
            let item = &self.current().rows[i];
            let rect = Rect::new(canvas.x, y + (i - first) as u16 * stride, left, 1);
            let key = source(&item.action, self.session.is_some()).0;
            let active = selected == i;
            let mark = if active { "▸ " } else { "  " };
            let line = Line::from(vec![
                Span::styled(mark, ink),
                Span::styled(
                    format!(" {key} "),
                    if active {
                        ink.reversed().bold()
                    } else {
                        ink.dim()
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    item.label.as_str(),
                    if active {
                        Style::new().bold()
                    } else {
                        Style::new()
                    },
                ),
            ]);
            f.render_widget(Paragraph::new(line), rect);
            self.home_hits.push((rect, i));
        }
        let guide_y = if wide {
            y
        } else {
            y + slots as u16 * stride + u16::from(room > sources + 3)
        };
        if guide_y < bottom
            && let Some(action) = self.current().action()
        {
            let (_, description, keys) = source(&action, self.session.is_some());
            let w = canvas.right() - guide_x;
            // Reserve for the longest description and shortcut at this width, including
            // Change connection when signed in. Selection only changes the text.
            let wrapped = |text| Paragraph::new(text).wrap(Wrap { trim: true });
            let (description_rows, key_rows) = self
                .current()
                .rows
                .iter()
                .map(|r| source(&r.action, self.session.is_some()))
                .fold((0, 0), |(d, k), (_, desc, keys)| {
                    (
                        d.max(wrapped(desc).line_count(w) as u16),
                        k.max(wrapped(keys).line_count(w) as u16),
                    )
                });
            let room = bottom - guide_y;
            let key_h = key_rows.min(room.saturating_sub(1));
            let desc_h = description_rows.min(room - key_h);
            f.render_widget(wrapped(description), Rect::new(guide_x, guide_y, w, desc_h));
            if key_h > 0 {
                let gap = u16::from(wide && room > desc_h + key_h);
                let key_y = guide_y + desc_h + gap;
                f.render_widget(
                    wrapped(keys).style(ink),
                    Rect::new(guide_x, key_y, w, key_h),
                );
            }
        }
        if let Some(msg) = &self.notice
            && notice_h != 0
        {
            f.render_widget(Paragraph::new(msg.as_str()).yellow(), row(bottom, 1));
        }
        self.prefs_buttons.draw(f, footer);
        let mut x = footer.x + 23;
        self.home_controls.clear();
        for (label, command) in [
            ("↑↓ Choose", None),
            ("Enter Open", self.current().action()),
            ("Q Quit", Some(Command::Quit)),
        ] {
            let width = Line::from(label).width() as u16;
            if x.saturating_add(width) > footer.right() {
                break;
            }
            let rect = Rect::new(x, footer.y, width, 1);
            f.render_widget(Paragraph::new(label).dim(), rect);
            if let Some(c) = command {
                self.home_controls.push((rect, c));
            }
            x = rect.right().saturating_add(3);
        }
    }
}
