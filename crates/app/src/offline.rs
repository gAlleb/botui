//! A downloaded native edition retains its exact progress baseline and account.
//! Uploads use the same conditional write as a live read. No credentials on disk.
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
impl Copy {
    pub fn open(path: &Path) -> io::Result<Option<Self>> {
        if path.parent() != Some(crate::cache::downloads()?.as_path()) {
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
    pub fn queue(mut self, path: &Path, m: Move) -> io::Result<()> {
        let _lock = lock(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
        let meta = sidecar(path);
        if load(&meta)?.as_ref() != Some(&self) {
            return Err(io::Error::other(
                "Offline copy changed in another session; position saved locally",
            ));
        }
        self.pending = Some(m);
        write(&meta, &self)
    }
}
pub fn sync(
    session: &Session,
    account: &str,
    cancel: &AtomicBool,
) -> botui_bookorbit::Result<Option<String>> {
    let dir = crate::cache::downloads()?;
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut messages = Vec::new();
    for item in fs::read_dir(&dir)? {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let item = item?;
        let name = item.file_name();
        if !name.to_string_lossy().starts_with('.')
            || item.path().extension().and_then(|s| s.to_str()) != Some("json")
        {
            continue;
        }
        let path = item.path();
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
        let matches = (current.percentage - m.percentage).abs() < 0.000_001
            && match &m.target {
                Target::Text(cfi) => current.text.cfi.as_ref() == Some(cfi),
                Target::Page(n) => current.text.page == Some(*n),
            };
        let saved = matches
            || (current == record.base
                && session.with_token(|a, t| a.save_move(t, &record.base, m))?);
        let _lock = lock(&dir)?;
        if load(&path)?.as_ref() != Some(&record) {
            continue;
        }
        if saved {
            record.base = if matches {
                current
            } else {
                session.with_token(|a, t| a.progress(t, record.base.file_id))?
            };
            record.pending = None;
            write(&path, &record)?;
        } else {
            messages.push(
                "Another device changed an offline book; kept its local progress.".to_owned(),
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
    for item in fs::read_dir(&dir)? {
        let path = item?.path();
        if !path
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with('.'))
            || path.extension().and_then(|v| v.to_str()) != Some("json")
        {
            continue;
        }
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
