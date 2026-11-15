//! Mark breaks before pagination, so changing the viewport only changes where the prior chapter ends.
use super::{Document, PageLine};

pub(super) fn format(lines: Vec<PageLine>, doc: &Document, scrolling: bool) -> Vec<PageLine> {
    let blank = |line: &PageLine| {
        line.text.trim().is_empty() && line.figures.is_empty() && line.surfaces.is_empty()
    };
    let mut result = Vec::with_capacity(lines.len());
    let mut last_block = None;
    let mut in_title = false;
    for mut line in lines {
        if blank(&line) {
            if !in_title {
                result.push(line);
            }
            continue;
        }
        let block = line.start.block;
        let has_text = !line.text.trim().is_empty();
        let starts = has_text && last_block != Some(block) && doc.chapter_starts.contains(&block);
        let title = has_text && doc.blocks.get(block).is_some_and(|b| b.heading);
        if starts {
            while result.last().is_some_and(blank) {
                result.pop();
            }
            if scrolling && let Some(last) = result.last() {
                let anchor = last.start;
                result.resize_with(result.len() + 2, || PageLine {
                    start: anchor,
                    ..Default::default()
                });
            }
            line.chapter_start = true;
            in_title = title;
        } else if in_title && !title {
            if let Some(last) = result.last() {
                result.push(PageLine {
                    start: last.start,
                    ..Default::default()
                });
            }
            in_title = false;
        }
        if has_text {
            last_block = Some(block);
        }
        result.push(line);
    }
    result
}
