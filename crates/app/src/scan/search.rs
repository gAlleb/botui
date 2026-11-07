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
}
impl Search {
    pub fn start(&mut self, worker: &Worker, path: &Path, kind: FileKind, query: String) {
        self.query = query.clone();
        self.hits.clear();
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
    pub fn poll(&mut self, current: u32) -> Option<io::Result<Option<u32>>> {
        let response = match self.job.as_ref()?.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(io::Error::other("Search interrupted")),
        };
        self.job = None;
        Some(response.map(|(layer, hits)| {
            self.layer = Some(layer);
            self.hits = hits;
            self.hits
                .iter()
                .find(|hit| hit.page >= current)
                .or(self.hits.first())
                .map(|hit| hit.page)
        }))
    }
    pub fn step(&self, current: u32, forward: bool) -> Option<u32> {
        let hit = if forward {
            self.hits
                .iter()
                .find(|h| h.page > current)
                .or(self.hits.first())
        } else {
            self.hits
                .iter()
                .rev()
                .find(|h| h.page < current)
                .or(self.hits.last())
        };
        hit.map(|h| h.page)
    }
    pub fn status(&self) -> String {
        match self.hits.len() {
            0 => "No matches in the embedded text".into(),
            1 => "1 matching page · F5 results".into(),
            n => format!("{n} matching pages · N next · F5 results"),
        }
    }
    pub fn choose(&self, term: &mut AppTerminal, current: u32) -> io::Result<Option<u32>> {
        let labels: Vec<_> = self
            .hits
            .iter()
            .map(|h| format!(" Page {}  {}", h.page, h.excerpt))
            .collect();
        let selected = self.hits.partition_point(|hit| hit.page < current);
        let picked = prompt::choose(term, "Matching pages", &labels, selected)?;
        Ok(picked.map(|i| self.hits[i].page))
    }
}
