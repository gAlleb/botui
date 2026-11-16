//! Chapter boundaries in the source text and in the paginated book.
use super::{Anchor, Document, PageLine};
use std::collections::BTreeMap;

/// Source position within the active chapter, independent of page dimensions.
pub struct ChapterProgress<'a> {
    pub ordinal: usize,
    pub total: usize,
    pub title: Option<&'a str>,
    pub toc_title: Option<&'a str>,
    pub percent: Option<u8>,
}

struct Section {
    start: Anchor,
    title: Option<Box<str>>,
    toc_title: Option<Box<str>>,
}

#[derive(Default)]
pub(super) struct Progress {
    spine: crate::EpubSpine,
    sections: Vec<Section>,
    /// Cumulative UTF-8 bytes at the start of each source block, then at EOF.
    offsets: Vec<usize>,
}

impl Progress {
    pub fn new(doc: &Document) -> Self {
        let outline: BTreeMap<_, _> = doc
            .contents
            .iter()
            .filter_map(|entry| Some((entry.anchor()?, entry)))
            .collect();
        let starts = if doc.chapter_starts.is_empty() {
            // Markdown headings, for example, can all occupy the same source block.
            outline.keys().copied().collect()
        } else {
            doc.chapter_starts
                .iter()
                .map(|&block| Anchor { block, byte: 0 })
                .collect::<Vec<_>>()
        };
        let sections: Vec<_> = starts
            .into_iter()
            .filter_map(|start| {
                let block = doc.blocks.get(start.block)?;
                let mut heading = block.heading.then_some(block);
                let mut toc = outline.get(&start).copied();
                // In a numbered EPUB the page break can precede a standalone "3",
                // while its TOC destination "3. Anatomy" points at the next block.
                let next = Anchor {
                    block: start.block + 1,
                    byte: 0,
                };
                if toc.is_none()
                    && start.byte == 0
                    && !doc.chapter_starts.contains(&next.block)
                    && let Some(next_heading) = doc.blocks.get(next.block).filter(|b| b.heading)
                    && let Some(&next_entry) = outline.get(&next)
                    && next_entry.matches_numbered_title(&block.text, &next_heading.text)
                {
                    heading = Some(next_heading);
                    toc = Some(next_entry);
                }
                Some(Section {
                    start,
                    title: heading
                        .map(|b| b.text.trim())
                        .filter(|s| !s.is_empty())
                        .map(Into::into),
                    toc_title: toc
                        .map(|entry| entry.title.trim())
                        .filter(|s| !s.is_empty())
                        .map(Into::into),
                })
            })
            .collect();
        if sections.is_empty() && doc.epub_spine.is_empty() {
            return Self::default();
        }
        let mut offsets = Vec::with_capacity(doc.blocks.len() + 1);
        let mut bytes = 0;
        for block in &doc.blocks {
            offsets.push(bytes);
            bytes += block.text.len();
        }
        offsets.push(bytes);
        Self {
            spine: doc.epub_spine.clone(),
            sections,
            offsets,
        }
    }

    pub fn at(&self, mut anchor: Anchor, figure: Option<usize>) -> Option<ChapterProgress<'_>> {
        let spine = figure
            .and_then(|r| self.spine.position_at_reference(r))
            .or_else(|| self.spine.position_at_block(anchor.block));
        if let Some(item) = &spine {
            // A standalone illustration may share the preceding paragraph's anchor.
            // Its source file is known even when it contributes no text blocks.
            if item.blocks.is_empty() {
                return Some(ChapterProgress {
                    ordinal: item.ordinal,
                    total: item.total,
                    title: None,
                    toc_title: None,
                    percent: None,
                });
            }
            if anchor.block < item.blocks.start {
                anchor = Anchor {
                    block: item.blocks.start,
                    byte: 0,
                };
            } else if anchor.block >= item.blocks.end {
                anchor = Anchor {
                    block: item.blocks.end - 1,
                    byte: usize::MAX,
                };
            }
        }
        let next = self.sections.partition_point(|s| s.start <= anchor);
        let section = self.sections[..next].last();
        let (ordinal, total) = if let Some(item) = &spine {
            (item.ordinal, item.total)
        } else {
            section?;
            (next, self.sections.len())
        };
        // Chapter boundaries can cut across source files in either direction.
        let (start, end) = if let Some(section) = section {
            let end = match self.sections.get(next) {
                Some(section) => self.byte_offset(section.start)?,
                None => *self.offsets.last()?,
            };
            (self.byte_offset(section.start)?, end)
        } else {
            let range = spine?.blocks;
            (self.offsets[range.start], self.offsets[range.end])
        };
        let position = self.byte_offset(anchor)?;
        Some(ChapterProgress {
            ordinal,
            total,
            title: section.and_then(|s| s.title.as_deref()),
            toc_title: section.and_then(|s| s.toc_title.as_deref()),
            percent: (end > start)
                .then(|| (100 * (position - start) as u128 / (end - start) as u128) as u8),
        })
    }

    fn byte_offset(&self, anchor: Anchor) -> Option<usize> {
        let &[start, end, ..] = self.offsets.get(anchor.block..)? else {
            return self.offsets.last().copied();
        };
        Some(start + anchor.byte.min(end - start))
    }
}

pub(super) fn format(lines: Vec<PageLine>, doc: &Document, scrolling: bool) -> Vec<PageLine> {
    let blank = |line: &PageLine| {
        line.text.trim().is_empty() && line.figures.is_empty() && !line.has_visible_surface()
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
