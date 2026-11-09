use super::{Entry, Picker};
use crate::catalog::{Book, BookFile};
use crate::shelf::gallery::{Grid, book_face};
use ratatui::{
    Frame,
    layout::Rect,
    text::Text,
    widgets::{Paragraph, Wrap},
};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::atomic::AtomicBool,
};

impl Picker {
    pub(super) fn gallery(&mut self, f: &mut Frame) {
        let selected = self.list.selected().unwrap_or(0);
        let books = self.entries.iter().any(|e| e.kind.is_some());
        let grid = Grid::new(
            self.rows,
            books,
            self.entries.len(),
            selected,
            self.list.offset_mut(),
        );
        for &(i, area) in &grid.cells {
            let entry = &self.entries[i];
            let active = i == selected;
            let border = crate::appearance::card(active);
            let inner = border.inner(area);
            f.render_widget(border, area);
            if inner.is_empty() {
                continue;
            }
            let Some(kind) = entry.kind else {
                f.render_widget(
                    Paragraph::new(Text::styled(
                        format!("{}/", entry.name),
                        crate::appearance::card_title(active),
                    ))
                    .wrap(Wrap { trim: true }),
                    Rect {
                        y: inner.y + inner.height / 3,
                        height: inner.height - inner.height / 3,
                        ..inner
                    },
                );
                continue;
            };
            let m = entry
                .meta
                .clone()
                .or_else(|| self.index.cached(&entry.path));
            let book = card(entry, kind, m.as_deref());
            let path = entry.path.clone();
            self.covers.request(book.id, move || {
                if kind.readable() {
                    return Ok(botui_formats::cover(&path).unwrap_or(None));
                }
                let page = botui_fixed::page(&path, kind, 1, 240, &AtomicBool::new(false))?;
                let mut out = std::io::Cursor::new(Vec::new());
                page.write_to(&mut out, image::ImageFormat::Png)?;
                Ok(Some(out.into_inner()))
            });
            book_face(
                f,
                inner,
                &book,
                self.covers.image(book.id),
                self.graphics.get_or_insert_with(crate::cover::graphics),
                active,
            );
        }
        self.grid = Some(grid);
    }
}
fn card(
    e: &Entry,
    kind: botui_formats::FileKind,
    meta: Option<&botui_formats::BookMetadata>,
) -> Book {
    let mut h = DefaultHasher::new();
    e.path.hash(&mut h);
    let id = h.finish();
    let (title, authors) = crate::catalog::local_credits(&e.path, meta);
    Book {
        id,
        title: Some(title),
        authors,
        files: vec![BookFile {
            id,
            format: Some(kind.label().into()),
            role: "primary".into(),
            size_bytes: None,
        }],
        series: meta.and_then(|m| m.series.first()).cloned(),
        series_index: None,
        has_cover: None,
        progress: e.percent,
    }
}
