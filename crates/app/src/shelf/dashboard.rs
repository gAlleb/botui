use super::{
    Command, Loaded, Section, Session, Shelf,
    gallery::{Grid, book_face},
    native::book_rows,
};
use botui_bookorbit::{
    CatalogOrder,
    dashboard::{DashboardShelf, Widget},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    widgets::Paragraph,
};
use std::ops::Range;

struct Strip {
    title: &'static str,
    books: Range<usize>,
    empty: &'static str,
}
pub(super) struct Dashboard {
    strips: Vec<Strip>,
    widgets: Vec<(String, String)>,
}

pub(super) fn load(session: &Session) -> botui_bookorbit::Result<Loaded> {
    use DashboardShelf::*;
    const LIMIT: u32 = 12;
    let get = |shelf| {
        let result = session.with_token(|a, t| a.dashboard_shelf(t, shelf, LIMIT));
        let order = match shelf {
            Continue => CatalogOrder::Reading,
            Recent => CatalogOrder::Recent,
            Discover => return result,
        };
        match result {
            Err(e) if botui_bookorbit::missing_route(e.as_ref()) => session
                .with_token(|a, t| a.catalog_query(t, None, 0, LIMIT, order, None))
                .map(|p| p.items),
            other => other,
        }
    };
    let (strips, widgets) = std::thread::scope(|scope| {
        let handles = [Continue, Recent, Discover].map(|kind| scope.spawn(move || get(kind)));
        let widgets = session
            .with_token(|a, t| a.dashboard_widgets(t))
            .unwrap_or_default();
        (
            handles.map(|job| job.join().expect("shelf worker")),
            widgets,
        )
    });
    let mut rows = vec![];
    let mut sections = vec![];
    for ((title, empty), result) in [
        ("Continue reading", "No books in progress."),
        ("Recently added", "No recent additions."),
        ("Discover something new", "No suggestions yet."),
    ]
    .into_iter()
    .zip(strips)
    {
        let start = rows.len();
        let empty = match result {
            Ok(items) => {
                rows.extend(book_rows(items));
                empty
            }
            Err(e) if botui_bookorbit::is_unauthorized(e.as_ref()) => return Err(e),
            Err(_) => "Shelf unavailable · 0 to refresh",
        };
        sections.push(Strip {
            title,
            books: start..rows.len(),
            empty,
        });
    }
    let widgets = widgets
        .into_iter()
        .map(|w| match w {
            Widget::Streak {
                current_streak,
                longest_streak,
            } => (
                "Reading streak".into(),
                format!("{current_streak} days · best {longest_streak}"),
            ),
            Widget::Goal {
                goal_books,
                completed_books,
                year,
            } => (
                format!("Reading goal {year}"),
                goal_books.map_or_else(
                    || format!("{completed_books} read · no goal set"),
                    |n| format!("{completed_books} / {n} books"),
                ),
            ),
            Widget::Challenge {
                title,
                progress,
                target,
            } => (
                "Monthly challenge".into(),
                format!("{title} · {progress}/{target}"),
            ),
            Widget::Dna {
                archetype,
                books_analyzed,
            } => (
                "Reading DNA".into(),
                if books_analyzed < 5 {
                    "Read 5 books to unlock".into()
                } else {
                    archetype
                },
            ),
        })
        .collect();
    Ok(Loaded::Dashboard(
        Dashboard {
            strips: sections,
            widgets,
        },
        rows,
    ))
}

