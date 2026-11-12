//! Selection addresses source text, never wrapped rows or terminal columns.
use crate::{Anchor, Document, PageLine, Pager};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextRange {
    pub start: Anchor,
    /// Exclusive UTF-8 boundary.
    pub end: Anchor,
}
impl TextRange {
    pub fn new(a: Anchor, b: Anchor) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }
    pub fn union(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
    pub fn intersects(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
    pub fn contains(self, at: Anchor) -> bool {
        self.start <= at && at < self.end
    }
}

#[derive(Clone, Debug)]
pub struct TextCell {
    pub columns: Range<usize>,
    pub source: TextRange,
}

impl Pager {
    pub fn text_line_range(&mut self, doc: &Document, at: Anchor) -> Option<TextRange> {
        self.finish_resize();
        let row = Self::line_at(at, &self.lines, &self.source_lines);
        self.lines
            .get(row)?
            .text_cells(doc)
            .into_iter()
            .map(|c| c.source)
            .reduce(TextRange::union)
    }
    /// Cursor motion follows laid-out text rows, across column and page breaks.
    /// A preferred column survives short lines; images and paragraph gaps are skipped.
    pub fn move_text_line(
        &mut self,
        doc: &Document,
        at: Anchor,
        down: bool,
        preferred: Option<usize>,
    ) -> (Anchor, usize) {
        self.finish_resize();
        let row = Self::line_at(at, &self.lines, &self.source_lines);
        let column = preferred.unwrap_or_else(|| {
            self.lines
                .get(row)
                .and_then(|line| {
                    line.text_cells(doc)
                        .into_iter()
                        .find(|c| c.source.contains(at))
                })
                .map_or(0, |c| c.columns.start)
        });
        let rows: Box<dyn Iterator<Item = usize>> = if down {
            Box::new(row.saturating_add(1)..self.lines.len())
        } else {
            Box::new((0..row).rev())
        };
        for row in rows {
            if let Some(cell) = self.lines[row]
                .text_cells(doc)
                .into_iter()
                .min_by_key(|c| c.columns.start.abs_diff(column))
            {
                return (cell.source.start, column);
            }
        }
        (at, column)
    }
}

impl Document {
    pub fn text_range(&self, range: TextRange) -> String {
        if let Some(md) = &self.markdown
            && !md.show_source
        {
            let raw = &self.blocks[0].text;
            let mut result = String::new();
            let mut previous = None;
            for passage in &md.passages {
                let start = passage.start.byte.max(range.start.byte);
                let end = passage.end.min(range.end.byte);
                if start >= end {
                    continue;
                }
                if let Some(last) = previous {
                    let gap = &raw[last..start];
                    if gap.contains('\n') {
                        result.push('\n');
                    }
                }
                if let Some(text) = &passage.display {
                    result.push_str(text);
                } else if let Some(text) = raw.get(start..end) {
                    result.push_str(text);
                }
                previous = Some(end);
            }
            return result;
        }
        let mut parts = Vec::new();
        for block in range.start.block..=range.end.block.min(self.blocks.len().saturating_sub(1)) {
            let Some(text) = self.blocks.get(block).map(|b| b.text.as_str()) else {
                break;
            };
            let start = if block == range.start.block {
                range.start.byte
            } else {
                0
            };
            let end = if block == range.end.block {
                range.end.byte
            } else {
                text.len()
            };
            if let Some(s) = text.get(start..end)
                && !s.is_empty()
            {
                parts.push(s);
            }
        }
        parts.join("\n\n")
    }
    pub fn character_at(&self, at: Anchor) -> Option<TextRange> {
        if let Some(md) = self.markdown.as_ref().filter(|m| !m.show_source) {
            let pos = md.display_position(at.byte);
            let (i, g) = md
                .text
                .grapheme_indices(true)
                .find(|(i, g)| i + g.len() > pos)?;
            return md.source_range(i..i + g.len());
        }
        let text = &self.blocks.get(at.block)?.text;
        let (byte, g) = text
            .grapheme_indices(true)
            .find(|(i, g)| *i + g.len() > at.byte)?;
        Some(TextRange::new(
            Anchor { byte, ..at },
            Anchor {
                byte: byte + g.len(),
                ..at
            },
        ))
    }
    pub fn word_at(&self, at: Anchor) -> Option<TextRange> {
        if let Some(md) = self.markdown.as_ref().filter(|m| !m.show_source) {
            let pos = md.display_position(at.byte);
            let (i, w) = md
                .text
                .split_word_bound_indices()
                .find(|(i, w)| i + w.len() > pos)?;
            return md.source_range(i..i + w.len());
        }
        let text = &self.blocks.get(at.block)?.text;
        let (byte, word) = text
            .split_word_bound_indices()
            .find(|(i, w)| *i + w.len() > at.byte)?;
        Some(TextRange::new(
            Anchor { byte, ..at },
            Anchor {
                byte: byte + word.len(),
                ..at
            },
        ))
    }
    pub fn paragraph_at(&self, at: Anchor) -> Option<TextRange> {
        if let Some(md) = &self.markdown {
            let p = md
                .paragraphs
                .iter()
                .filter(|p| p.contains(&at.byte))
                .min_by_key(|p| p.len())
                .or_else(|| md.paragraphs.iter().find(|p| p.end > at.byte))?;
            if md.show_source {
                return Some(TextRange::new(
                    Anchor {
                        byte: p.start,
                        ..at
                    },
                    Anchor { byte: p.end, ..at },
                ));
            }
            let a = md.mapping.partition_point(|(_, r)| r.end <= p.start);
            let b = md
                .mapping
                .partition_point(|(_, r)| r.start < p.end)
                .checked_sub(1)?;
            return Some(TextRange::new(
                Anchor {
                    byte: md.mapping.get(a)?.1.start,
                    ..at
                },
                Anchor {
                    byte: md.mapping.get(b)?.1.end,
                    ..at
                },
            ));
        }
        let text = &self.blocks.get(at.block)?.text;
        Some(TextRange::new(
            Anchor { byte: 0, ..at },
            Anchor {
                byte: text.len(),
                ..at
            },
        ))
    }
    pub fn move_paragraph(&self, at: Anchor, forward: bool) -> Anchor {
        if let Some(md) = &self.markdown {
            let current = self.paragraph_at(at).map_or(at.byte, |r| r.start.byte);
            let mut starts: Vec<_> = md
                .paragraphs
                .iter()
                .filter_map(|p| {
                    self.paragraph_at(Anchor {
                        block: 0,
                        byte: p.start,
                    })
                    .map(|r| r.start)
                })
                .collect();
            starts.sort();
            starts.dedup();
            return if forward {
                starts.into_iter().find(|a| a.byte > at.byte)
            } else {
                starts.into_iter().rev().find(|a| a.byte < current)
            }
            .unwrap_or(at);
        }
        let block = if forward {
            ((at.block + 1)..self.blocks.len()).find(|&i| !self.blocks[i].text.is_empty())
        } else {
            (0..at.block)
                .rev()
                .find(|&i| !self.blocks[i].text.is_empty())
        };
        block.map_or(at, |block| Anchor { block, byte: 0 })
    }
    pub fn word_end(&self, at: Anchor) -> Anchor {
        if let Some(md) = self.markdown.as_ref().filter(|m| !m.show_source) {
            let pos = md.display_position(at.byte);
            return md
                .text
                .unicode_word_indices()
                .filter_map(|(i, w)| {
                    let end = i + w.grapheme_indices(true).next_back()?.0;
                    (end > pos)
                        .then(|| md.source_range(end..i + w.len()).map(|r| r.start))
                        .flatten()
                })
                .next()
                .unwrap_or(at);
        }
        for block in at.block..self.blocks.len() {
            for (offset, word) in self.blocks[block].text.unicode_word_indices() {
                let byte = offset + word.grapheme_indices(true).next_back().unwrap().0;
                let end = Anchor { block, byte };
                if end > at {
                    return end;
                }
            }
        }
        at
    }
    /// Move between graphemes or word starts, crossing paragraph boundaries.
    pub fn move_text(&self, at: Anchor, forward: bool, word: bool) -> Anchor {
        if let Some(md) = self.markdown.as_ref().filter(|m| !m.show_source) {
            let pos = md.display_position(at.byte);
            let positions: Box<dyn Iterator<Item = (usize, &str)> + '_> = if word {
                Box::new(md.text.unicode_word_indices())
            } else {
                Box::new(md.text.grapheme_indices(true))
            };
            let mut anchors = positions
                .filter(|(i, _)| if forward { *i > pos } else { *i < pos })
                .filter_map(|(i, s)| md.source_range(i..i + s.len()).map(|r| r.start));
            return if forward {
                anchors.next()
            } else {
                anchors.last()
            }
            .unwrap_or(at);
        }
        let Some(block) = self.blocks.get(at.block) else {
            return at;
        };
        let mut positions: Box<dyn Iterator<Item = usize> + '_> = if word {
            Box::new(block.text.unicode_word_indices().map(|(i, _)| i))
        } else {
            Box::new(block.text.grapheme_indices(true).map(|(i, _)| i))
        };
        let next = if forward {
            positions.find(|i| *i > at.byte)
        } else {
            positions.filter(|i| *i < at.byte).last()
        };
        if let Some(byte) = next {
            return Anchor { byte, ..at };
        }
        if forward {
            for block in at.block + 1..self.blocks.len() {
                let text = &self.blocks[block].text;
                let byte = if word {
                    text.unicode_word_indices().next().map(|(i, _)| i)
                } else {
                    (!text.is_empty()).then_some(0)
                };
                if let Some(byte) = byte {
                    return Anchor { block, byte };
                }
            }
        } else {
            for block in (0..at.block).rev() {
                let text = &self.blocks[block].text;
                let byte = if word {
                    text.unicode_word_indices().next_back().map(|(i, _)| i)
                } else {
                    text.grapheme_indices(true).next_back().map(|(i, _)| i)
                };
                if let Some(byte) = byte {
                    return Anchor { block, byte };
                }
            }
        }
        at
    }
}

