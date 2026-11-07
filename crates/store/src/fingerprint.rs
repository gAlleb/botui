//! Preserve content-addressed state without rereading a large unchanged local file at every open.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::Path,
};

const MIN_BYTES: u64 = 16 << 20;

#[derive(Deserialize, Serialize, PartialEq)]
struct Stamp {
    device: u64,
    inode: u64,
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl Stamp {
    fn new(m: &fs::Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            bytes: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
}
#[derive(Deserialize, Serialize)]
struct Cached {
    stamp: Stamp,
    sha256: String,
}

pub(super) fn digest(file: &mut File, cache: Option<&Path>) -> io::Result<String> {
    let meta = file.metadata()?;
    let stamp = Stamp::new(&meta);
    let Some(cache) = cache.filter(|_| {
        meta.is_file()
            && stamp.bytes >= MIN_BYTES
            && stamp.inode != 0
            && (stamp.modified.1 != 0 || stamp.changed.1 != 0)
    }) else {
        return super::hash_file(file);
    };
    let entry = cache.join(format!("{:x}-{:x}.json", stamp.device, stamp.inode));
    let cached: Option<Cached> = File::open(&entry)
        .ok()
        .and_then(|f| serde_json::from_reader(f.take(1024)).ok());
    if let Some(c) = cached
        && c.stamp == stamp
        && c.sha256.len() == 64
        && c.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(c.sha256);
    }
    let sha256 = super::hash_file(file)?;
    if Stamp::new(&file.metadata()?) != stamp {
        return Err(io::Error::other(
            "The book changed while it was being opened",
        ));
    }
    // Read-only/full caches must never prevent opening a book or saving its position.
    if fs::create_dir_all(cache).is_ok()
        && let Ok(mut tmp) = tempfile::NamedTempFile::new_in(cache)
        && serde_json::to_writer(
            tmp.as_file_mut(),
            &Cached {
                stamp,
                sha256: sha256.clone(),
            },
        )
        .is_ok()
    {
        let _ = tmp.persist(&entry);
    }
    Ok(sha256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, Write};
    #[test]
    fn skips_second_read_but_rehashes_rewritten_or_replaced_files() -> io::Result<()> {
        let d = tempfile::tempdir()?;
        let cache = d.path().join("cache");
        let path = d.path().join("large");
        let write_edition = |text: &[u8]| -> io::Result<()> {
            let mut file = File::create(&path)?;
            file.set_len(MIN_BYTES)?;
            file.write_all(text)
        };
        write_edition(b"edition1")?;
        let time = fs::metadata(&path)?.modified()?;
        let a = digest(&mut File::open(&path)?, Some(&cache))?;
        let mut second = File::open(&path)?;
        assert_eq!(digest(&mut second, Some(&cache))?, a);
        assert_eq!(second.stream_position()?, 0);
        write_edition(b"edition2")?;
        File::open(&path)?.set_modified(time)?;
        let b = digest(&mut File::open(&path)?, Some(&cache))?;
        assert_ne!(a, b);
        fs::rename(&path, d.path().join("moved"))?;
        write_edition(b"edition3")?;
        File::open(&path)?.set_modified(time)?;
        let c = digest(&mut File::open(&path)?, Some(&cache))?;
        assert_ne!(b, c);
        assert_eq!(
            digest(&mut File::open(d.path().join("moved"))?, Some(&cache))?,
            b
        );
        Ok(())
    }
}
