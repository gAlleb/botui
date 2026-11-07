//! Keep a spread and its neighbours; the next request always favours visible pages.
use crate::{
    cover::Picture,
    shelf::background::{Ticket, Worker},
};
use botui_formats::FileKind;
use lru::LruCache;
use std::{io, num::NonZeroUsize, path::Path, sync::mpsc::TryRecvError};
type Key = (u32, u16);
pub(super) struct Pages {
    worker: Worker,
    job: Option<(Key, Ticket<io::Result<Picture>>)>,
    cache: LruCache<Key, Picture>,
    failed_prefetch: Option<Key>,
}
impl Pages {
    pub fn new() -> Self {
        Self {
            worker: Worker::new(),
            job: None,
            cache: LruCache::new(NonZeroUsize::new(4).unwrap()),
            failed_prefetch: None,
        }
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn get(&self, page: u32, side: u16) -> Option<&Picture> {
        self.cache.peek(&(page, side))
    }
    pub fn update(
        &mut self,
        path: &Path,
        kind: FileKind,
        first: u32,
        count: u32,
        cols: u16,
        side: u16,
    ) -> io::Result<()> {
        let visible_end = first.saturating_add(u32::from(cols) - 1).min(count);
        let end = visible_end.saturating_add(u32::from(cols)).min(count);
        let relevant = |k: Key| k.1 == side && (first..=end).contains(&k.0);
        if self.job.as_ref().is_some_and(|(k, _)| !relevant(*k)) {
            self.job = None;
        }
        if let Some((k, t)) = &self.job {
            match t.receiver.try_recv() {
                Ok(Ok(p)) => {
                    self.cache.put(*k, p);
                    self.job = None;
                }
                Err(TryRecvError::Empty) => (),
                result => {
                    let err = match result {
                        Ok(Err(e)) => e,
                        _ => io::Error::other("Page rendering interrupted"),
                    };
                    if k.0 <= visible_end {
                        return Err(err);
                    }
                    self.failed_prefetch = Some(*k);
                    self.job = None;
                }
            }
        }
        for i in first..=visible_end {
            self.cache.promote(&(i, side));
        }
        if self.job.is_none()
            && let Some(i) = (first..=end).find(|i| {
                !self.cache.contains(&(*i, side))
                    && (*i <= visible_end || self.failed_prefetch != Some((*i, side)))
            })
        {
            let path = path.to_owned();
            self.job = Some((
                (i, side),
                self.worker.start(move |cancel| {
                    Ok(Picture::new(botui_fixed::page(
                        &path, kind, i, side, cancel,
                    )?))
                }),
            ));
        }
        Ok(())
    }
}
