use super::index::Index;
use crate::live_search::{Item, Source};
use std::{io, path::PathBuf};

pub(super) type Search = crate::live_search::Search<Index>;

impl Source for Index {
    type Key = PathBuf;
    type Action = PathBuf;

    fn busy(&self) -> bool {
        Index::busy(self)
    }
    fn collect(&mut self, batch: &mut Vec<Item<PathBuf, PathBuf>>) -> io::Result<()> {
        let mut files = vec![];
        let result = Index::collect(self, &mut files);
        for entry in files {
            let mut text = entry.name.clone();
            let mut label = String::new();
            if let Some(meta) = &entry.meta {
                for field in std::iter::once(&meta.title)
                    .chain(&meta.authors)
                    .chain(&meta.series)
                {
                    text.push(' ');
                    text.push_str(field);
                }
                if !meta.title.is_empty() {
                    label = format!("{} · {}  —  ", meta.title, meta.authors.join(", "));
                }
            }
            label.push_str(&entry.name);
            batch.push(Item {
                key: entry.path.clone(),
                action: entry.path,
                name: entry.name,
                label,
                text,
            });
        }
        result.unwrap_or(Ok(()))
    }
}