impl PageLine {
    /// Only built while interacting with text or painting annotations. Ordinary
    /// reading does not need a per-cell map and keeps the existing fast path.
    pub fn text_cells(&self, doc: &Document) -> Vec<TextCell> {
        let mut cells = Vec::new();
        let mut column = 0;
        let mut plain = self.anchor();
        for (byte, g) in self.text.grapheme_indices(true) {
            let width = g.width();
            let columns = column..column + width;
            column += width;
            let source = if self.spans.is_empty() {
                // Plain wrapping collapses whitespace and can join printed lines.
                // Walk the source once; do not assume displayed byte offsets match.
                next_plain(doc, &mut plain, g)
            } else {
                self.spans
                    .iter()
                    .find(|s| s.bytes.contains(&byte))
                    .and_then(|span| {
                        let at = span.source?;
                        let end = span.source_end;
                        let source = doc.blocks.get(at.block)?.text.get(at.byte..end)?;
                        let rendered = self.text.get(span.bytes.clone())?;
                        let offset = byte - span.bytes.start;
                        let range = if rendered == source {
                            at.byte + offset..at.byte + offset + g.len()
                        } else if doc.markdown.is_some() {
                            at.byte..end
                        } else {
                            // CSS transformations can change UTF-8 length. Match grapheme
                            // ordinals, leaving synthetic layout padding unselectable.
                            let index = rendered.get(..offset)?.graphemes(true).count();
                            let (i, glyph) = source.grapheme_indices(true).nth(index)?;
                            at.byte + i..at.byte + i + glyph.len()
                        };
                        Some(TextRange::new(
                            Anchor {
                                byte: range.start,
                                ..at
                            },
                            Anchor {
                                byte: range.end,
                                ..at
                            },
                        ))
                    })
            };
            if let Some(source) = source
                && width > 0
            {
                cells.push(TextCell { columns, source });
            }
        }
        cells
    }
}

