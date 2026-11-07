//! Staging and publication of complete downloads.
pub(crate) mod native;
use crate::catalog::BookFile;
use botui_bookorbit::{BookOrbit, Result};
use botui_formats::FileKind;
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::{Builder, NamedTempFile, TempPath};

pub(crate) fn download_limit() -> io::Result<u64> {
    Ok(u64::from(crate::settings::Settings::load()?.max_download_mib) * botui_transfer::MIB)
}

pub(crate) fn cache_dir() -> io::Result<std::path::PathBuf> {
    use std::path::PathBuf;
    let root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))
        .ok_or(io::ErrorKind::NotFound)?;
    Ok(root.join("botui"))
}

/// Large editions are staged in the user's disk cache instead of the system
/// temporary directory, which is often a small RAM-backed filesystem.
pub(crate) fn staging(kind: FileKind) -> io::Result<NamedTempFile> {
    let dir = cache_dir()?.join("downloads");
    std::fs::create_dir_all(&dir)?;
    Builder::new()
        .prefix("edition-")
        .suffix(&format!(".{}", kind.label().to_ascii_lowercase()))
        .tempfile_in(dir)
}

pub(crate) fn fetch(
    api: &BookOrbit,
    token: &str,
    file: &BookFile,
    cancelled: &AtomicBool,
    progress: &botui_transfer::Progress,
) -> Result<TempPath> {
    let kind = file
        .format
        .as_deref()
        .and_then(FileKind::from_extension)
        .ok_or("Book information is available for this format")?;
    let limit = download_limit()?;
    if let Some(size) = file.size_bytes {
        botui_transfer::check_size(size, limit)?;
    }
    let mut destination = staging(kind)?;
    api.download(
        token,
        file.id,
        destination.as_file_mut(),
        limit,
        cancelled,
        progress,
    )?;
    Ok(destination.into_temp_path())
}

pub fn data_dir() -> io::Result<std::path::PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })
        .ok_or(io::ErrorKind::NotFound)?;
    Ok(base.join("botui"))
}

pub fn downloads() -> io::Result<std::path::PathBuf> {
    Ok(data_dir()?.join("downloads"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    BookOrbit,
    Opds,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::BookOrbit => "BookOrbit offline downloads",
            Self::Opds => "OPDS downloads",
        }
    }

    pub fn files(
        self,
        dir: &std::path::Path,
    ) -> io::Result<std::collections::HashSet<std::path::PathBuf>> {
        let native = native::saved_names(dir)?;
        let mut files = std::collections::HashSet::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_file() || FileKind::from_path(&path).is_none() {
                continue;
            }
            let is_native =
                native.contains(&entry.file_name()) || crate::offline::has_sidecar(&path);
            if is_native == (self == Self::BookOrbit) {
                files.insert(path);
            }
        }
        Ok(files)
    }
}

/// A content suffix distinguishes editions and makes repeated saves idempotent.
/// The offline library is user data; evicting the cover cache cannot remove books.
pub fn keep_opds(
    path: &std::path::Path,
    title: &str,
    format: &str,
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
    publish(file, title, kind, h.finalize().as_slice(), cancel)
}

pub(super) fn publish(
    file: NamedTempFile,
    title: &str,
    kind: FileKind,
    digest: &[u8],
    cancel: &AtomicBool,
) -> Result<std::path::PathBuf> {
    let dir = downloads()?;
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
    let digest: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    let dest = dir.join(format!(
        "{} [{}].{}",
        if title.is_empty() { "Untitled" } else { title },
        digest,
        kind.label().to_lowercase()
    ));
    botui_transfer::check_cancel(cancel)?;
    // The completed bytes determine the name. This also repairs a locally damaged copy.
    file.persist(&dest)?;
    Ok(dest)
}
