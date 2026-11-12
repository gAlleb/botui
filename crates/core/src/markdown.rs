//! Project visible Markdown text onto immutable source byte ranges.
use crate::{Anchor, Markdown, TextRange};
use std::ops::Range;
impl Markdown {
    pub fn index(&mut self, raw: &str) {
        self.text.clear();
        self.mapping.clear();
        let mut previous = None;
        for p in &self.passages {
            if let Some(end) = previous
                && raw[end..p.start.byte].contains('\n')
            {
                self.text.push('\n');
            }
            let start = self.text.len();
            self.text
                .push_str(p.display.as_deref().unwrap_or(&raw[p.start.byte..p.end]));
            self.mapping
                .push((start..self.text.len(), p.start.byte..p.end));
            previous = Some(p.end);
        }
    }
    pub fn source_range(&self, bytes: Range<usize>) -> Option<TextRange> {
        let first = self.mapping.partition_point(|(r, _)| r.end <= bytes.start);
        let last = self
            .mapping
            .partition_point(|(r, _)| r.start < bytes.end)
            .checked_sub(1)?;
        if first > last {
            return None;
        }
        let (a, source_a) = self.mapping.get(first)?;
        let (b, source_b) = self.mapping.get(last)?;
        let start = source_a.start
            + if a.len() == source_a.len() {
                bytes.start.saturating_sub(a.start)
            } else {
                0
            };
        let end = source_b.end
            - if b.len() == source_b.len() {
                b.end.saturating_sub(bytes.end)
            } else {
                0
            };
        Some(TextRange::new(
            Anchor {
                block: 0,
                byte: start,
            },
            Anchor {
                block: 0,
                byte: end,
            },
        ))
    }
    pub fn display_position(&self, byte: usize) -> usize {
        let i = self.mapping.partition_point(|(_, r)| r.end <= byte);
        self.mapping.get(i).map_or(self.text.len(), |(r, source)| {
            r.start
                + if r.len() == source.len() {
                    byte.saturating_sub(source.start)
                } else {
                    0
                }
        })
    }
}
