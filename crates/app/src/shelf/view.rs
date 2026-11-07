use super::{Scope, Section, Shelf, Tab};
use crate::centered;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Wrap},
};

fn heading() -> Style {
    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
}

impl Shelf {
    pub(super) fn draw(&mut self, frame: &mut Frame) {
        self.keep_button = Rect::default();
        self.search_button = Rect::default();
        self.scan_button = Rect::default();
        self.rail.clear_hits();
        self.grid = None;
        self.view_button = Rect::default();
        self.back_button = Rect::default();
        self.file_button = Rect::default();
        self.list_area = Rect::default();
        self.tab_buttons = [Rect::default(); 5];
        if let Some(job) = &self.pending {
            let panel = centered(frame.area(), 54, 7);
            let border =
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title(if self.is_opds {
                        " OPDS "
                    } else {
                        " BookOrbit "
                    });
            let inner = border.inner(panel);
            frame.render_widget(border, panel);
            frame.render_widget(Paragraph::new(job.title).style(heading()), inner);
            if !job.closing {
                self.draw_buttons(frame, inner, "[ Cancel ]", true);
            }
            return;
        }
        if let Some(form) = &mut self.login {
            form.draw(frame, self.notice.as_deref());
            return;
        }
        let panel = if self.session.is_some() || self.is_opds {
            centered(
                frame.area(),
                frame
                    .area()
                    .width
                    .saturating_sub(if frame.area().width > 84 { 4 } else { 0 })
                    .min(158),
                frame
                    .area()
                    .height
                    .saturating_sub(if frame.area().height > 26 { 2 } else { 0 })
                    .min(60),
            )
        } else {
            centered(frame.area(), 84, 18)
        };
        let border = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(if self.is_opds {
                " OPDS "
            } else {
                " BookOrbit "
            });
        let inner = border.inner(panel);
        frame.render_widget(border, panel);
        if inner.is_empty() {
            return;
        }
        let detail = match &self.current().section {
            Section::Files(b) => Some(b.clone()),
            _ => None,
        };
        if let Some(book) = &detail
            && book.has_cover != Some(false)
            && inner.width >= 58
        {
            self.request_covers(&[book.id]);
        }
        if let Some(info) = &self.info {
            let width = if let Some(book) = &detail
                && inner.width >= 58
            {
                self.artwork(
                    frame,
                    Rect::new(
                        inner.right() - 20,
                        inner.y + 1,
                        19,
                        inner.height.saturating_sub(3),
                    ),
                    book,
                );
                inner.width - 22
            } else {
                inner.width
            };
            frame.render_widget(
                Paragraph::new(info.as_str()).wrap(Wrap { trim: true }),
                Rect::new(
                    inner.x + 1,
                    inner.y + 1,
                    width.saturating_sub(2),
                    inner.height.saturating_sub(3),
                ),
            );
            self.draw_buttons(frame, inner, "[ Back ]", false);
            if self.scan_file.is_some() && inner.height >= 2 {
                let x = self.back_button.right().saturating_add(1);
                self.scan_button = Rect::new(
                    x,
                    inner.bottom() - 1,
                    inner.right().saturating_sub(x).min(19),
                    1,
                );
                frame.render_widget(
                    Paragraph::new("[ P Read pages ]").style(heading()),
                    self.scan_button,
                );
            }
            return;
        }
        let inner = if inner.width >= 121
            && inner.height >= 18
            && self.view_available()
            && let Some(session) = self.session.clone()
        {
            let w = (inner.width / 5).clamp(23, 30);
            let tab = self.active_tab();
            let scope = super::rail::current_scope(&self.current().section).cloned();
            self.rail.draw(
                frame,
                Rect::new(inner.x, inner.y, w, inner.height.saturating_sub(1)),
                session,
                tab,
                scope.as_ref(),
            );
            Rect::new(inner.x + w + 1, inner.y, inner.width - w - 1, inner.height)
        } else if self.session.is_some() && inner.height >= 4 {
            self.draw_tabs(frame, inner);
            Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1)
        } else {
            inner
        };
        let top = inner.height.min(1);
        let mut left = inner;
        if let Some(book) = &detail
            && inner.width >= 58
            && inner.height >= 8
        {
            let width = (inner.width / 4).clamp(16, 22);
            left.width -= width + 2;
            self.artwork(
                frame,
                Rect::new(
                    inner.right() - width - 1,
                    inner.y + 2,
                    width,
                    inner.height.saturating_sub(5),
                ),
                book,
            );
        }
        frame.render_widget(
            Paragraph::new(self.current().title.as_str()).style(heading()),
            Rect::new(left.x + 1, left.y, left.width.saturating_sub(2), top),
        );
        if detail.is_some() && left.width >= 30 {
            self.keep_button = Rect::new(left.right() - 16, left.y, 16, 1);
            frame.render_widget(
                Paragraph::new("[D Save offline]").style(heading()),
                self.keep_button,
            );
        }
        if self.view_available() && left.width >= 19 {
            let label = if self.grid_view {
                "[V List]"
            } else {
                "[V Grid]"
            };
            let w = label.chars().count() as u16;
            if (self.session.is_some() || self.links.search.is_some()) && left.width >= 30 {
                self.search_button = Rect::new(left.right() - w - 12, left.y, 11, 1);
                frame.render_widget(
                    Paragraph::new("[/ Search]").style(Style::new().fg(Color::Cyan)),
                    self.search_button,
                );
            }
            self.view_button = Rect::new(left.right() - w, left.y, w, 1);
            frame.render_widget(
                Paragraph::new(label).style(Style::new().fg(Color::Cyan)),
                self.view_button,
            );
        }
        let bottom = u16::from(inner.height >= 3);
        let notice_rows = if self.notice.is_some() {
            inner.height.saturating_sub(top + bottom).min(2)
        } else {
            0
        };
        let gap = u16::from(inner.height >= 8);
        self.list_area = Rect::new(
            left.x,
            left.y + top + gap,
            left.width,
            inner
                .height
                .saturating_sub(top + gap + bottom + notice_rows),
        );
        let list_area = self.list_area;
        if !self.grid_view && gap > 0 && matches!(self.current().section, Section::Books { .. }) {
            frame.render_widget(
                Paragraph::new(book_line(None, list_area.width)).style(Style::new().dim()),
                Rect::new(left.x, list_area.y - 1, left.width, 1),
            );
        }
        if self.grid_view
            && self.view_available()
            && !self.current().rows.is_empty()
            && list_area.height >= 5
            && list_area.width >= 18
        {
            self.draw_grid(frame, list_area);
        } else if self.current().rows.is_empty() {
            let pane = self.current();
            let empty = match pane.section {
                Section::Collections => "No personal collections yet.",
                Section::Libraries => "No libraries available to this account.",
                Section::Files(_) => "No files available for this book.",
                Section::Books {
                    scope: Scope::All, ..
                } => "No books available yet.",
                Section::Books {
                    scope: Scope::Library(_),
                    ..
                } => "This library is empty.",
                Section::Books {
                    scope: Scope::Collection(_),
                    ..
                } => "This collection is empty.",
                Section::Books { .. } => "No books on this shelf.",
                Section::Home => "",
                Section::Feed { .. } => "No books or catalog entries here.",
            };
            frame.render_widget(Paragraph::new(empty).wrap(Wrap { trim: true }), list_area);
        } else {
            let pane = self.current_mut();
            let rows = pane.rows.iter().map(|row| match &row.action {
                super::Command::Files(b) => ListItem::new(book_line(Some(b), list_area.width)),
                _ => ListItem::new(format!(" {}", row.label)),
            });
            frame.render_stateful_widget(
                List::new(rows).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
                list_area,
                &mut pane.selection,
            );
        }
        if let Some(message) = &self.notice {
            frame.render_widget(
                Clear,
                Rect::new(
                    inner.x,
                    inner.bottom() - bottom - notice_rows,
                    inner.width,
                    notice_rows,
                ),
            );
            frame.render_widget(
                Paragraph::new(message.as_str())
                    .style(Style::new().fg(Color::Yellow))
                    .wrap(Wrap { trim: true }),
                Rect::new(
                    inner.x,
                    inner.bottom() - bottom - notice_rows,
                    inner.width,
                    notice_rows,
                ),
            );
        }
        let (back, local) = if matches!(self.current().section, Section::Home) {
            ("[ Quit ]", false)
        } else {
            ("[ Back ]", true)
        };
        self.draw_buttons(frame, inner, back, local);
    }

    fn draw_tabs(&mut self, frame: &mut Frame, area: Rect) {
        let labels = if area.width >= 76 {
            [
                "1 All books",
                "2 Libraries",
                "3 Collections",
                "4 Currently reading",
                "5 Recently added",
            ]
        } else if area.width >= 42 {
            ["1 Books", "2 Libs", "3 Lists", "4 Reading", "5 New"]
        } else {
            ["1 All", "2 L", "3 C", "4 R", "5 +"]
        };
        let active = self.active_tab();
        let mut x = area.x;
        for (i, label) in labels.iter().enumerate() {
            let width = (label.chars().count() as u16 + 2).min(area.right().saturating_sub(x));
            let rect = Rect::new(x, area.y, width, 1);
            self.tab_buttons[i] = rect;
            let style = if active == Tab::ALL[i] {
                heading().add_modifier(Modifier::REVERSED)
            } else {
                Style::new().fg(Color::Cyan)
            };
            frame.render_widget(Paragraph::new(format!(" {label} ")).style(style), rect);
            x = rect.right();
        }
    }

    fn draw_buttons(&mut self, frame: &mut Frame, inner: Rect, back: &str, local: bool) {
        if inner.height < 2 {
            return;
        }
        let y = inner.bottom() - 1;
        let back_width = (back.chars().count() as u16).min(inner.width);
        self.back_button = Rect::new(inner.x, y, back_width, 1);
        frame.render_widget(Paragraph::new(back), self.back_button);
        if local {
            let label = "[ Local library ]";
            let x = self.back_button.right().saturating_add(2);
            self.file_button = Rect::new(
                x,
                y,
                (label.chars().count() as u16).min(inner.right().saturating_sub(x)),
                1,
            );
            frame.render_widget(Paragraph::new(label), self.file_button);
        }
        if self.pending.is_none() && self.info.is_none() && inner.width > 54 {
            let help = if matches!(self.current().section, Section::Books { .. }) {
                "PgUp/PgDn · Enter or click"
            } else {
                "Enter or click to open"
            };
            let x = inner.right().saturating_sub(help.chars().count() as u16);
            if !self.back_button.contains(Position::new(x, y))
                && !self.file_button.contains(Position::new(x, y))
            {
                frame.render_widget(
                    Paragraph::new(help).style(Style::new().add_modifier(Modifier::DIM)),
                    Rect::new(x, y, inner.right() - x, 1),
                );
            }
        }
    }
}

