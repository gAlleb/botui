//! The native shelf and BookOrbit offline downloads share one durable file per downloaded revision.
use crate::{catalog::BookFile, offline};
use botui_bookorbit::{Result, Session};
use botui_formats::FileKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, SystemTime},
};
use tempfile::NamedTempFile;

#[derive(Deserialize, Serialize)]
struct Edition {
    name: String,
    bytes: u64,
    modified: SystemTime,
    content_version: Option<String>,
    etag: Option<String>,
}

impl Edition {
    fn from_path(path: &Path, version: Option<&str>, etag: Option<String>) -> io::Result<Self> {
        let meta = path.symlink_metadata()?;
        Ok(Self {
            name: path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or(io::ErrorKind::InvalidData)?
                .to_owned(),
            bytes: meta.len(),
            modified: meta.modified()?,
            content_version: version.map(str::to_owned),
            etag,
        })
    }

    fn local_file(&self, dir: &Path, kind: FileKind) -> Option<PathBuf> {
        let name = Path::new(&self.name);
        if name.file_name()? != name.as_os_str() || FileKind::from_path(name) != Some(kind) {
            return None;
        }
        let path = dir.join(name);
        let meta = path.symlink_metadata().ok()?;
        (meta.is_file()
            && meta.len() == self.bytes
            && self.bytes > 0
            && meta.modified().ok() == Some(self.modified))
        .then_some(path)
    }

    fn record(&self, index: &Path) -> io::Result<()> {
        let mut temp = NamedTempFile::new_in(index.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
        serde_json::to_writer(temp.as_file_mut(), self)?;
        temp.persist(index)?;
        Ok(())
    }
}

pub(super) fn saved_names(dir: &Path) -> io::Result<std::collections::HashSet<std::ffi::OsString>> {
    let mut names = std::collections::HashSet::new();
    let accounts = match fs::read_dir(dir.join(".editions")) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(names),
        Err(e) => return Err(e),
    };
    for account in accounts {
        let account = account?;
        if !account.file_type()?.is_dir() {
            continue;
        }
        for entry in fs::read_dir(account.path())? {
            let path = entry?.path();
            if path.extension().is_some_and(|v| v == "json")
                && let Ok(file) = fs::File::open(path)
                && let Ok(edition) = serde_json::from_reader::<_, Edition>(file)
            {
                names.insert(edition.name.into());
            }
        }
    }
    Ok(names)
}

struct DigestWriter<'a> {
    file: &'a mut fs::File,
    digest: Sha256,
}
impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.file.write(bytes)?;
        self.digest.update(&bytes[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub(crate) struct Request<'a> {
    pub account: &'a str,
    pub file: &'a BookFile,
    pub title: &'a str,
    pub version: Option<&'a str>,
    pub check_update: bool,
}

impl Request<'_> {
    pub fn open(
        &self,
        session: &Session,
        cancel: &AtomicBool,
        progress: &botui_transfer::Progress,
    ) -> Result<PathBuf> {
        let kind = self
            .file
            .format
            .as_deref()
            .and_then(FileKind::from_extension)
            .ok_or("Unsupported edition")?;
        let dir = super::downloads()?;
        let account_dir = dir.join(".editions").join(self.account);
        fs::create_dir_all(&account_dir)?;
        let index = account_dir.join(format!("{}.json", self.file.id));
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(index.with_extension("lock"))?;
        loop {
            botui_transfer::check_cancel(cancel)?;
            match lock.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(40)),
                Err(fs::TryLockError::Error(e)) => return Err(e.into()),
            }
        }

        let existing: Option<Edition> = match fs::File::open(&index) {
            Ok(f) => serde_json::from_reader(f).ok(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if let Some(path) = offline::legacy_file(&dir, self.account, self.file.id, kind)? {
                    let edition = Edition::from_path(&path, None, None)?;
                    edition.record(&index)?;
                    Some(edition)
                } else {
                    None
                }
            }
            Err(e) => return Err(e.into()),
        };
        let mut local =
            existing.and_then(|edition| Some((edition.local_file(&dir, kind)?, edition)));
        if let Some((path, edition)) = &local {
            let changed = self
                .version
                .zip(edition.content_version.as_deref())
                .is_some_and(|(a, b)| a != b)
                || self
                    .file
                    .size_bytes
                    .is_some_and(|bytes| bytes != edition.bytes);
            let bind_version = edition.content_version.is_none()
                && self.version.is_some()
                && edition.etag.is_some();
            if !self.check_update && !changed && !bind_version {
                return Ok(path.clone());
            }
        }
        let tag = local.as_ref().and_then(|(_, e)| e.etag.as_deref());
        let body = session.with_token(|a, t| a.edition(t, self.file.id, tag))?;
        botui_transfer::check_cancel(cancel)?;
        let Some(body) = body else {
            let (path, mut edition) = local
                .take()
                .ok_or("Missing local edition for server response")?;
            if let Some(version) = self.version {
                edition.content_version = Some(version.to_owned());
                edition.record(&index)?;
            }
            return Ok(path);
        };
        let limit = super::download_limit()?;
        if let Some(size) = self.file.size_bytes {
            botui_transfer::check_size(size, limit)?;
        }
        let mut tmp = NamedTempFile::new_in(&dir)?;
        let mut digest = Sha256::new();
        digest.update(self.account);
        digest.update(self.file.id.to_be_bytes());
        let etag = body.etag().map(str::to_owned);
        let mut writer = DigestWriter {
            file: tmp.as_file_mut(),
            digest,
        };
        body.write_to(&mut writer, limit, cancel, progress)?;
        let hash = writer.digest.finalize();
        let path = super::publish(tmp, self.title, kind, &hash, cancel)?;
        Edition::from_path(&path, self.version, etag)?.record(&index)?;
        Ok(path)
    }
}
