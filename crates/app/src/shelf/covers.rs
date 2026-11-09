//! One bounded cache per login. Request only covers visible on the current shelf.
use crate::cover::Picture;

use lru::LruCache;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex, mpsc},
    time::{Duration, Instant},
};

type Loaded = botui_bookorbit::Result<Option<Picture>>;
struct Request {
    id: u64,
    load: Box<dyn FnOnce() + Send>,
}
#[derive(Default)]
struct Queue {
    waiting: VecDeque<Request>,
    closed: bool,
}
#[derive(Default)]
struct Workers {
    queue: Mutex<Queue>,
    wake: Condvar,
}
impl Workers {
    fn run(&self) {
        loop {
            let mut queue = self.queue.lock().unwrap();
            while queue.waiting.is_empty() && !queue.closed {
                queue = self.wake.wait(queue).unwrap();
            }
            let Some(request) = queue.waiting.pop_front() else {
                return;
            };
            drop(queue);
            (request.load)();
        }
    }
}
enum Entry {
    Image(Box<Picture>),
    Missing,
    RetryAfter(Instant),
}
pub(crate) enum Artwork<'a> {
    Loading,
    Image(&'a Picture),
    Jacket,
}
pub(crate) struct Covers {
    images: LruCache<u64, Entry>,
    workers: Arc<Workers>,
    pending: HashMap<u64, mpsc::Receiver<Loaded>>,
    visible: HashSet<u64>,
}
impl Covers {
    pub fn new() -> Self {
        Self::with_capacity(48)
    }
    pub fn with_capacity(size: usize) -> Self {
        let workers = Arc::new(Workers::default());
        for _ in 0..2 {
            let workers = Arc::clone(&workers);
            std::thread::spawn(move || workers.run());
        }
        Self {
            images: LruCache::new(NonZeroUsize::new(size).unwrap()),
            workers,
            pending: HashMap::new(),
            visible: HashSet::new(),
        }
    }
    /// A loaded figure must stay resident until the rest of this viewport is ready.
    pub fn fit_visible(&mut self, count: usize) {
        self.images.resize(NonZeroUsize::new(count.max(8)).unwrap());
    }
    pub fn reset(&mut self) {
        self.workers.queue.lock().unwrap().waiting.clear();
        self.pending.clear();
        self.visible.clear();
        self.images.clear();
    }
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn image(&self, id: u64) -> Option<&Picture> {
        match self.images.peek(&id) {
            Some(Entry::Image(p)) => Some(p),
            _ => None,
        }
    }
    pub fn artwork(&self, book: &crate::catalog::Book) -> Artwork<'_> {
        match self.images.peek(&book.id) {
            Some(Entry::Image(p)) => Artwork::Image(p),
            Some(Entry::Missing | Entry::RetryAfter(_)) => Artwork::Jacket,
            None if book.has_cover == Some(false) => Artwork::Jacket,
            None => Artwork::Loading,
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
        {
            return;
        }
        self.visible.insert(id);
        if self.pending.contains_key(&id) || self.pending.len() >= self.images.cap().get() + 2 {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending.insert(id, receiver);
        let request = Request {
            id,
            load: Box::new(move || {
                let _ = sender.send(fetch().map(|bytes| bytes.and_then(decode)));
            }),
        };
        self.workers
            .queue
            .lock()
            .unwrap()
            .waiting
            .push_back(request);
        self.workers.wake.notify_one();
    }
    /// Keep in-flight images for a quick return; skip queued work that scrolled out of view.
    pub fn finish_frame(&mut self) {
        if !self.pending.is_empty() {
            let mut queue = self.workers.queue.lock().unwrap();
            queue.waiting.retain(|request| {
                let keep = self.visible.contains(&request.id);
                if !keep {
                    self.pending.remove(&request.id);
                }
                keep
            });
        }
        self.visible.clear();
    }
    pub fn collect(&mut self) {
        self.pending.retain(|id, receiver| {
            let entry = match receiver.try_recv() {
                Ok(Ok(Some(pic))) => Entry::Image(Box::new(pic)),
                Ok(Ok(None)) => Entry::Missing,
                Err(mpsc::TryRecvError::Empty) => return true,
                _ => Entry::RetryAfter(Instant::now() + Duration::from_secs(30)),
            };
            self.images.put(*id, entry);
            false
        });
    }
}
impl Drop for Covers {
    fn drop(&mut self) {
        let mut queue = self.workers.queue.lock().unwrap();
        queue.waiting.clear();
        queue.closed = true;
        self.workers.wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_covers_load_without_another_frame_and_stale_jobs_are_skipped() {
        let mut covers = Covers::new();
        let (started, calls) = mpsc::channel();
        let mut gates = Vec::new();
        for id in 0..8 {
            let gate = (id < 2).then(|| {
                let (release, gate) = mpsc::channel();
                gates.push(release);
                gate
            });
            let started = started.clone();
            covers.request(id, move || {
                started.send(id).unwrap();
                if let Some(gate) = gate {
                    gate.recv().unwrap();
                }
                Ok(None)
            });
        }
        covers.finish_frame();
        let timeout = Duration::from_secs(3);
        let mut first = [
            calls.recv_timeout(timeout).unwrap(),
            calls.recv_timeout(timeout).unwrap(),
        ];
        first.sort();
        assert_eq!(first, [0, 1]);
        // Scroll past 2 and 3 while the two requests are in flight.
        for id in 4..8 {
            covers.request(id, || panic!("queued cover was fetched twice"));
        }
        covers.finish_frame();
        for release in gates {
            release.send(()).unwrap();
        }
        let mut remaining: Vec<_> = (0..4)
            .map(|_| calls.recv_timeout(timeout).unwrap())
            .collect();
        remaining.sort();
        assert_eq!(remaining, [4, 5, 6, 7]);
        let deadline = Instant::now() + timeout;
        while covers.busy() {
            assert!(Instant::now() < deadline);
            covers.collect();
            std::thread::yield_now();
        }
        assert_eq!(covers.images.len(), 6);
        assert!(calls.try_recv().is_err());
    }
}