fn book_line(b: Option<&super::Book>, width: u16) -> ratatui::text::Line<'static> {
    use super::gallery::{caption, formats};
    use ratatui::text::{Line, Span};
    let available = width.saturating_sub(2);
    let format = if available >= 40 { 10 } else { 0 };
    let progress = if available >= 26 { 5 } else { 0 };
    let author = if available >= 50 {
        (available - format - progress) / 3
    } else {
        0
    };
    let title = available - format - author - progress;
    let cell = |text: &str, w: u16| {
        if w == 0 {
            return String::new();
        }
        let text = caption(text, w - 1, 1);
        let used = Line::from(text.as_str()).width();
        format!("{}{}", text, " ".repeat((w as usize).saturating_sub(used)))
    };
    let (ext, name, by, pct) = match b {
        Some(b) => (
            formats(b),
            super::single_line(b.title.clone().unwrap_or_else(|| "Untitled".into())),
            super::single_line(b.authors.join(", ")),
            b.progress.map(|n| format!("{n:>3}%")).unwrap_or_default(),
        ),
        None => ("Format".into(), "Title".into(), "Author".into(), "%".into()),
    };
    Line::from(vec![
        Span::raw(" "),
        Span::styled(cell(&ext, format), Style::new().fg(Color::Cyan)),
        Span::raw(cell(&name, title)),
        Span::raw(cell(&by, author)),
        Span::raw(if progress > 0 { pct } else { String::new() }),
    ])
}
