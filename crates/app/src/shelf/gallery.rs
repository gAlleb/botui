//! Geometry is shared by drawing, keyboard navigation and mouse hit testing.
pub(crate) mod binding;
mod jacket_edge;
use super::{Book, Command, Scope, Shelf, covers::Artwork, single_line};
use crate::centered;
use crate::cover::ImagePlacement;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Style, Stylize},
    text::Text,
    widgets::{Paragraph, Wrap},
};

pub(crate) struct Grid {
    pub cols: usize,
    pub(crate) cells: Vec<(usize, Rect)>,
}
impl Grid {
    pub(crate) fn new(
        area: Rect,
        books: bool,
        count: usize,
        total: Option<u64>,
        selected: usize,
        offset: &mut usize,
    ) -> Self {
        let cols = (area.width / if books { 27 } else { 29 }).clamp(1, 6) as usize;
        // Directory rows keep their final height while later pages are still in flight.
        let layout_count = total.map_or(count, |n| {
            usize::try_from(n).unwrap_or(usize::MAX).max(count)
        });
        let rows = ((area.height / if books { 16 } else { 4 }).max(1) as usize)
            .min(layout_count.div_ceil(cols).max(1));
        let height = (area.height / rows as u16).min(if books { 22 } else { 5 });
        let first = (*offset / cols)
            .min(selected / cols)
            .max((selected / cols + 1).saturating_sub(rows))
            .min(count.div_ceil(cols).saturating_sub(rows));
        *offset = first * cols;
        let mut cells = Vec::new();
        for i in *offset..count.min(*offset + rows * cols) {
            let n = i - *offset;
            let x = (n % cols) * usize::from(area.width) / cols;
            let end = (n % cols + 1) * usize::from(area.width) / cols;
            cells.push((
                i,
                Rect::new(
                    area.x + x as u16,
                    area.y + (n / cols) as u16 * height,
                    (end - x) as u16 - u16::from(cols > 1),
                    height.saturating_sub(u16::from(height >= 5 && !books)),
                ),
            ));
        }
        Self { cols, cells }
    }
    pub fn len(&self) -> usize {
        self.cells.len()
    }
    pub fn hit(&self, pos: Position) -> Option<usize> {
        self.cells
            .iter()
            .find(|(_, r)| r.contains(pos))
            .map(|(i, _)| *i)
    }
}

pub fn formats(book: &Book) -> String {
    let mut values = Vec::new();
    for ext in book.files.iter().filter_map(|f| f.format.as_deref()) {
        let ext = single_line(ext.to_uppercase());
        if !ext.is_empty() && !values.contains(&ext) {
            values.push(ext);
        }
    }
    values.join(" / ")
}
fn line(area: Rect, y: u16, height: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(y.min(area.height)),
        area.width,
        height.min(area.height.saturating_sub(y)),
    )
}
fn amount(n: u64) -> String {
    let noun = if n == 1 { "book" } else { "books" };
    format!("{n} {noun}")
}

/// Wrap at a word when possible, and mark clipped captions without splitting a grapheme.
pub(super) fn caption(text: &str, width: u16, max_lines: usize) -> String {
    use unicode_truncate::UnicodeTruncateStr;
    let mut out = Vec::new();
    let mut rest = text.trim();
    for row in 0..max_lines {
        let (prefix, _) = rest.unicode_truncate(usize::from(width));
        if prefix.len() == rest.len() {
            out.push(rest.to_owned());
            break;
        }
        if row + 1 == max_lines {
            let (cut, _) = rest.unicode_truncate(usize::from(width.saturating_sub(1)));
            out.push(format!("{}…", cut.trim_end()));
            break;
        }
        let cut = prefix.rfind(' ').filter(|i| *i > 0).unwrap_or(prefix.len());
        if cut == 0 {
            break;
        }
        out.push(rest[..cut].to_owned());
        rest = rest[cut..].trim_start();
    }
    out.join("\n")
}

