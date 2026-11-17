//! Vim motions over visible UTF-8 text. Offsets always point at grapheme boundaries.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

fn words(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut start = None;
    text.char_indices()
        .chain(std::iter::once((text.len(), ' ')))
        .filter_map(move |(i, c)| {
            if c.is_whitespace() {
                start.take().map(|a| a..i)
            } else {
                start.get_or_insert(i);
                None
            }
        })
}

pub fn word(text: &str, at: usize, forward: bool, end: bool) -> Option<usize> {
    let mut positions = text.unicode_word_indices().filter_map(|(i, word)| {
        let pos = if end {
            i + word.grapheme_indices(true).next_back()?.0
        } else {
            i
        };
        (if forward { pos > at } else { pos < at }).then_some(pos)
    });
    if forward {
        positions.next()
    } else {
        positions.next_back()
    }
}

pub fn word_at(text: &str, at: usize, around: bool) -> Option<Range<usize>> {
    let (i, word) = text
        .split_word_bound_indices()
        .find(|(i, w)| i + w.len() > at)?;
    Some(around_word(text, i..i + word.len(), around))
}

pub fn big_word(text: &str, at: usize, forward: bool, end: bool) -> Option<usize> {
    let mut positions = words(text).filter_map(|r| {
        let pos = if end {
            r.start + text[r.clone()].grapheme_indices(true).next_back()?.0
        } else {
            r.start
        };
        (if forward { pos > at } else { pos < at }).then_some(pos)
    });
    if forward {
        positions.next()
    } else {
        positions.last()
    }
}

pub fn big_word_at(text: &str, at: usize, around: bool) -> Option<Range<usize>> {
    let whitespace = text.get(at..)?.chars().next()?.is_whitespace();
    let start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace() != whitespace)
        .map_or(0, |(i, c)| i + c.len_utf8());
    let end = text[at..]
        .char_indices()
        .find(|(_, c)| c.is_whitespace() != whitespace)
        .map_or(text.len(), |(i, _)| at + i);
    let range = if around && whitespace {
        start..words(&text[end..]).next().map_or(end, |r| end + r.end)
    } else {
        start..end
    };
    Some(around_word(text, range, around && !whitespace))
}

fn around_word(text: &str, mut range: Range<usize>, around: bool) -> Range<usize> {
    if around {
        let end = text[range.end..].len() - text[range.end..].trim_start().len();
        if end > 0 {
            range.end += end;
        } else {
            range.start = text[..range.start].trim_end().len();
        }
    }
    range
}

/// Find the Nth character on the supplied line. A failed count does not move.
/// A repeated t/T skips the adjacent match which would leave the cursor still.
#[allow(clippy::too_many_arguments)]
pub fn find_char(
    line: &str,
    at: usize,
    ch: char,
    forward: bool,
    till: bool,
    repeat: bool,
    count: usize,
) -> Option<usize> {
    let mut matches: Vec<_> = line
        .grapheme_indices(true)
        .filter(|(i, g)| (if forward { *i > at } else { *i < at }) && g.contains(ch))
        .filter_map(|(i, g)| {
            let target = if !till {
                i
            } else if forward {
                line[..i].grapheme_indices(true).next_back()?.0
            } else {
                i + g.len()
            };
            (!(repeat && till && target == at)).then_some(target)
        })
        .collect();
    if !forward {
        matches.reverse();
    }
    matches.get(count.checked_sub(1)?).copied()
}

/// Inclusive delimiter positions, shared by source-mapped text and fixed pages.
/// Bracket counts choose enclosing pairs from the innermost outwards.
pub fn pair_bounds<P: Copy + Ord>(
    chars: impl Iterator<Item = (P, char)>,
    at: P,
    open: char,
    close: char,
    count: usize,
) -> Option<(P, P)> {
    let mut stack = Vec::new();
    let mut escaped = false;
    let mut remaining = count.max(1);
    for (pos, ch) in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if open == close {
            if ch != open {
                continue;
            }
            if let Some(start) = stack.pop() {
                if pos >= at {
                    remaining -= 1;
                    if remaining == 0 {
                        return Some((start, pos));
                    }
                }
            } else {
                stack.push(pos);
            }
        } else if ch == open {
            stack.push(pos);
        } else if ch == close
            && let Some(start) = stack.pop()
            && start <= at
            && at <= pos
        {
            remaining -= 1;
            if remaining == 0 {
                return Some((start, pos));
            }
        }
    }
    None
}

pub fn delimited(
    text: &str,
    at: usize,
    open: char,
    close: char,
    around: bool,
    count: usize,
) -> Option<Range<usize>> {
    let quoted = open == close;
    let (a, b) = pair_bounds(
        text.char_indices(),
        at,
        open,
        close,
        if quoted { 1 } else { count },
    )?;
    // In Vim, 2i" includes the current quotes; it does not select a later pair.
    Some(if around || (quoted && count > 1) {
        a..b + close.len_utf8()
    } else {
        a + open.len_utf8()..b
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_keep_punctuation_and_unicode_graphemes() {
        let text = "one-two  Привет,мир! e\u{301}nd";
        assert_eq!(big_word(text, 0, true, false), Some(9));
        assert_eq!(big_word(text, 0, true, true), Some(6));
        let last = text.rfind('e').unwrap();
        assert_eq!(big_word(text, text.len(), false, false), Some(last));
        assert_eq!(&text[big_word_at(text, 11, false).unwrap()], "Привет,мир!");
        assert_eq!(&text[big_word_at(text, 0, true).unwrap()], "one-two  ");
    }

    #[test]
    fn counted_find_till_and_repeat_do_not_stall_or_partially_move() {
        let text = "a,b,c,d";
        assert_eq!(find_char(text, 0, ',', true, false, false, 2), Some(3));
        assert_eq!(find_char(text, 0, ',', true, true, false, 1), Some(0));
        assert_eq!(find_char(text, 0, ',', true, true, true, 1), Some(2));
        assert_eq!(find_char(text, 0, ',', true, false, false, 4), None);
        assert_eq!(find_char(text, 6, ',', false, true, false, 2), Some(4));
        assert_eq!(
            find_char("я e\u{301} я", 0, 'я', true, true, false, 1),
            Some(6)
        );
    }

    #[test]
    fn objects_are_nested_escaped_and_include_delimiters_only_when_requested() {
        let text = "before (outer [inner] tail) after";
        let at = text.find("inner").unwrap();
        assert_eq!(
            &text[delimited(text, at, '[', ']', false, 1).unwrap()],
            "inner"
        );
        assert_eq!(
            &text[delimited(text, at, '(', ')', true, 1).unwrap()],
            "(outer [inner] tail)"
        );
        let nested = "((one) two)";
        assert_eq!(
            &nested[delimited(nested, 2, '(', ')', false, 2).unwrap()],
            "(one) two"
        );
        let quoted = r#"before "say \"hello\" again" after"#;
        assert_eq!(
            &quoted[delimited(quoted, 10, '"', '"', false, 1).unwrap()],
            r#"say \"hello\" again"#
        );
        assert_eq!(delimited("no pair", 2, '(', ')', false, 1), None);
        assert_eq!(delimited("()", 0, '(', ')', false, 1), Some(1..1));
        let two_pairs = r#"before "one" and "two""#;
        assert_eq!(
            &two_pairs[delimited(two_pairs, 9, '"', '"', false, 2).unwrap()],
            r#""one""#
        );
    }
}
