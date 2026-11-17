//! Keep Vim's visible-text positions mapped to the document's stable source anchors.
use super::text;
use botui_core::{Anchor, Document, TextRange};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

fn visible(doc: &Document, at: Anchor) -> (&str, usize) {
    if let Some(md) = doc.markdown.as_ref().filter(|m| !m.show_source) {
        (&md.text, md.display_position(at.byte))
    } else {
        (&doc.blocks[at.block].text, at.byte)
    }
}

fn source_range(doc: &Document, block: usize, bytes: Range<usize>) -> Option<TextRange> {
    if let Some(md) = doc.markdown.as_ref().filter(|m| !m.show_source) {
        if bytes.is_empty() {
            let at = source_position(doc, block, bytes.start)?;
            return Some(TextRange::new(at, at));
        }
        md.source_range(bytes)
    } else {
        Some(TextRange::new(
            Anchor {
                block,
                byte: bytes.start,
            },
            Anchor {
                block,
                byte: bytes.end,
            },
        ))
    }
}

fn source_position(doc: &Document, block: usize, byte: usize) -> Option<Anchor> {
    if let Some(md) = doc.markdown.as_ref().filter(|m| !m.show_source) {
        if byte == md.text.len() {
            return md.source_range(0..byte).map(|r| r.end);
        }
        let len = md.text.get(byte..)?.graphemes(true).next()?.len();
        md.source_range(byte..byte + len).map(|r| r.start)
    } else {
        Some(Anchor { block, byte })
    }
}

pub fn big_word(doc: &Document, at: Anchor, forward: bool, end: bool) -> Option<Anchor> {
    let (text, byte) = visible(doc, at);
    if let Some(next) = text::big_word(text, byte, forward, end) {
        return source_position(doc, at.block, next);
    }
    if doc.markdown.as_ref().is_some_and(|m| !m.show_source) {
        return None;
    }
    let blocks: Box<dyn Iterator<Item = usize>> = if forward {
        Box::new(at.block + 1..doc.blocks.len())
    } else {
        Box::new((0..at.block).rev())
    };
    for block in blocks {
        let text = &doc.blocks[block].text;
        let next = if forward {
            text::big_word_at(text, text.len() - text.trim_start().len(), false).map(|r| {
                if end {
                    r.start
                        + text[r.clone()]
                            .grapheme_indices(true)
                            .next_back()
                            .unwrap()
                            .0
                } else {
                    r.start
                }
            })
        } else {
            text::big_word(text, text.len(), false, false)
        };
        if let Some(byte) = next {
            return Some(Anchor { block, byte });
        }
    }
    None
}

pub fn big_word_at(doc: &Document, at: Anchor, around: bool) -> Option<TextRange> {
    let (text, byte) = visible(doc, at);
    source_range(doc, at.block, text::big_word_at(text, byte, around)?)
}

#[allow(clippy::too_many_arguments)]
pub fn find_char(
    doc: &Document,
    line: TextRange,
    at: Anchor,
    ch: char,
    forward: bool,
    till: bool,
    repeat: bool,
    count: usize,
) -> Option<Anchor> {
    if line.start.block != line.end.block || at.block != line.start.block {
        return None;
    }
    let (text, byte) = visible(doc, at);
    let (_, start) = visible(doc, line.start);
    let (_, end) = visible(doc, line.end);
    let next = text::find_char(
        text.get(start..end)?,
        byte.checked_sub(start)?,
        ch,
        forward,
        till,
        repeat,
        count,
    )?;
    source_position(doc, at.block, start + next)
}

pub fn delimited(
    doc: &Document,
    at: Anchor,
    open: char,
    close: char,
    around: bool,
    count: usize,
) -> Option<TextRange> {
    if doc.markdown.as_ref().is_some_and(|m| !m.show_source) || open == close {
        let (text, byte) = visible(doc, at);
        return source_range(
            doc,
            at.block,
            text::delimited(text, byte, open, close, around, count)?,
        );
    }
    // Brackets may span paragraphs, but an unmatched pair cannot capture another chapter.
    let first = doc
        .chapter_starts
        .range(..=at.block)
        .next_back()
        .copied()
        .unwrap_or(0);
    let last = doc
        .chapter_starts
        .range(at.block + 1..)
        .next()
        .copied()
        .unwrap_or(doc.blocks.len());
    let chars = (first..last).flat_map(|block| {
        doc.blocks[block]
            .text
            .char_indices()
            .map(move |(byte, ch)| (Anchor { block, byte }, ch))
    });
    let (a, b) = text::pair_bounds(chars, at, open, close, count)?;
    Some(if around {
        TextRange::new(a, doc.character_at(b)?.end)
    } else {
        TextRange::new(doc.character_at(a)?.end, b)
    })
}
