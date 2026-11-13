use crate::{
    AppTerminal, prompt,
    shelf::background::{Ticket, Worker},
};
use botui_fixed::{PageMatch, TextLayer};
use botui_formats::FileKind;
use std::{
    io,
    path::Path,
    sync::{Arc, mpsc::TryRecvError},
};

type Reply = io::Result<(Arc<TextLayer>, Vec<PageMatch>)>;

#[derive(Default)]
pub(super) struct Search {
    layer: Option<Arc<TextLayer>>,
    job: Option<Ticket<Reply>>,
    hits: Vec<PageMatch>,
    pub query: String,
    selected: Option<usize>,
}

impl Search {
    pub fn start(&mut self, worker: &Worker, path: &Path, kind: FileKind, query: String) {
        self.query = query.clone();
        self.hits.clear();
        self.selected = None;
        let layer = self.layer.clone();
        let path = path.to_owned();
        self.job = Some(worker.start(move |cancel| {
            let layer = match layer {
                Some(layer) => layer,
                None => Arc::new(botui_fixed::text_layer(&path, kind, cancel)?),
            };
            let hits = layer.find(&query, cancel);
            Ok((layer, hits))
        }));
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn poll(
        &mut self,
        current: u32,
        origin: Option<(u32, usize)>,
        forward: bool,
    ) -> Option<io::Result<Option<u32>>> {
        let response = match self.job.as_ref()?.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(io::Error::other("Search interrupted")),
        };
        self.job = None;
        Some(response.map(|(layer, hits)| {
            self.layer = Some(layer);
            self.hits = hits;
            self.selected = (!self.hits.is_empty()).then(|| {
                let at = origin.unwrap_or((current, if forward { 0 } else { usize::MAX }));
                if forward {
                    self.hits.partition_point(|h| (h.page, h.bytes.start) < at) % self.hits.len()
                } else {
                    (self.hits.partition_point(|h| (h.page, h.bytes.start) <= at) + self.hits.len()
                        - 1)
                        % self.hits.len()
                }
            });
            self.selected.map(|i| self.hits[i].page)
        }))
    }
    pub fn step(
        &mut self,
        current: u32,
        origin: Option<(u32, usize)>,
        forward: bool,
    ) -> Option<u32> {
        let count = self.hits.len();
        if count == 0 {
            return None;
        }
        let i = if let Some(at) = origin {
            if forward {
                self.hits.partition_point(|h| (h.page, h.bytes.start) <= at) % count
            } else {
                (self.hits.partition_point(|h| (h.page, h.bytes.start) < at) + count - 1) % count
            }
        } else if let Some(i) = self.selected.filter(|i| self.hits[*i].page == current) {
            (i + if forward { 1 } else { count - 1 }) % count
        } else if forward {
            self.hits.partition_point(|h| h.page < current) % count
        } else {
            (self.hits.partition_point(|h| h.page <= current) + count - 1) % count
        };
        self.selected = Some(i);
        Some(self.hits[i].page)
    }
    pub fn reveal(&self, text: &mut super::text::Text) -> Option<botui_store::PdfRegion> {
        let hit = self.hits.get(self.selected?)?;
        let page = self.layer.as_ref()?.page(hit.page)?;
        text.show_search(hit, page)
    }
    pub fn status(&self) -> String {
        if self.hits.is_empty() {
            "No matches in the embedded text".into()
        } else {
            format!(
                "Match {} / {} · F3 next · F4 previous · F5 results",
                self.selected.map_or(0, |i| i + 1),
                self.hits.len()
            )
        }
    }
    pub fn choose(&mut self, term: &mut AppTerminal, current: u32) -> io::Result<Option<u32>> {
        let labels: Vec<_> = self
            .hits
            .iter()
            .map(|h| format!(" Page {}  {}", h.page, h.excerpt))
            .collect();
        let selected = self
            .selected
            .unwrap_or_else(|| self.hits.partition_point(|hit| hit.page < current));
        let picked = prompt::choose(term, "Search results", &labels, selected)?;
        if picked.is_some() {
            self.selected = picked;
        }
        Ok(picked.map(|i| self.hits[i].page))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_visits_each_occurrence_wraps_and_follows_a_moved_cursor() {
        let mut search = Search {
            hits: [(1, 4), (1, 24), (1, 48), (3, 12)]
                .into_iter()
                .map(|(page, byte)| PageMatch {
                    page,
                    bytes: byte..byte + 4,
                    excerpt: String::new(),
                })
                .collect(),
            selected: Some(0),
            ..Default::default()
        };
        for (page, index) in [(1, 1), (1, 2), (3, 3), (1, 0)] {
            let current = search.hits[search.selected.unwrap()].page;
            assert_eq!(search.step(current, None, true), Some(page));
            assert_eq!(search.selected, Some(index));
        }
        assert_eq!(search.step(1, None, false), Some(3));
        assert_eq!(search.selected, Some(3));
        assert_eq!(search.step(1, Some((1, 30)), true), Some(1));
        assert_eq!(search.selected, Some(2));
        assert_eq!(search.step(1, Some((1, 30)), false), Some(1));
        assert_eq!(search.selected, Some(1));
        assert!(!search.status().contains("N next"));
    }
}
