//! One bounded cache per login. Request only covers visible on the current shelf.
use super::background::{Ticket, Worker};
use crate::cover::Picture;

use lru::LruCache;
use std::{
    num::NonZeroUsize,
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

struct Slot {
    worker: Worker,
    job: Option<(u64, Ticket<botui_bookorbit::Result<Option<Picture>>>)>,
}
enum Entry {
    Image(Box<Picture>),
    Missing,
    RetryAfter(Instant),
}
pub(crate) struct Covers {
    images: LruCache<u64, Entry>,
    slots: [Slot; 2],
}
impl Covers {
    pub fn new() -> Self {
        Self::with_capacity(48)
    }
    pub fn with_capacity(size: usize) -> Self {
        Self {
            images: LruCache::new(NonZeroUsize::new(size).unwrap()),
            slots: std::array::from_fn(|_| Slot {
                worker: Worker::new(),
                job: None,
            }),
        }
    }
    pub fn reset(&mut self) {
        for s in &mut self.slots {
            s.job = None;
        }
        self.images.clear();
    }
    pub fn busy(&self) -> bool {
        self.slots.iter().any(|s| s.job.is_some())
    }
    pub fn image(&self, id: u64) -> Option<&Picture> {
        match self.images.peek(&id) {
            Some(Entry::Image(p)) => Some(p),
            _ => None,
        }
    }
    pub fn request(
        &mut self,
        id: u64,
        fetch: impl FnOnce() -> botui_bookorbit::Result<Option<Vec<u8>>> + Send + 'static,
    ) {
        self.request_with(id, fetch, Picture::cover)
    }
    pub fn request_with(
        &mut self,
        id: u64,
        fetch: impl FnOnce() -> botui_bookorbit::Result<Option<Vec<u8>>> + Send + 'static,
        decode: fn(Vec<u8>) -> Option<Picture>,
    ) {
        if self
            .images
            .get(&id)
            .is_some_and(|e| !matches!(e, Entry::RetryAfter(t) if *t <= Instant::now()))
            || self
                .slots
                .iter()
                .any(|s| s.job.as_ref().is_some_and(|j| j.0 == id))
        {
            return;
        }
        let Some(slot) = self.slots.iter_mut().find(|s| s.job.is_none()) else {
            return;
        };
        slot.job = Some((
            id,
            slot.worker.start(move |_| Ok(fetch()?.and_then(decode))),
        ));
    }
    pub fn collect(&mut self) {
        for slot in &mut self.slots {
            let Some((id, ticket)) = &slot.job else {
                continue;
            };
            let entry = match ticket.receiver.try_recv() {
                Ok(Ok(Some(pic))) => Entry::Image(Box::new(pic)),
                Ok(Ok(None)) => Entry::Missing,
                Err(TryRecvError::Empty) => continue,
                _ => Entry::RetryAfter(Instant::now() + Duration::from_secs(30)),
            };
            self.images.put(*id, entry);
            slot.job = None;
        }
    }
}
