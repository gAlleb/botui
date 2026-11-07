//! A remote book is readable only once its private, format-suffixed temporary file is complete.
use crate::catalog::BookFile;
use botui_bookorbit::{BookOrbit, Result};
use botui_formats::FileKind;
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::{Builder, TempPath};

const MAX_FILE: u64 = 512 << 20;

pub(crate) fn fetch(
    api: &BookOrbit,
    token: &str,
    file: &BookFile,
    cancelled: &AtomicBool,
) -> Result<TempPath> {
    let kind = file
        .format
        .as_deref()
        .and_then(FileKind::from_extension)
        .ok_or("Book information is available for this format")?;
    let suffix = format!(".{}", kind.label().to_ascii_lowercase());
    if file.size_bytes.is_some_and(|size| size > MAX_FILE) {
        return Err("Book exceeds the 512 MiB download limit".into());
    }
    let mut staging = Builder::new().prefix("botui-").suffix(&suffix).tempfile()?;
    let mut writer = BookWriter {
        file: staging.as_file_mut(),
        cancelled,
    };
    writer.check()?;
    let count = api.download(token, file.id, &mut writer, MAX_FILE)?;
    writer.check()?;
    if count == 0 {
        return Err("Server returned an empty book".into());
    }
    Ok(staging.into_temp_path())
}

struct BookWriter<'a> {
    file: &'a mut std::fs::File,
    cancelled: &'a AtomicBool,
}

impl BookWriter<'_> {
    fn check(&self) -> io::Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(io::Error::other("Download cancelled"))
        } else {
            Ok(())
        }
    }
}

impl Write for BookWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.check()?;
        self.file.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub fn downloads() -> io::Result<std::path::PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })
        .ok_or(io::ErrorKind::NotFound)?;
    Ok(base.join("botui/downloads"))
}

/// A content suffix distinguishes editions and makes repeated saves idempotent.
/// The offline library is user data; evicting the cover cache cannot remove books.
pub fn keep(
    path: &std::path::Path,
    title: &str,
    format: &str,
    source: Option<(&str, u64)>,
    cancel: &AtomicBool,
) -> Result<std::path::PathBuf> {
    use sha2::{Digest, Sha256};
    use std::{fs, io::Read};
    let dir = downloads()?;
    fs::DirBuilder::new().recursive(true).create(&dir)?;
    let kind = FileKind::from_extension(format).ok_or("Unknown file format")?;
    let mut input = fs::File::open(path)?;
    let mut file = tempfile::NamedTempFile::new_in(&dir)?;
    let mut h = Sha256::new();
    if let Some((account, file)) = source {
        h.update(account);
        h.update(file.to_be_bytes());
    }
    let mut buf = [0u8; 65536];
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Download cancelled".into());
        }
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        h.update(&buf[..n]);
    }
    let title: String = title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\') {
                ' '
            } else {
                c
            }
        })
        .take_while({
            let mut bytes = 0;
            move |c| {
                bytes += c.len_utf8();
                bytes <= 165
            }
        })
        .collect();
    let title = title.trim().trim_matches('.');
    let digest = format!("{:x}", h.finalize());
    let dest = dir.join(format!(
        "{} [{}].{}",
        if title.is_empty() { "Untitled" } else { title },
        &digest[..16],
        kind.label().to_lowercase()
    ));
    match file.persist_noclobber(&dest) {
        Ok(_) => Ok(dest),
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => Ok(dest),
        Err(e) => Err(e.error.into()),
    }
}
