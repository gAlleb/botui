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
        self.scan_button = Rect::default();
        self.rail.clear_hits();
        self.grid = None;
        self.view_button = Rect::default();
        self.back_button = Rect::default();
        self.file_button = Rect::default();
        self.list_area = Rect::default();
        self.tab_buttons = [Rect::default(); 3];
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
                self.draw_buttons(frame, inner, "[ Отмена ]", true);
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
            self.draw_buttons(frame, inner, "[ Назад ]", false);
            if self.scan_file.is_some() && inner.height >= 2 {
                let x = self.back_button.right().saturating_add(1);
                self.scan_button = Rect::new(
                    x,
                    inner.bottom() - 1,
                    inner.right().saturating_sub(x).min(19),
                    1,
                );
                frame.render_widget(
                    Paragraph::new("[ P Читать скан ]").style(heading()),
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
        if self.view_available() && left.width >= 19 {
            let label = if self.grid_view {
                "[V Список]"
            } else {
                "[V Сетка]"
            };
            let w = label.chars().count() as u16;
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
                Section::Collections => "Личных коллекций пока нет.",
                Section::Libraries => "Сервер не вернул доступных библиотек для этого аккаунта.",
                Section::Files(_) => "У книги нет доступных файлов.",
                Section::Books {
                    scope: Scope::All, ..
                } => "В доступном каталоге пока нет книг.",
                Section::Books {
                    scope: Scope::Library(_),
                    ..
                } => "В этой библиотеке пока нет книг.",
                Section::Books {
                    scope: Scope::Collection(_),
                    ..
                } => "В этой коллекции пока нет книг.",
                Section::Home => "",
                Section::Feed { .. } => "В этом разделе нет доступных книг или подразделов.",
            };
            frame.render_widget(Paragraph::new(empty).wrap(Wrap { trim: true }), list_area);
        } else {
            let pane = self.current_mut();
            let rows = pane
                .rows
                .iter()
                .map(|row| ListItem::new(format!(" {}", row.label)));
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
            ("[ Выход ]", false)
        } else {
            ("[ Назад ]", true)
        };
        self.draw_buttons(frame, inner, back, local);
    }

    fn draw_tabs(&mut self, frame: &mut Frame, area: Rect) {
        let labels = if area.width >= 41 {
            ["1 Все книги", "2 Библиотеки", "3 Коллекции"]
        } else if area.width >= 24 {
            ["1 Книги", "2 Библ.", "3 Колл."]
        } else {
            ["1 Все", "2 Б", "3 К"]
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
            let label = "[ Локальный файл ]";
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
                "PgUp/PgDn · Enter / щелчок"
            } else {
                "Enter / щелчок — открыть"
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