impl Dashboard {
    pub fn vertical(&self, index: usize, down: bool) -> usize {
        let Some((position, strip)) = self
            .strips
            .iter()
            .enumerate()
            .find(|(_, s)| s.books.contains(&index))
        else {
            return index;
        };
        let next = if down {
            self.strips[position + 1..]
                .iter()
                .find(|s| !s.books.is_empty())
        } else {
            self.strips[..position]
                .iter()
                .rev()
                .find(|s| !s.books.is_empty())
        };
        next.map_or(index, |s| {
            s.books.start + (index - strip.books.start).min(s.books.len() - 1)
        })
    }
}
impl Shelf {
    pub(super) fn draw_dashboard(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 4 || area.height < 3 {
            return;
        }
        let ink = Style::new().fg(Color::Cyan);
        let username = super::single_line(self.username.clone());
        frame.render_widget(
            Paragraph::new(format!(" Dashboard  ·  {username}")).style(ink.bold()),
            Rect::new(area.x, area.y, area.width, 1),
        );
        let mut y = area.y + 1;
        self.search_button = Rect::new(
            area.right() - area.width.min(10),
            area.y,
            area.width.min(10),
            1,
        );
        frame.render_widget(Paragraph::new("[/ Search]").style(ink), self.search_button);
        let pane = self.current();
        let Section::Dashboard(data) = &pane.section else {
            return;
        };
        let bottom = area
            .bottom()
            .saturating_sub(1 + u16::from(self.notice.is_some()));
        if !data.widgets.is_empty() && bottom.saturating_sub(y) >= 14 {
            let across = usize::from((area.width / 24).clamp(1, 4));
            for chunk in data.widgets.chunks(across) {
                for (i, (label, value)) in chunk.iter().enumerate() {
                    let w = area.width / across as u16;
                    let rect = Rect::new(area.x + i as u16 * w, y, w, 3);
                    let block = crate::appearance::block()
                        .title(format!(" {} ", super::single_line(label.clone())));
                    let inside = block.inner(rect);
                    frame.render_widget(block, rect);
                    frame.render_widget(
                        Paragraph::new(super::single_line(value.clone())).centered(),
                        inside,
                    );
                }
                y += 3;
                if bottom.saturating_sub(y) < 11 {
                    break;
                }
            }
        }
        let selected = pane.selection.selected();
        let mut cells = vec![];
        let mut wanted = vec![];
        let cols = usize::from((area.width / 23).clamp(1, 8));
        let first_y = y;
        let active_strip = data
            .strips
            .iter()
            .position(|s| selected.is_some_and(|i| s.books.contains(&i)))
            .unwrap_or(0);
        let count = ((bottom.saturating_sub(y) / 3).max(1) as usize).min(data.strips.len());
        let first_strip = (active_strip + 1).saturating_sub(count);
        for (i, s) in data.strips.iter().skip(first_strip).take(count).enumerate() {
            let left = (count - i) as u16;
            let h = (bottom.saturating_sub(y) / left).max(1);
            if y >= bottom {
                break;
            }
            frame.render_widget(
                Paragraph::new(format!(" {}", s.title)).style(ink.bold()),
                Rect::new(area.x, y, area.width, 1),
            );
            let tile_h = h.saturating_sub(1);
            let offset = selected
                .filter(|idx| s.books.contains(idx))
                .map_or(0, |idx| (idx - s.books.start) / cols * cols);
            if s.books.is_empty() && tile_h > 0 {
                frame.render_widget(
                    Paragraph::new(format!(" {}", s.empty)).dim(),
                    Rect::new(area.x, y + 1, area.width, 1),
                );
            }
            for (col, index) in s.books.clone().skip(offset).take(cols).enumerate() {
                if tile_h == 0 {
                    break;
                }
                let Command::Files(book) = &pane.rows[index].action else {
                    continue;
                };
                let x = area.x + (col * area.width as usize / cols) as u16;
                let end = area.x + ((col + 1) * area.width as usize / cols) as u16;
                let tile = Rect::new(x, y + 1, end - x, tile_h);
                let active = selected == Some(index);
                if tile_h >= 5 {
                    let border = crate::appearance::card(active);
                    let face = border.inner(tile);
                    frame.render_widget(border, tile);
                    book_face(
                        frame,
                        face,
                        book,
                        self.covers.image(book.id),
                        &self.graphics,
                        active,
                    );
                    if book.has_cover != Some(false) {
                        wanted.push(book.id);
                    }
                } else {
                    let percent = book.progress.map(|p| format!("{p}% ")).unwrap_or_default();
                    let title =
                        super::single_line(book.title.clone().unwrap_or_else(|| "Untitled".into()));
                    frame.render_widget(
                        Paragraph::new(format!("{percent}{title}"))
                            .style(crate::appearance::card_title(active)),
                        tile,
                    );
                }
                cells.push((index, tile));
            }
            y += h;
        }
        if let Some(notice) = &self.notice {
            frame.render_widget(
                Paragraph::new(notice.as_str()).yellow(),
                Rect::new(area.x, bottom, area.width, 1),
            );
        }
        self.list_area = Rect::new(area.x, first_y, area.width, bottom.saturating_sub(first_y));
        self.grid = Some(Grid { cols, cells });
        self.request_covers(&wanted);
    }
}
