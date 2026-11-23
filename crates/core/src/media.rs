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
            spans: Vec::new(),
            indent: 0,
            text: line,
            heading: false,
            start: anchor,
            figures: Vec::new(),
            surfaces: Vec::new(),
            table_header: row.header,
            chapter_start: false,
            equation: (0, 0),
        });
    }
}

/// Replace the vertical margins around a standalone picture before paginating.
/// Images alongside text, table cells or other artwork retain their CSS layout.
pub(super) fn space_figures(
    lines: Vec<PageLine>,
    doc: &super::Document,
    cell: (u16, u16),
) -> Vec<PageLine> {
    if !lines.iter().any(|line| !line.figures.is_empty()) {
        return lines;
    }
    let mut result = Vec::with_capacity(lines.len());
    let mut rows = lines.into_iter();
    let mut after_image = false;
    while let Some(line) = rows.next() {
        let Some(figure) = standalone_figure(&line, rows.as_slice()) else {
            if after_image {
                if line.figures.is_empty() && bare(&line) {
                    continue;
                }
                result.push(PageLine {
                    start: result.last().map_or(line.start, PageLine::anchor),
                    ..Default::default()
                });
            }
            after_image = false;
            result.push(line);
            continue;
        };
        while result
            .last()
            .is_some_and(|l| l.figures.is_empty() && bare(l))
        {
            result.pop();
        }
        if let Some(prev) = result.last() {
            result.push(PageLine {
                start: prev.start,
                ..Default::default()
            });
        }
        let mut height = figure.height;
        if let super::Target::Image(member) = &doc.references[figure.reference].target
            && let Some(&(w, h)) = doc.image_sizes.get(member)
            && w > 0
            && h > 0
        {
            let fitted = f64::from(figure.width) * f64::from(cell.0) * h as f64
                / (f64::from(cell.1) * w as f64);
            height = height.min((fitted.round() as u16).max(1));
        }
        for mut row in
            std::iter::once(line).chain(rows.by_ref().take(usize::from(figure.height - 1)))
        {
            if row.figures[0].row < height {
                row.figures[0].height = height;
                result.push(row);
            }
        }
        after_image = true;
    }
    result
}

pub(super) fn bare(line: &PageLine) -> bool {
    line.text.trim().is_empty()
        && !line.has_visible_surface()
        && !line.table_header
        && line.spans.iter().all(|s| s.source.is_none())
}

fn standalone_figure(first: &PageLine, following: &[PageLine]) -> Option<super::Figure> {
    let [figure] = first.figures.as_slice() else {
        return None;
    };
    if figure.row != 0 || !bare(first) {
        return None;
    }
    let rest = following.get(..usize::from(figure.height.checked_sub(1)?))?;
    let complete = rest.iter().enumerate().all(|(index, line)| {
        bare(line) && matches!(line.figures.as_slice(), [part] if part.reference == figure.reference && usize::from(part.row) == index + 1)
    });
    complete.then_some(*figure)
}
