//! A downloaded native edition retains its exact progress baseline and account.
//! Uploads use the same conditional write as a live read. No credentials on disk.
pub(crate) mod reconnect;
use botui_bookorbit::{
    Session,
    progress::{Move, Progress, Target},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub fn account(server: &str, user: &str) -> String {
    let mut h = Sha256::new();
    h.update(server.trim_end_matches('/'));
    h.update([0]);
    h.update(user);
    format!("{:x}", h.finalize())
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct Copy {
    account: String,
    base: Progress,
    pending: Option<Move>,
}
fn sidecar(book: &Path) -> PathBuf {
    book.with_file_name(format!(
        ".{}.json",
        book.file_name().unwrap_or_default().to_string_lossy()
    ))
}
pub(super) fn has_sidecar(book: &Path) -> bool {
    sidecar(book).is_file()
}

fn lock(dir: &Path) -> io::Result<fs::File> {
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".progress.lock"))?;
    file.lock()?;
    Ok(file)
}
fn write(path: &Path, record: &Copy) -> io::Result<()> {
    let mut tmp =
        tempfile::NamedTempFile::new_in(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    serde_json::to_writer(tmp.as_file_mut(), record)?;
    tmp.persist(path)?;
    Ok(())
}
fn load(path: &Path) -> io::Result<Option<Copy>> {
    match fs::File::open(path) {
        Ok(f) => Ok(Some(serde_json::from_reader(f)?)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
/// Older installations stored only the progress sidecar beside the saved edition.
pub(super) fn legacy_file(
    dir: &Path,
    account: &str,
    id: u64,
    kind: botui_formats::FileKind,
) -> io::Result<Option<PathBuf>> {
    let mut newest = None;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(book) = name
            .to_str()
            .and_then(|n| n.strip_prefix('.'))
            .and_then(|n| n.strip_suffix(".json"))
        else {
            continue;
        };
        let path = dir.join(book);
        if botui_formats::FileKind::from_path(&path) != Some(kind)
            || !load(&entry.path())
                .ok()
                .flatten()
                .is_some_and(|r| r.account == account && r.base.file_id == id)
        {
            continue;
        }
        let Ok(meta) = path.symlink_metadata() else {
            continue;
        };
        if meta.is_file() && meta.len() > 0 {
            newest = newest.max(Some((meta.modified()?, path)));
        }
    }
    Ok(newest.map(|(_, path)| path))
}

/// Keep an unsent position syncable even when its book no longer takes up disk space.
pub fn remove_download(book: &Path, source: crate::cache::Source) -> io::Result<()> {
    let dir = crate::cache::downloads()?.canonicalize()?;
    let _lock = lock(&dir)?;
    if book.parent() != Some(dir.as_path()) || !source.files(&dir)?.contains(book) {
        return Err(io::Error::other(
            "This file is no longer on the selected download shelf.",
        ));
    }
    let record = sidecar(book);
    let remove_record = source == crate::cache::Source::BookOrbit
        && load(&record)?.is_some_and(|copy| copy.pending.is_none());
    fs::remove_file(book)?;
    if remove_record {
        fs::remove_file(record)?;
    }
    Ok(())
}

pub fn associate(book: &Path, account: String, base: Progress) -> io::Result<()> {
    let path = sidecar(book);
    let _lock = lock(book.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    if load(&path)?.is_some_and(|r| r.pending.is_some()) {
        return Ok(());
    }
    write(
        &path,
        &Copy {
            account,
            base,
            pending: None,
        },
    )
}
pub fn advance(book: &Path, base: &Progress, current: Progress) -> io::Result<()> {
    let _lock = lock(book.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    let path = sidecar(book);
    if let Some(mut copy) = load(&path)?
        && copy.base == *base
        && copy.pending.is_none()
    {
        copy.base = current;
        write(&path, &copy)?;
    }
    Ok(())
}

pub fn queue_if_current(book: &Path, base: &Progress, change: Move) -> io::Result<()> {
    if let Some(copy) = Copy::open(book)?
        && copy.base == *base
        && copy.pending.is_none()
    {
        copy.queue(book, change)?;
    }
    Ok(())
}
impl Copy {
    pub fn open(path: &Path) -> io::Result<Option<Self>> {
        let dir = crate::cache::downloads()?;
        if path.parent() != Some(dir.as_path())
            && dir.canonicalize().ok().as_deref() != path.parent()
        {
            return Ok(None);
        }
        load(&sidecar(path))
    }
    pub fn starting_position(&self) -> Progress {
        let mut current = self.base.clone();
        if let Some(m) = &self.pending {
            current.percentage = m.percentage;
            match &m.target {
                Target::Text(cfi) => {
                    current.text.cfi = Some(cfi.clone());
                    current.text.page = None
                }
                Target::Page(n) => {
                    current.text.page = Some(*n);
                    current.text.cfi = None
                }
            }
        }
        current
    }
    pub(crate) fn pending_for(&self, account: &str, file: u64) -> Option<&Move> {
        self.pending
            .as_ref()
            .filter(|_| self.account == account && self.base.file_id == file)
    }

    /// Apply the user's choice only to the pair of positions they saw.
    /// The next upload still uses the normal conditional BookOrbit write.
    pub(crate) fn resolve(
        mut self,
        book: &Path,
        server: &Progress,
        keep_offline: bool,
    ) -> io::Result<()> {
        let _lock = lock(book.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
        let path = sidecar(book);
        if load(&path)?.as_ref() != Some(&self) {
            return Err(io::Error::other(
                "The offline position changed. Open the book again.",
            ));
        }
        self.base = server.clone();
        if !keep_offline {
            self.pending = None;
        }
        write(&path, &self)
    }
    pub fn queue(self, path: &Path, m: Move) -> io::Result<()> {
        let _lock = lock(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
        let meta = sidecar(path);
        let Some(mut current) = load(&meta)? else {
            return Err(io::Error::other("Saved BookOrbit edition is missing"));
        };
        // A reconnect can acknowledge the previous queued location while this reader is open.
        if current != self
            && !(current.account == self.account
                && current.pending.is_none()
                && self
                    .pending
                    .as_ref()
                    .is_some_and(|m| matches_move(&current.base, m)))
        {
            return Err(io::Error::other(
                "Offline copy changed in another session; position saved locally",
            ));
        }
        current.pending = Some(m);
        write(&meta, &current)?;
        reconnect::wake();
        Ok(())
    }
}
/// The connected reader saves a file position here even before the download
/// has ever been opened from the offline shelf.
pub fn unfinished_downloads(
    files: &std::collections::HashSet<PathBuf>,
    history: Vec<botui_store::HistoryEntry>,
) -> io::Result<Vec<(PathBuf, u8)>> {
    let mut local: std::collections::HashMap<_, _> = history
        .into_iter()
        .map(|h| (h.path, (h.read_at, h.percent)))
        .collect();
    let mut reading = Vec::new();
    for path in files {
        let record = sidecar(path);
        let previous = local.remove(path);
        let (time, percent) = if let Some(copy) = load(&record)? {
            let pos = copy.starting_position();
            let started = pos.percentage > 0. || pos.text.cfi.is_some() || pos.text.page.is_some();
            if !started || pos.percentage >= 100. {
                continue;
            }
            let modified = record.metadata()?.modified()?;
            let time = previous.map_or(modified, |(at, _)| at.max(modified));
            (time, pos.percentage.round().clamp(0., 99.) as u8)
        } else if let Some(known) = previous {
            known
        } else {
            continue;
        };
        reading.push((std::cmp::Reverse(time), path.clone(), percent));
    }
    reading.sort_unstable();
    Ok(reading
        .into_iter()
        .map(|(_, path, percent)| (path, percent))
        .collect())
}

fn matches_move(current: &Progress, movement: &Move) -> bool {
    (current.percentage - movement.percentage).abs() < 0.000_001
        && match &movement.target {
            Target::Text(cfi) => current.text.cfi.as_ref() == Some(cfi),
            Target::Page(n) => current.text.page == Some(*n),
        }
}

fn records(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut result = Vec::new();
    for item in entries {
        let path = item?.path();
        if path
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with('.'))
            && path.extension().is_some_and(|v| v == "json")
        {
            result.push(path);
        }
    }
    Ok(result)
}
pub(super) fn has_pending(account: &str) -> io::Result<bool> {
    for path in records(&crate::cache::downloads()?)? {
        if load(&path)?.is_some_and(|r| r.account == account && r.pending.is_some()) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn sync(
    session: &Session,
    account: &str,
    cancel: &AtomicBool,
) -> botui_bookorbit::Result<Option<String>> {
    let dir = crate::cache::downloads()?;
    if !has_pending(account)? {
        return Ok(None);
    }
    let serial = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!(".sync-{account}.lock")))?;
    serial.lock()?;
    let mut messages = Vec::new();
    for path in records(&dir)? {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let Some(mut record) = load(&path)? else {
            continue;
        };
        if record.account != account {
            continue;
        }
        let Some(m) = record.pending.as_ref() else {
            continue;
        };
        let current = session.with_token(|a, t| a.progress(t, record.base.file_id))?;
        // Finishing the last viewport can change only the percentage.
        let matches = matches_move(&current, m);
        let saved = matches
            || (current == record.base
                && session.with_token(|a, t| a.save_move(t, &record.base, m))?);
        if saved {
            let acknowledged = if matches {
                current
            } else {
                session.with_token(|a, t| a.progress(t, record.base.file_id))?
            };
            let _lock = lock(&dir)?;
            let Some(mut latest) = load(&path)? else {
                continue;
            };
            if latest == record {
                record.base = acknowledged;
                record.pending = None;
                write(&path, &record)?;
            } else if latest.account == record.account
                && latest.base == record.base
                && latest.pending.is_some()
                && matches_move(&acknowledged, m)
            {
                latest.base = acknowledged;
                write(&path, &latest)?;
            }
        } else {
            messages.push(
                "Reading positions differ. Open the downloaded edition in BookOrbit to choose which position to keep.".to_owned(),
            );
        }
    }
    Ok(messages.into_iter().next())
}

/// A confirmed BookOrbit reset supersedes unsent moves for this account's editions.
pub fn reset_positions(account: &str, files: &[u64]) -> io::Result<()> {
    let dir = crate::cache::downloads()?;
    if !dir.is_dir() {
        return Ok(());
    }
    let _lock = lock(&dir)?;
    for path in records(&dir)? {
        if let Some(mut copy) = load(&path)?
            && copy.account == account
            && files.contains(&copy.base.file_id)
        {
            copy.pending = None;
            copy.base.updated_at = None;
            copy.base.percentage = 0.;
            copy.base.text.cfi = None;
            copy.base.text.page = None;
            write(&path, &copy)?;
        }
    }
    Ok(())
}