/// A cloth binding and foil ornament drawn as terminal cells. The same title
/// and author always get the same jacket, independent of sorting and resizing.
fn jacket(
    frame: &mut Frame,
    slot: Rect,
    book: &Book,
    graphics: &ratatui_image::picker::Picker,
    placement: ImagePlacement,
) {
    let binding = binding::Binding::from(book);
    if graphics.protocol_type() != ratatui_image::picker::ProtocolType::Halfblocks
        && binding.draw_or_wait(frame, slot, graphics, placement)
    {
        return;
    }
    let title = &binding.title;
    let (r, g, b) = binding.cloth();
    let cloth = Style::new()
        .bg(Color::Rgb(r, g, b))
        .fg(Color::Rgb(246, 229, 191));
    let gold = cloth.fg(Color::Rgb(222, 192, 139));
    let slot = placement.text_area(slot);
    let area = centered(slot, (slot.height * 4 / 3).max(4), slot.height);
    let area = jacket_edge::paper(frame, area, cloth, (r, g, b), graphics);
    if area.is_empty() {
        return;
    }
    if area.width < 4 || area.height < 2 {
        return;
    }
    frame.render_widget(
        Paragraph::new("│\n".repeat(area.height.into())).style(gold),
        Rect::new(area.x + 1, area.y, 1, area.height),
    );
    let field = Rect::new(area.x + 2, area.y, area.width - 3, area.height);
    let ornament = "─ ❧ ─";
    frame.render_widget(
        Paragraph::new(ornament).centered().style(gold),
        line(field, 0, 1),
    );
    if field.height > 3 {
        let y = (field.height / 3).max(1);
        frame.render_widget(
            Paragraph::new(if field.width < 9 {
                title
                    .split_whitespace()
                    .filter_map(|w| w.chars().next())
                    .take(2)
                    .flat_map(char::to_uppercase)
                    .collect()
            } else {
                caption(
                    title,
                    field.width,
                    usize::from(field.height.saturating_sub(y + 1)),
                )
            })
            .wrap(Wrap { trim: true })
            .centered()
            .style(cloth.bold()),
            line(field, y, (field.height - y).saturating_sub(1)),
        );
        frame.render_widget(
            Paragraph::new("───").centered().style(gold),
            line(field, field.height - 1, 1),
        );
    }
}
impl Shelf {
    pub(super) fn artwork(&self, frame: &mut Frame, slot: Rect, book: &Book) {
        self.covers
            .artwork(book)
            .draw(frame, slot, book, ImagePlacement::Fit);
    }
    pub(super) fn draw_grid(&mut self, frame: &mut Frame, area: Rect) {
        let books = matches!(self.current().section, super::Section::Books { .. })
            || matches!(self.current().section, super::Section::Feed { .. })
                && self
                    .current()
                    .rows
                    .iter()
                    .any(|r| matches!(r.action, Command::Files(_)));
        let pane = self.current_mut();
        let selected = pane.selection.selected().unwrap_or(0);
        let total = match &pane.section {
            super::Section::Groups { total, .. } => Some(*total),
            _ => None,
        };
        let grid = Grid::new(
            area,
            books,
            pane.rows.len(),
            total,
            selected,
            pane.selection.offset_mut(),
        );
        let mut wanted = vec![];
        for &(i, cell) in &grid.cells {
            let row = &self.current().rows[i];
            let active = selected == i;
            let border = crate::appearance::card(active);
            let inner = border.inner(cell);
            frame.render_widget(border, cell);
            if inner.is_empty() {
                continue;
            }
            match &row.action {
                Command::Files(book) => {
                    if book.has_cover != Some(false) {
                        wanted.push(book.id);
                    }
                    book_face(frame, inner, book, self.covers.artwork(book), active);
                }
                Command::Books(scope, _) => {
                    let n = match scope {
                        Scope::Library(l) => l.book_count,
                        Scope::Collection(c) => Some(c.book_count),
                        Scope::Group(_, g) => g.book_count,
                        _ => None,
                    };
                    frame.render_widget(
                        Paragraph::new(Text::styled(
                            format!(" {}", single_line(scope.name().into())),
                            crate::appearance::card_title(active),
                        )),
                        line(inner, 0, 1),
                    );
                    if let Some(n) = n {
                        frame.render_widget(
                            Paragraph::new(format!(" {}", amount(n)))
                                .style(Style::new().fg(Color::Cyan)),
                            line(inner, 1, 1),
                        );
                    }
                }
                _ => frame.render_widget(
                    Paragraph::new(Text::styled(
                        row.label.as_str(),
                        crate::appearance::card_title(active),
                    ))
                    .centered()
                    .wrap(Wrap { trim: true }),
                    line(inner, inner.height / 2, 2),
                ),
            }
        }
        self.request_covers(&wanted);
        self.grid = Some(grid);
    }
}

impl Artwork<'_> {
    fn draw(self, frame: &mut Frame, area: Rect, book: &Book, placement: ImagePlacement) {
        if area.is_empty() {
            return;
        }
        match self {
            Self::Image(pic) => pic.draw_cover(frame, area, &crate::cover::graphics(), placement),
            Self::Jacket => jacket(frame, area, book, &crate::cover::graphics(), placement),
            Self::Loading => {}
        }
    }
}

pub(crate) fn book_face(
    frame: &mut Frame,
    inner: Rect,
    book: &Book,
    artwork: Artwork<'_>,
    selected: bool,
) {
    if inner.is_empty() {
        return;
    }
    let meta = if inner.height >= 7 { 4 } else { 2 };
    let art = inner.height.saturating_sub(meta);
    let title = single_line(book.title.clone().unwrap_or_else(|| "Untitled".into()));
    let area = line(inner, 0, art);
    artwork.draw(frame, area, book, ImagePlacement::ShelfCard);
    frame.render_widget(
        Paragraph::new(Text::styled(
            caption(&title, inner.width, if meta == 4 { 2 } else { 1 }),
            crate::appearance::card_title(selected),
        ))
        .centered()
        .wrap(Wrap { trim: true }),
        line(inner, art, meta - 1),
    );
    if meta == 4 {
        frame.render_widget(
            Paragraph::new(caption(
                &single_line(book.authors.join(", ")),
                inner.width,
                1,
            ))
            .centered()
            .dim(),
            line(inner, art + 2, 1),
        );
    }
    frame.render_widget(
        Paragraph::new(if let Some(p) = book.progress {
            format!("{}  ·  {p}%", formats(book))
        } else {
            formats(book)
        })
        .centered()
        .style(Style::new().fg(Color::Cyan)),
        line(inner, inner.height - 1, 1),
    );
}
