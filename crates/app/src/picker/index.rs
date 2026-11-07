//! Recursive browsing runs off the event loop. A view owns its result channel;
//! leaving that view drops the channel and cancels pending filesystem work.
use super::Entry;
use crate::shelf::background::{Ticket, Worker};
use botui_formats::{BookMetadata, FileKind};
use lru::LruCache;
use std::{
    fs, io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime},
};

type Stamp = (u64, Option<SystemTime>);
type Cache = Arc<Mutex<LruCache<PathBuf, (Stamp, Arc<BookMetadata>)>>>;
pub(super) struct Index {
    worker: Worker,
    run: Option<Ticket<io::Result<()>>>,
    batches: Option<mpsc::Receiver<Vec<Entry>>>,
    cache: Cache,
}
fn metadata(path: &Path, kind: FileKind, c: &Cache) -> Option<Arc<BookMetadata>> {
    if !kind.readable() {
        return None;
    }
    let m = path.metadata().ok()?;
    if !m.is_file() {
        return None;
    }
    let stamp = (m.len(), m.modified().ok());
    if let Some((s, v)) = c.lock().ok()?.get(path)
        && *s == stamp
    {
        return Some(v.clone());
    }
    let value = Arc::new(botui_formats::book_metadata(path).unwrap_or_default());
    c.lock().ok()?.put(path.to_owned(), (stamp, value.clone()));
    Some(value)
}
fn key(s: &str) -> String {
    s.to_lowercase().replace('ё', "е")
}
impl Index {
    pub fn new() -> Self {
        Self {
            worker: Worker::new(),
            run: None,
            batches: None,
            cache: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(8192).unwrap()))),
        }
    }
    pub fn listing(&self) -> bool {
        self.batches.is_some()
    }
    pub fn busy(&self) -> bool {
        self.run.is_some()
    }
    pub fn clear(&mut self) {
        self.run = None;
        self.batches = None;
    }
    pub fn warm(&mut self, files: Vec<(PathBuf, FileKind)>) {
        self.clear();
        if files.is_empty() {
            return;
        }
        let cache = self.cache.clone();
        self.run = Some(self.worker.start(move |stop| {
            for (p, k) in files {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                metadata(&p, k, &cache);
            }
            Ok(())
        }));
    }
    pub fn cached(&self, path: &Path) -> Option<Arc<BookMetadata>> {
        Some(self.cache.lock().ok()?.peek(path)?.1.clone())
    }
    pub fn start(&mut self, root: PathBuf, query: Option<&str>) {
        self.clear();
        let cache = self.cache.clone();
        let query = query.map(key);
        let (tx, rx) = mpsc::channel();
        self.batches = Some(rx);
        self.run = Some(
            self.worker
                .start(move |stop| scan(&root, query.as_deref(), stop, &cache, tx)),
        );
    }
    pub fn collect(&mut self, entries: &mut Vec<Entry>) -> Option<io::Result<()>> {
        let done = match self.run.as_ref()?.receiver.try_recv() {
            Ok(res) => Some(res),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(io::Error::other("Library scan interrupted")))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        };
        if let Some(rx) = &self.batches {
            for batch in rx.try_iter() {
                entries.extend(batch)
            }
        }
        if done.is_some() {
            self.clear()
        }
        done
    }
}

fn scan(
    root: &Path,
    query: Option<&str>,
    stop: &AtomicBool,
    cache: &Cache,
    tx: mpsc::Sender<Vec<Entry>>,
) -> io::Result<()> {
    let words: Vec<_> = query.unwrap_or_default().split_whitespace().collect();
    let mut dirs = vec![root.to_owned()];
    let mut dated = Vec::new();
    let mut batch = Vec::new();
    let mut sent = Instant::now();
    while let Some(dir) = dirs.pop() {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let iter = match fs::read_dir(&dir) {
            Ok(v) => v,
            Err(e) if dir == root => return Err(e),
            Err(_) => continue,
        };
        for en in iter.flatten() {
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            if en.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(ty) = en.file_type() else { continue };
            let path = en.path();
            if ty.is_dir() {
                dirs.push(path);
                continue;
            }
            if !ty.is_file() && !(ty.is_symlink() && path.is_file()) {
                continue;
            }
            let Some(kind) = FileKind::from_path(&path) else {
                continue;
            };
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            if query.is_some() {
                let meta = metadata(&path, kind, cache);
                let mut hay = key(&name);
                if let Some(m) = &meta {
                    for v in std::iter::once(&m.title).chain(&m.authors).chain(&m.series) {
                        hay.push(' ');
                        hay.push_str(&key(v));
                    }
                }
                if !words.iter().all(|w| hay.contains(w)) {
                    continue;
                }
                batch.push(Entry {
                    path,
                    name,
                    percent: None,
                    kind: Some(kind),
                    meta,
                });
                if batch.len() >= 48 || sent.elapsed() > Duration::from_millis(90) {
                    if tx.send(std::mem::take(&mut batch)).is_err() {
                        return Ok(());
                    }
                    sent = Instant::now();
                }
            } else {
                let at = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.created().or_else(|_| m.modified()).ok());
                dated.push((
                    std::cmp::Reverse(at),
                    key(&name),
                    Entry {
                        path,
                        name,
                        percent: None,
                        kind: Some(kind),
                        meta: None,
                    },
                ));
            }
        }
    }
    if query.is_none() {
        dated.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        batch = dated.into_iter().map(|(_, _, e)| e).collect();
    }
    let _ = tx.send(batch);
    Ok(())
}
