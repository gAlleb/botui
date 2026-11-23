//! Word wrapping for ordinary prose with a forced reading boundary or dictionary hyphenation.
//! Structural EPUB rows keep their publisher layout.
use crate::{
    Anchor, Block, Document, PageLine, TextAlignment, Typography, breaks_word, publisher::TextSpan,
};
use std::ops::Range;
use unicode_width::UnicodeWidthStr;

fn pieces(text: &str) -> impl DoubleEndedIterator<Item = (usize, &str)> {
    text.split(breaks_word)
        .filter(|s| !s.is_empty())
        .map(move |s| (s.as_ptr() as usize - text.as_ptr() as usize, s))
}

fn split_word(word: &str, consumed: usize, space: usize) -> Option<usize> {
    let stem = word
        .trim_end_matches(|c: char| c.is_ascii_punctuation() || matches!(c, '»' | '”' | '…' | '—'));
    if space < 3 || stem.len() < 5 {
        return None;
    }
    let lang = if stem.bytes().all(|c| c.is_ascii_alphabetic()) {
        hypher::Lang::English
    } else if stem.chars().all(|c| matches!(c, 'А'..='я' | 'ё' | 'Ё')) {
        hypher::Lang::Russian
    } else {
        return None;
    };
    let mut offset = 0;
    hypher::hyphenate_bounded(stem, lang, 2, 2)
        .filter_map(|part| {
            offset += part.len();
            (offset > consumed && offset < stem.len() && stem[consumed..offset].width() < space)
                .then_some(offset.saturating_sub(consumed))
        })
        .last()
}

struct Row {
    words: Vec<Range<usize>>,
    indent: u16,
    hyphen: bool,
}
impl Row {
    fn new(indent: u16) -> Self {
        Self {
            words: Vec::new(),
            indent,
            hyphen: false,
        }
    }
}

fn wrap(
    text: &str,
    start: usize,
    width: usize,
    indent: u16,
    hyphens: bool,
    out: &mut Vec<Row>,
) -> Option<()> {
    let mut row = Row::new(indent);
    let mut used = usize::from(indent);
    for (index, word) in pieces(text) {
        let mut offset = 0;
        while offset < word.len() {
            let rest = &word[offset..];
            let space = width.saturating_sub(used + usize::from(!row.words.is_empty()));
            let begin = start + index + offset;
            if rest.width() <= space {
                used += usize::from(!row.words.is_empty()) + rest.width();
                row.words.push(begin..begin + rest.len());
                break;
            }
            if let Some(cut) = hyphens.then(|| split_word(word, offset, space)).flatten() {
                row.words.push(begin..begin + cut);
                row.hyphen = true;
                offset += cut;
            } else if row.words.is_empty() {
                return None;
            }
            out.push(row);
            row = Row::new(0);
            used = 0;
        }
    }
    if !row.words.is_empty() {
        out.push(row);
    }
    Some(())
}

