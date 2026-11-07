//! Search the same logical paragraphs the reader displays, even across print-line breaks.
use super::{Anchor, Document, PageLine, Pager, printed};
use std::ops::Range;

struct Paragraph {
    start: usize,
    offsets: Vec<usize>,
    text: String,
    folded: String,
}
pub struct SearchIndex(Vec<Paragraph>);
impl Pager {
    pub fn search_index(&self, doc: &Document) -> SearchIndex {
        let mut rows = vec![];
        let mut start = 0;
        while start < doc.blocks.len() {
            let end = printed::end(&doc.blocks, &self.printed, start);
            let (text, offsets) = printed::join(&doc.blocks[start..=end]);
            let folded = text.to_lowercase();
            rows.push(Paragraph {
                start,
                offsets,
                text,
                folded,
            });
            start = end + 1;
        }
        SearchIndex(rows)
    }
}
/// Source offsets in a logical paragraph are stable when the reader changes its width.
#[derive(Clone, Debug)]
pub struct SearchMatch {
    pub anchor: Anchor,
    paragraph: usize,
    bytes: Range<usize>,
}

impl SearchIndex {
    pub fn matches(&self, query: &str) -> Vec<SearchMatch> {
        let mut hits = Vec::new();
        if query.is_empty() {
            return hits;
        }
        let q = query.to_lowercase();
        for (paragraph, p) in self.0.iter().enumerate() {
            // Case conversion can expand one source character into multiple code points.
            // Walk source and folded bytes together, once per paragraph.
            let mut source = p
                .text
                .char_indices()
                .scan(0, |end, (i, ch)| {
                    *end += ch.to_lowercase().map(char::len_utf8).sum::<usize>();
                    Some((*end, i..i + ch.len_utf8()))
                })
                .peekable();
            let mut source_char = |offset| {
                while source.peek().is_some_and(|(end, _)| *end <= offset) {
                    source.next();
                }
                source.peek().map(|(_, bytes)| bytes.clone())
            };
            for (pos, matched) in p.folded.match_indices(&q) {
                let Some(first) = source_char(pos) else { break };
                let Some(last) = source_char(pos + matched.len() - 1) else {
                    break;
                };
                let start = first.start;
                let part = p.offsets.partition_point(|n| *n <= start).saturating_sub(1);
                hits.push(SearchMatch {
                    anchor: Anchor {
                        block: p.start + part,
                        byte: start - p.offsets[part],
                    },
                    paragraph,
                    bytes: start..last.end,
                });
                if hits.len() == 5000 {
                    return hits;
                }
            }
        }
        hits
    }

    pub fn excerpt(&self, hit: &SearchMatch) -> String {
        self.0[hit.paragraph].text[hit.bytes.start..]
            .chars()
            .take(120)
            .collect()
    }

    pub fn find(&self, query: &str) -> Vec<(Anchor, String)> {
        self.matches(query)
            .iter()
            .map(|hit| (hit.anchor, self.excerpt(hit)))
            .collect()
    }

    /// Map the part of this occurrence visible on a rendered line back to its UTF-8 bytes.
    pub fn visible_range(&self, hit: &SearchMatch, line: &PageLine) -> Option<Range<usize>> {
        let p = self.0.get(hit.paragraph)?;
        let part = line.start.block.checked_sub(p.start)?;
        let from = p.offsets.get(part)?.checked_add(line.start.byte)?;
        let to = from.checked_add(line.text.len())?;
        let start = hit.bytes.start.max(from);
        let end = hit.bytes.end.min(to);
        if start < end && p.text.get(from..to) == Some(&line.text) {
            Some(start - from..end - from)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, LayoutMode};
    #[test]
    fn each_occurrence_keeps_its_utf8_offset_after_case_folding() {
        let text = "İ · Amber and AMBER";
        let d = Document {
            blocks: vec![Block {
                text: text.into(),
                heading: false,
            }],
            ..Default::default()
        };
        let hits = Pager::new(&d, 40, 10, LayoutMode::Single)
            .search_index(&d)
            .find("amber");
        assert_eq!(
            hits.iter().map(|(a, _)| a.byte).collect::<Vec<_>>(),
            vec![text.find("Amber").unwrap(), text.find("AMBER").unwrap()]
        );
    }
    #[test]
    fn selected_phrase_highlights_both_wrapped_lines_after_case_expansion() {
        let doc = Document {
            blocks: vec![Block {
                text: "İ foo BÉTA amber end".into(),
                heading: false,
            }],
            ..Default::default()
        };
        let mut pager = Pager::new(&doc, 10, 10, LayoutMode::Single);
        let index = pager.search_index(&doc);
        let hit = &index.matches("béta AMBER")[0];
        for width in [10, 7, 26] {
            pager.relayout(&doc, width, 10, LayoutMode::Single);
            let pieces: Vec<_> = pager
                .lines(0)
                .iter()
                .filter_map(|line| {
                    index
                        .visible_range(hit, line)
                        .map(|range| &line.text[range])
                })
                .collect();
            assert_eq!(pieces.join(" "), "BÉTA amber");
        }
    }

    #[test]
    fn word_split_by_printed_line_finds_its_original_bookmark_address() {
        let doc = Document {
            blocks: (0..12)
                .flat_map(|_| {
                    [
                        "Женщина рассказала, что версия очень по-",
                        "хожа на другую известную версию.",
                    ]
                })
                .map(|s| Block {
                    text: s.into(),
                    heading: false,
                })
                .collect(),
            ..Default::default()
        };
        let index = |repair| {
            Pager::with_reflow(&doc, 100, 30, LayoutMode::Spread, repair).search_index(&doc)
        };
        let hits = index(true).find("ПОХОЖА на");
        assert_eq!(
            hits[0].0,
            Anchor {
                block: 0,
                byte: doc.blocks[0].text.find("по-").unwrap()
            }
        );
        assert!(hits[0].1.starts_with("похожа на"));
        assert_eq!(
            index(true).find("другую")[0].0,
            Anchor {
                block: 1,
                byte: doc.blocks[1].text.find("другую").unwrap()
            }
        );
        assert!(index(false).find("похожа").is_empty());
    }
}
