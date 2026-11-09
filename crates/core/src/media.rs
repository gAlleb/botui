//! Layout for terminal illustrations and simple tables. Source text blocks are
//! untouched, so existing reading positions and annotations keep their meaning.
use super::{Anchor, Block, PageLine, wrap_block};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct TableRow {
    pub end: usize,
    pub cells: Vec<String>,
    /// Width in logical columns for each cell, including rowspan placeholders.
    pub spans: Vec<usize>,
    pub header: bool,
    pub columns: usize,
}
pub(super) fn table(lines: &mut Vec<PageLine>, start: usize, row: &TableRow, width: usize) {
    let columns = row.columns.max(row.cells.len()).max(1);
    let cell_width = width.saturating_sub((columns - 1) * 3) / columns;
    let anchor = Anchor {
        block: start,
        byte: 0,
    };
    if cell_width < 4 || columns > 12 {
        for (i, s) in row.cells.iter().enumerate() {
            let mut buf = vec![];
            wrap_block(
                &mut buf,
                &Block {
                    text: format!("{}: {s}", i + 1),
                    heading: false,
                },
                start,
                width,
                0,
            );
            for mut l in buf {
                l.start = anchor;
                lines.push(l);
            }
        }
        return;
    }
    let span = |i: usize| row.spans.get(i).copied().unwrap_or(1);
    let width_at = |i: usize| cell_width * span(i) + 3 * (span(i) - 1);
    let wrapped: Vec<_> = row
        .cells
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut buf = vec![];
            wrap_block(
                &mut buf,
                &Block {
                    text: s.clone(),
                    heading: false,
                },
                start,
                width_at(i),
                0,
            );
            buf
        })
        .collect();
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(0).max(1);
    for y in 0..height {
        let mut line = String::new();
        let occupied: usize = (0..row.cells.len()).map(span).sum();
        let count = row.cells.len() + columns.saturating_sub(occupied);
        for col in 0..count {
            if col > 0 {
                line.push_str(" │ ");
            }
            let t = wrapped
                .get(col)
                .and_then(|v| v.get(y))
                .map_or("", |l| l.text.as_str());
            line.push_str(t);
            if col + 1 < count {
                line.push_str(&" ".repeat(width_at(col).saturating_sub(t.width())));
            }
        }
        lines.push(PageLine {
            indent: 0,
            text: line,
            heading: false,
            start: anchor,
            image: None,
            table_header: row.header,
        });
    }
}

pub(super) fn preformatted(lines: &mut Vec<PageLine>, text: &str, block: usize, width: usize) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut offset = 0;
    for source in text.split('\n') {
        let mut byte = offset;
        let mut row = String::new();
        let mut cells = 0;
        let line = |text, byte| PageLine {
            text,
            indent: 0,
            heading: false,
            image: None,
            table_header: false,
            start: Anchor { block, byte },
        };
        for (i, glyph) in source.grapheme_indices(true) {
            if cells + glyph.width() > width && !row.is_empty() {
                lines.push(line(std::mem::take(&mut row), byte));
                byte = offset + i;
                cells = 0;
            }
            row.push_str(glyph);
            cells += glyph.width();
        }
        lines.push(line(row, byte));
        offset += source.len() + 1;
    }
}
