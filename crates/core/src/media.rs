//! Layout for terminal illustrations and simple tables. Source text blocks are
//! untouched, so existing reading positions and annotations keep their meaning.
use super::{Anchor, Block, PageLine, wrap_block};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct TableRow {
    pub end: usize,
    pub cells: Vec<String>,
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
    let wrapped: Vec<_> = row
        .cells
        .iter()
        .map(|s| {
            let mut buf = vec![];
            wrap_block(
                &mut buf,
                &Block {
                    text: s.clone(),
                    heading: false,
                },
                start,
                cell_width,
                0,
            );
            buf
        })
        .collect();
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(0).max(1);
    for y in 0..height {
        let mut line = String::new();
        for col in 0..columns {
            if col > 0 {
                line.push_str(" │ ");
            }
            let t = wrapped
                .get(col)
                .and_then(|v| v.get(y))
                .map_or("", |l| l.text.as_str());
            line.push_str(t);
            if col + 1 < columns {
                line.push_str(&" ".repeat(cell_width.saturating_sub(t.width())));
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