pub(super) fn layout(
    lines: &[PageLine],
    block: &Block,
    width: u16,
    style: Typography,
    anchor: Option<Anchor>,
) -> Option<Vec<PageLine>> {
    let first = lines.first()?;
    let index = first.start.block;
    // Preserve explicit line breaks and non-prose positioning (e.g. centered verse).
    let publisher_positions = style.alignment == TextAlignment::Book
        && lines
            .iter()
            .skip(1)
            .any(|line| line.indent > 0 || line.text.starts_with(' '));
    if block.heading
        || publisher_positions
        || block.text.contains(['\n', '\r'])
        || lines.iter().any(|l| {
            !l.figures.is_empty()
                || !l.surfaces.is_empty()
                || l.equation.1 > 1
                || l.table_header
                || l.spans.iter().any(|s| {
                    !matches!(s.face.role, 0 | 7) || s.source.is_some_and(|a| a.block != index)
                })
        })
    {
        return None;
    }
    // A list marker, explicit line break, table cell, or generated material must
    // not disappear merely because its text shares a source paragraph.
    let plain = |c: &char| !breaks_word(*c);
    if lines
        .iter()
        .flat_map(|l| l.text.chars())
        .filter(plain)
        .ne(block.text.chars().filter(plain))
    {
        return None;
    }
    let mut rows = Vec::new();
    let cut = anchor
        .filter(|a| {
            a.block == index
                && a.byte > 0
                && a.byte < block.text.len()
                && block
                    .text
                    .get(..a.byte)
                    .and_then(|s| s.chars().last())
                    .is_some_and(breaks_word)
        })
        .map(|a| a.byte);
    if cut.is_none() && !style.hyphenation {
        return None;
    }
    let (prefix, suffix) = block.text.split_at_checked(cut.unwrap_or(0))?;
    let width = usize::from(width);
    let indent = if matches!(
        style.alignment,
        TextAlignment::Center | TextAlignment::Right
    ) {
        0
    } else {
        first
            .indent
            .saturating_add((first.text.len() - first.text.trim_start_matches(' ').len()) as u16)
    };
    if cut.is_some() {
        // Work backwards from the forced break. The short line, if any, is at
        // the start of the paragraph rather than dangling above the reading point.
        let mut row = Row::new(0);
        let mut used = 0;
        for (i, word) in pieces(prefix).rev() {
            let cells = word.width();
            if cells > width {
                return None;
            }
            if used
                + usize::from(!row.words.is_empty())
                + cells
                + if i == 0 { usize::from(indent) } else { 0 }
                > width
                && !row.words.is_empty()
            {
                row.words.reverse();
                rows.push(row);
                row = Row::new(0);
                used = 0;
            }
            used += cells + usize::from(!row.words.is_empty());
            row.words.push(i..i + word.len());
        }
        if !row.words.is_empty() {
            row.words.reverse();
            row.indent = indent;
            rows.push(row);
        }
        rows.reverse();
    }
    wrap(
        suffix,
        prefix.len(),
        width,
        if cut.is_some() { 0 } else { indent },
        style.hyphenation,
        &mut rows,
    )?;
    if cut.is_none() && !rows.iter().any(|row| row.hyphen) {
        return None;
    }
    let mut faces: Vec<_> = lines
        .iter()
        .flat_map(|l| &l.spans)
        .filter_map(|s| Some((s.source?.byte..s.source_end, s.face)))
        .collect();
    faces.sort_by_key(|(r, _)| r.start);
    let count = rows.len();
    let mut output = Vec::with_capacity(count);
    for (i, row) in rows.into_iter().enumerate() {
        let mut line = PageLine {
            start: Anchor {
                block: index,
                byte: row.words.first()?.start,
            },
            indent: row.indent,
            chapter_start: i == 0 && first.chapter_start,
            ..Default::default()
        };
        let mut previous_end = None;
        for word in row.words {
            if let Some(byte) = previous_end {
                let space = line.text.len();
                line.spans.push(TextSpan {
                    bytes: space..space + 1,
                    source: Some(Anchor { block: index, byte }),
                    source_end: word.start,
                    face: Default::default(),
                });
                line.text.push(' ');
            }
            previous_end = Some(word.end);
            let origin = line.text.len();
            line.text.push_str(&block.text[word.clone()]);
            let mut pos = word.start;
            while pos < word.end {
                let next = faces.partition_point(|(r, _)| r.end <= pos);
                let (end, face) = match faces.get(next) {
                    Some((range, face)) if range.contains(&pos) => (range.end.min(word.end), *face),
                    Some((range, _)) if range.start > pos => {
                        (range.start.min(word.end), Default::default())
                    }
                    _ => (word.end, Default::default()),
                };
                line.spans.push(TextSpan {
                    bytes: origin + pos - word.start..origin + end - word.start,
                    source: Some(Anchor {
                        block: index,
                        byte: pos,
                    }),
                    source_end: end,
                    face,
                });
                pos = end;
            }
        }
        if row.hyphen {
            line.text.push('-');
        }
        let spare = width.saturating_sub(usize::from(line.indent) + line.text.width());
        match style.alignment {
            TextAlignment::Center => line.indent = (spare / 2) as u16,
            TextAlignment::Right => line.indent = spare as u16,
            TextAlignment::Justify if i + 1 < count => crate::printed::justify(&mut line, spare),
            _ => (),
        }
        output.push(line);
    }
    Some(output)
}

pub(super) fn rewrap(
    lines: Vec<PageLine>,
    doc: &Document,
    width: u16,
    style: Typography,
    anchor: Option<Anchor>,
) -> Vec<PageLine> {
    if width == 0
        || (!style.hyphenation && anchor.is_none_or(|a| a.byte == 0))
        || doc.markdown.is_some()
    {
        return lines;
    }
    let mut input = lines.into_iter().peekable();
    let mut out = Vec::new();
    while let Some(line) = input.next() {
        let index = line.start.block;
        if line.text.trim().is_empty()
            || (!style.hyphenation && anchor.is_none_or(|a| a.block != index))
        {
            out.push(line);
            continue;
        }
        let mut group = vec![line];
        while input
            .peek()
            .is_some_and(|l| l.start.block == index && !l.text.trim().is_empty())
        {
            group.push(input.next().unwrap());
        }
        let replacement = doc
            .blocks
            .get(index)
            .and_then(|block| layout(&group, block, width, style, anchor));
        out.extend(replacement.unwrap_or(group));
    }
    out
}
