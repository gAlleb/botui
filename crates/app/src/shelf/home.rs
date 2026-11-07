//! The offline source chooser and its contextual keyboard reference.
use super::{Command, Shelf, Tab};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

const LOGO: [&str; 7] = [
    " ▄▄▄▄▄▄▄   ▄▄▄▄▄▄▄",
    " █     ▀▄ ▄▀     █     ██████╗   ██████╗  ████████╗ ██╗   ██╗ ██╗",
    " █ ▀▀▀  █ █  ▀▀▀ █     ██╔══██╗ ██╔═══██╗ ╚══██╔══╝ ██║   ██║ ██║",
    " █ ▀▀   █ █   ▀▀ █     ██████╔╝ ██║   ██║    ██║    ██║   ██║ ██║",
    " █ ▀▀▀  █ █  ▀▀▀ █     ██╔══██╗ ██║   ██║    ██║    ██║   ██║ ██║",
    " █     ▄█ █▄     █     ██████╔╝ ╚██████╔╝    ██║    ╚██████╔╝ ██║",
    " ▀▀▀▀▀▀  ▀  ▀▀▀▀▀▀     ╚═════╝   ╚═════╝     ╚═╝     ╚═════╝  ╚═╝",
];
fn source(cmd: &Command, connected: bool) -> (&'static str, &'static str, &'static str) {
    match cmd {
        Command::Connect if connected => (
            "C",
            "Sign out of the current account and enter another server address or username.",
            "Enter Change connection   Esc Back",
        ),
        Command::Connect => (
            "B",
            "Sign in to a BookOrbit server to read your books and sync your position and bookmarks.",
            "0 Dashboard   / Search   6 Authors   7 Series",
        ),
        Command::OpenTab(Tab::Dashboard) => (
            "B",
            "Return to your account dashboard, current books and newest additions.",
            "0 Dashboard   / Search   6 Authors   7 Series",
        ),
        Command::Local => (
            "L",
            "Browse a folder on this device. Open an EPUB, FB2, PDF or DjVu and pick up where you left off.",
            "P Choose folder   / Search folders   V Grid or list",
        ),
        Command::Opds => (
            "P",
            "Connect to an OPDS catalog, browse its feeds and choose an edition to read or download.",
            "Enter Open feed   / Catalog search   Esc Back",
        ),
        Command::Downloads => (
            "D",
            "Read editions saved on this device. Your downloads are available without a server connection.",
            "Enter Open   / Search downloads   Q Back",
        ),
        _ => ("", "", ""),
    }
}

impl Shelf {
    pub(super) fn draw_home(&mut self, f: &mut Frame) {
        let screen = f.area();
        let frame = crate::appearance::block();
        let area = frame.inner(screen);
        f.render_widget(frame, screen);
        if area.is_empty() {
            return;
        }
        let canvas = crate::centered(
            area,
            area.width
                .saturating_sub(u16::from(area.width > 32) * 4)
                .min(82),
            area.height.min(34),
        );
        let ink = Style::new().fg(Color::Cyan);
        let row = |y, h| Rect::new(canvas.x, y, canvas.width, h);
        let large = canvas.height >= 24 && canvas.width >= 66;
        let mut y = canvas.y;
        if large {
            let logo_width = LOGO.iter().map(|s| s.chars().count() as u16).max().unwrap();
            let logo_area = crate::centered(row(y, 7), logo_width, 7);
            let colors = if std::env::var_os("NO_COLOR").is_none() {
                [
                    Color::Rgb(102, 207, 200),
                    Color::Rgb(171, 161, 232),
                    Color::Rgb(137, 179, 217),
                ]
            } else {
                [Color::Reset; 3]
            };
            for (i, line) in LOGO.iter().enumerate() {
                let left: String = line.chars().take(10).collect();
                let right: String = line.chars().skip(10).take(13).collect();
                let word: String = line.chars().skip(23).collect();
                let spans = Line::from(vec![
                    Span::styled(left, Style::new().fg(colors[0])),
                    Span::styled(right, Style::new().fg(colors[1])),
                    Span::styled(word, Style::new().fg(colors[2])),
                ]);
                f.render_widget(
                    Paragraph::new(spans),
                    Rect::new(logo_area.x, y + i as u16, logo_area.width, 1),
                );
            }
            y += 8;
        } else {
            f.render_widget(
                Paragraph::new("BOTUI").centered().style(ink.bold()),
                row(y, 1),
            );
            y += 1;
        }
        if canvas.height >= 12 {
            let tag =
                Paragraph::new("Your TUI book reader. BookOrbit/OPDS/PDF/DjVU/EPUB/FB2 compatible")
                    .centered()
                    .dim()
                    .wrap(Wrap { trim: true });
            let h = (tag.line_count(canvas.width) as u16).min(canvas.height.saturating_sub(9));
            f.render_widget(tag, row(y, h));
            y += h + u16::from(canvas.height >= 20);
        }
        if y < canvas.bottom() {
            f.render_widget(Paragraph::new("Choose a library").bold(), row(y, 1));
            y += 1;
        }
        let footer_height = if canvas.height >= 18 { 3 } else { 1 };
        let footer_y = canvas
            .bottom()
            .saturating_sub(footer_height)
            .max(y.min(canvas.bottom()));
        let help_height = if canvas.height >= 19 {
            5
        } else if canvas.height >= 11 {
            3
        } else {
            0
        };
        let detail_y = footer_y.saturating_sub(help_height).max(y.min(footer_y));
        let pane = self.current_mut();
        let stride = if detail_y.saturating_sub(y) as usize >= pane.rows.len() * 2 {
            2
        } else {
            1
        };
        let slots = (detail_y.saturating_sub(y) / stride) as usize;
        let selected = pane.selection.selected().unwrap_or(0);
        let first = (selected + 1).saturating_sub(slots);
        *pane.selection.offset_mut() = first;
        let pane = self.current();
        let mut hits = vec![];
        for (i, r) in pane.rows.iter().enumerate().skip(first).take(slots) {
            let rect = row(y + (i - first) as u16 * stride, 1);
            let key = source(&r.action, self.session.is_some()).0;
            let mark = if selected == i { "▸" } else { " " };
            let style = if selected == i {
                ink.reversed().bold()
            } else {
                Style::new()
            };
            f.render_widget(
                Paragraph::new(format!(" {mark}  {key}   {}", r.label)).style(style),
                rect,
            );
            hits.push((rect, i));
        }
        if help_height > 0
            && let Some(action) = pane.action()
        {
            let (_, description, keys) = source(&action, self.session.is_some());
            f.render_widget(
                Paragraph::new(description).wrap(Wrap { trim: true }),
                row(detail_y, help_height - 1),
            );
            f.render_widget(Paragraph::new(keys).style(ink), row(footer_y - 1, 1));
        }
        if let Some(message) = &self.notice
            && footer_height >= 2
        {
            f.render_widget(Paragraph::new(message.as_str()).yellow(), row(footer_y, 1));
        }
        self.home_hits = hits;
        self.list_area = Rect::new(canvas.x, y, canvas.width, detail_y.saturating_sub(y));
        let footer = row(canvas.bottom() - 1, 1);
        let mut x = footer.x;
        self.home_controls.clear();
        for (label, command) in [
            ("↑↓ Choose", None),
            ("Enter Open", self.current().action()),
            ("F Background", Some(Command::Background)),
            ("U Borders", Some(Command::Borders)),
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
