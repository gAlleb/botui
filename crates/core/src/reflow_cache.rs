//! Recently used text geometries, independent of page count and reading position.
use crate::{Anchor, PageLine};
use std::collections::VecDeque;

pub(crate) struct Snapshot {
    pub size: (u16, usize),
    pub lines: Vec<PageLine>,
    pub sources: Vec<(Anchor, usize)>,
}
impl Snapshot {
    fn bytes(&self) -> usize {
        self.lines.capacity() * size_of::<PageLine>()
            + self.sources.capacity() * size_of::<(Anchor, usize)>()
            + self
                .lines
                .iter()
                .map(|l| {
                    l.text.capacity()
                        + l.spans.capacity() * size_of::<crate::publisher::TextSpan>()
                        + l.figures.capacity() * size_of::<crate::Figure>()
                })
                .sum::<usize>()
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    entries: VecDeque<(Snapshot, usize)>,
    bytes: usize,
}
impl Cache {
    pub fn take(&mut self, size: (u16, usize)) -> Option<Snapshot> {
        let i = self.entries.iter().position(|(s, _)| s.size == size)?;
        let (snapshot, bytes) = self.entries.remove(i)?;
        self.bytes -= bytes;
        Some(snapshot)
    }
    pub fn put(&mut self, snapshot: Snapshot) {
        const LIMIT: usize = 16 * 1024 * 1024;
        let bytes = snapshot.bytes();
        if bytes > LIMIT {
            return;
        }
        // Keep nearby widths as well as both sidebar/zen geometries. The byte
        // bound still takes precedence for books with many styled spans.
        while self.entries.len() >= 4 || self.bytes + bytes > LIMIT {
            if let Some((_, bytes)) = self.entries.pop_front() {
                self.bytes -= bytes;
            }
        }
        self.bytes += bytes;
        self.entries.push_back((snapshot, bytes));
    }
}
