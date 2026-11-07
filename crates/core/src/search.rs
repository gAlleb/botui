//! Search the same logical paragraphs the reader displays, even across print-line breaks.
use super::{Anchor, Document, Pager, printed};

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
impl SearchIndex {
    pub fn find(&self, query: &str) -> Vec<(Anchor, String)> {
        if query.is_empty() {
            return vec![];
        }
        let q = query.to_lowercase();
        let mut hits = Vec::new();
        for p in &self.0 {
            let mut original = p.text.char_indices();
            let mut byte = 0;
            let mut folded_end = 0;
            for (pos, _) in p.folded.match_indices(&q) {
                while folded_end <= pos {
                    let Some((offset, c)) = original.next() else {
                        break;
                    };
                    byte = offset;
                    folded_end += c.to_lowercase().map(char::len_utf8).sum::<usize>();
                }
                let part = p.offsets.partition_point(|n| *n <= byte).saturating_sub(1);
                hits.push((
                    Anchor {
                        block: p.start + part,
                        byte: byte - p.offsets[part],
                    },
                    p.text[byte..].chars().take(120).collect(),
                ));
                // The interactive chooser is bounded even for a one-letter query in a huge book.
                if hits.len() == 5000 {
                    return hits;
                }
            }
        }
        hits
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
