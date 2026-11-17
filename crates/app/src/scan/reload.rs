//! A changed local PDF is adopted only after the published file settles and Poppler can read it.
use crate::shelf::background::{Ticket, Worker};
use botui_core::Anchor;
use botui_formats::FileKind;
use botui_store::LocalState;
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    sync::{atomic::AtomicBool, mpsc::TryRecvError},
    time::{Duration, Instant, SystemTime},
};

const IDLE: Duration = Duration::from_secs(1);
const SETTLE: Duration = Duration::from_millis(300);
const CHECK_JOB: Duration = Duration::from_millis(100);

#[derive(PartialEq, Eq)]
struct Stamp {
    bytes: u64,
    modified: SystemTime,
    #[cfg(unix)]
    file: (u64, u64, i64, i64),
}

impl Stamp {
    fn read(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok().filter(|m| m.is_file())?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Some(Self {
            bytes: meta.len(),
            modified: meta.modified().ok()?,
            #[cfg(unix)]
            file: (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()),
        })
    }
}

pub(super) struct Revision {
    pub count: u32,
    pub title: Option<String>,
    pub state: io::Result<(LocalState, Vec<Anchor>)>,
}

impl Revision {
    fn read(path: &Path, page: u32, cancel: &AtomicBool) -> io::Result<Self> {
        // Poppler can repair a half-written PDF. Wait for the writer's final EOF marker.
        let mut file = fs::File::open(path)?;
        let tail = file.metadata()?.len().saturating_sub(1024);
        file.seek(SeekFrom::Start(tail))?;
        let mut end = Vec::with_capacity(1024);
        file.take(1024).read_to_end(&mut end)?;
        let complete = end.trim_ascii_end().ends_with(b"%%EOF");
        if !complete {
            return Err(io::Error::other("Unfinished PDF"));
        }
        let (count, title) = botui_fixed::page_info(path, FileKind::Pdf, cancel)?;
        botui_fixed::page(path, FileKind::Pdf, page.clamp(1, count), 200, cancel)?;
        let state = LocalState::for_file(path).and_then(|store| {
            let marks = store.load()?.map_or_else(Vec::new, |s| s.marks);
            Ok((store, marks))
        });
        Ok(Self {
            count,
            title,
            state,
        })
    }
}

pub(super) enum Wake {
    Input(Option<crossterm::event::Event>),
    Revised(Revision),
    Jump(botui_store::PdfRegion),
}

pub(super) struct Watch {
    inbox: Option<crate::synctex::Inbox>,
    stamp: Option<Stamp>,
    next: Instant,
    settling: bool,
    job: Option<Ticket<io::Result<Revision>>>,
}

impl Watch {
    pub fn editor(&self) -> Option<crate::synctex::EditorTarget> {
        self.inbox.as_ref().map(|i| i.editor())
    }

    pub fn new(path: &Path) -> Self {
        Self {
            inbox: crate::synctex::Inbox::open(path).ok(),
            stamp: Stamp::read(path),
            next: Instant::now() + IDLE,
            settling: false,
            job: None,
        }
    }

    /// Wake for input or reader work as usual. A metadata poll with no change
    /// goes straight back to sleep without redrawing the page.
    pub fn wait(
        &mut self,
        worker: &Worker,
        path: &Path,
        page: u32,
        timeout: Option<Duration>,
        mut input: impl FnMut(Option<Duration>) -> io::Result<Option<crossterm::event::Event>>,
    ) -> io::Result<Wake> {
        let deadline = timeout.map(|d| Instant::now() + d);
        loop {
            let now = Instant::now();
            if let Some(inbox) = &mut self.inbox {
                inbox.poll();
            }
            let jump = self.inbox.as_ref().is_some_and(|i| i.pending.is_some());
            // An editor can send its jump immediately after rebuilding the PDF.
            // Adopt the new edition before navigating to its fresh coordinates.
            if (now >= self.next || (jump && Stamp::read(path) != self.stamp))
                && let Some(revision) = self.poll(worker, path, page, now)
            {
                return Ok(Wake::Revised(revision));
            }
            if !self.settling
                && self.job.is_none()
                && let Some(target) = self.inbox.as_mut().and_then(|i| i.pending.take())
            {
                return Ok(Wake::Jump(target));
            }
            let wake = deadline.map_or(self.next, |d| d.min(self.next));
            let event = input(Some(wake.saturating_duration_since(Instant::now())))?;
            if event.is_some() || deadline.is_some_and(|d| d <= Instant::now()) {
                return Ok(Wake::Input(event));
            }
        }
    }

    fn poll(&mut self, worker: &Worker, path: &Path, page: u32, now: Instant) -> Option<Revision> {
        let stamp = Stamp::read(path);
        if stamp != self.stamp {
            self.job = None;
            self.settling = stamp.is_some();
            self.stamp = stamp;
        } else if self.settling {
            self.settling = false;
            let path = path.to_owned();
            self.job = Some(worker.start(move |cancel| Revision::read(&path, page, cancel)));
        } else if let Some(job) = &self.job {
            match job.receiver.try_recv() {
                Ok(result) => {
                    self.job = None;
                    self.next = now + IDLE;
                    return result.ok();
                }
                Err(TryRecvError::Disconnected) => self.job = None,
                Err(TryRecvError::Empty) => (),
            }
        }
        self.next = now
            + if self.settling {
                SETTLE
            } else if self.job.is_some() {
                CHECK_JOB
            } else {
                IDLE
            };
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_incremental_update_keeps_the_previous_edition() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.pdf");
        fs::write(&path, b"%PDF-1.4\n%%EOF\n7 0 obj\n").unwrap();
        assert_eq!(
            Revision::read(&path, 1, &AtomicBool::new(false))
                .err()
                .unwrap()
                .to_string(),
            "Unfinished PDF"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rename_with_the_same_length_and_mtime_restarts_the_settle_period() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("document.pdf");
        let tmp = dir.path().join("replacement");
        fs::write(&path, b"abc").unwrap();
        let mut watch = Watch::new(&path);
        let worker = Worker::new();
        let now = Instant::now();
        fs::write(&tmp, b"ABC").unwrap();
        fs::File::open(&tmp)
            .unwrap()
            .set_modified(fs::metadata(&path).unwrap().modified().unwrap())
            .unwrap();
        fs::rename(&tmp, &path).unwrap();
        assert!(watch.poll(&worker, &path, 1, now).is_none());
        assert!(watch.settling);
        assert_eq!(watch.next, now + SETTLE);
        fs::remove_file(&path).unwrap();
        assert!(watch.poll(&worker, &path, 1, now + SETTLE).is_none());
        assert!(!watch.settling && watch.job.is_none());
    }
}