fn next_plain(doc: &Document, at: &mut Anchor, rendered: &str) -> Option<TextRange> {
    loop {
        let text = &doc.blocks.get(at.block)?.text;
        if at.byte >= text.len() {
            if rendered.trim().is_empty() {
                return None;
            }
            at.block += 1;
            at.byte = 0;
            continue;
        }
        let tail = text.get(at.byte..)?;
        let g = tail.graphemes(true).next()?;
        let start = *at;
        if g.trim().is_empty() {
            at.byte += tail.len() - tail.trim_start().len();
            if rendered.trim().is_empty() {
                return Some(TextRange::new(start, *at));
            }
            continue;
        }
        // A mismatch means generated material, not permission to select unrelated text.
        if g != rendered {
            return None;
        }
        at.byte += g.len();
        return Some(TextRange::new(start, *at));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, LayoutMode, Pager};
    fn doc() -> Document {
        Document {
            blocks: vec![
                Block {
                    text: "Привет  мир e\u{301} 中文 🦉!".into(),
                    heading: false,
                },
                Block {
                    text: "Второй абзац".into(),
                    heading: false,
                },
            ],
            ..Default::default()
        }
    }
    #[test]
    fn ranges_copy_source_across_paragraphs() {
        let d = doc();
        let range = TextRange::new(
            Anchor {
                block: 0,
                byte: "Привет  ".len(),
            },
            Anchor {
                block: 1,
                byte: "Второй".len(),
            },
        );
        assert_eq!(d.text_range(range), "мир e\u{301} 中文 🦉!\n\nВторой");
        let at = Anchor {
            block: 0,
            byte: "Привет  мир ".len() + 1,
        };
        assert_eq!(d.text_range(d.character_at(at).unwrap()), "e\u{301}");
        let last = d.move_text(Anchor { block: 1, byte: 0 }, false, true);
        assert!(d.text_range(d.word_at(last).unwrap()).contains('文'));
        assert_eq!(
            d.word_end(Anchor {
                block: 0,
                byte: "Привет".len()
            }),
            Anchor {
                block: 0,
                byte: "Привет  ми".len()
            }
        );
    }
    #[test]
    fn mapped_cells_survive_reflow_without_copying_added_spaces() {
        let d = doc();
        for width in [32, 46, 80] {
            let p = Pager::new(&d, width, 20, LayoutMode::Single);
            for row in p.viewport_rows().lines(0) {
                for cell in row.text_cells(&d) {
                    let text = d.text_range(cell.source);
                    assert!(!text.is_empty());
                    if text == "中" {
                        assert_eq!(cell.columns.len(), 2);
                    }
                }
            }
        }
        let at = Anchor { block: 0, byte: 0 };
        assert_eq!(d.move_text(at, true, true).byte, "Привет  ".len());
        assert_eq!(
            d.word_at(d.move_text(at, true, true))
                .map(|r| d.text_range(r)),
            Some("мир".into())
        );
    }

    #[test]
    fn vertical_cursor_crosses_pages_and_keeps_column_after_short_lines() {
        let d = Document {
            blocks: vec![
                Block {
                    text: "0123456789 beginning of a longer paragraph to wrap".into(),
                    heading: false,
                },
                Block {
                    text: "短".into(),
                    heading: false,
                },
                Block {
                    text: "0123456789 another longer paragraph to continue".into(),
                    heading: false,
                },
            ],
            ..Default::default()
        };
        for mode in [LayoutMode::Single, LayoutMode::Spread, LayoutMode::Scroll] {
            let mut p = Pager::new(&d, 90, 2, mode);
            let start = Anchor { block: 0, byte: 8 };
            let mut at = start;
            let mut preferred = None;
            let mut visited = vec![at];
            loop {
                let (next, column) = p.move_text_line(&d, at, true, preferred);
                preferred = Some(column);
                if next == at {
                    break;
                }
                at = next;
                visited.push(at);
            }
            assert!(visited.iter().any(|at| at.block == 1 && at.byte == 0));
            assert!(visited.iter().any(|at| at.block == 2 && at.byte == 8));
            for expected in visited[..visited.len() - 1].iter().rev() {
                (at, _) = p.move_text_line(&d, at, false, preferred);
                assert_eq!(at, *expected);
            }
        }
    }
}
