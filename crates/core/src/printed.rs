//! Some converted ebooks have a paragraph per printed line. Keep their original
//! block/byte addresses while flowing likely prose across those line breaks.
use super::{Anchor, Block, PageLine, wrap_block};

pub(super) fn printed_lines(blocks: &[Block]) -> Vec<bool> {
    let mut found = vec![false; blocks.len()];
    for (start, window) in blocks.windows(16).enumerate() {
        let short = window
            .iter()
            .filter(|b| !b.heading && (20..=90).contains(&b.text.chars().count()))
            .count();
        let splits = window
            .windows(2)
            .filter(|pair| split_word(&pair[0].text, &pair[1].text))
            .count();
        if short >= 12 && splits >= 3 {
            found[start..start + window.len()].fill(true);
        }
    }
    found
}

fn split_word(left: &str, right: &str) -> bool {
    left.strip_suffix('-')
        .and_then(|s| s.chars().last())
        .is_some_and(char::is_alphabetic)
        && right.chars().next().is_some_and(char::is_lowercase)
}

pub(super) fn continues(left: &Block, right: &Block) -> bool {
    !left.heading
        && !right.heading
        && (split_word(&left.text, &right.text)
            || (left.text.chars().count() >= 26
                && right.text.starts_with(char::is_alphabetic)
                && !left.text.ends_with(['.', '!', '?', '…', ':'])))
}

pub(super) fn wrap(
    lines: &mut Vec<PageLine>,
    blocks: &[Block],
    start: usize,
    width: usize,
    indent: u8,
    resume: Option<Anchor>,
) {
    if blocks.len() == 1 && resume.is_none() {
        wrap_block(
            lines,
            &blocks[0],
            start,
            width,
            if blocks[0].heading { 0 } else { indent },
        );
        return;
    }
    let (mut text, offsets) = join(blocks);
    let before = lines.len();
    let heading = blocks[0].heading;
    let indent = if heading { 0 } else { indent };
    let cut = resume.and_then(|a| {
        let i = a.block.checked_sub(start)?;
        let source = blocks.get(i)?.text.get(a.byte..)?;
        let pos = offsets[i] + a.byte;
        (!source.is_empty() && pos > 0 && text.is_char_boundary(pos)).then_some(pos)
    });
    let suffix = cut.map(|pos| text.split_off(pos));
    wrap_block(lines, &Block { text, heading }, start, width, indent);
    if let (Some(byte), Some(text)) = (cut, suffix) {
        let after_prefix = lines.len();
        wrap_block(lines, &Block { text, heading }, start, width, 0);
        for line in &mut lines[after_prefix..] {
            line.start.byte += byte;
        }
    }
    for line in &mut lines[before..] {
        let byte = line.start.byte;
        let i = offsets.partition_point(|n| *n <= byte).saturating_sub(1);
        line.start = Anchor {
            block: start + i,
            byte: byte - offsets[i],
        };
    }
}

pub(super) fn join(blocks: &[Block]) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut offsets = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            if split_word(&text, &block.text) {
                text.pop();
            } else {
                text.push(' ');
            }
        }
        offsets.push(text.len());
        text.push_str(&block.text);
    }
    (text, offsets)
}

pub(super) fn end(blocks: &[Block], printed: &[bool], start: usize) -> usize {
    let mut end = start;
    while end + 1 < printed.len()
        && printed[end]
        && printed[end + 1]
        && continues(&blocks[end], &blocks[end + 1])
    {
        end += 1;
    }
    end
}
