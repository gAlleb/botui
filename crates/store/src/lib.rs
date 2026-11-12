//! Device-local resume points. A display anchor is never a BookOrbit CFI.
#[cfg(unix)]
mod fingerprint;
mod notes;
mod stats;
use botui_core::Anchor;
pub use notes::{HighlightColor, Note, PdfRegion};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use stats::{LibraryStats, Stats};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanAppearance {
    pub quarter_turns: u8,
    pub inverted: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub terminal_paper: bool,
}

#[derive(Clone, Default)]
pub struct Saved {
    pub anchor: Anchor,
    pub spread: bool,
    pub scrolling: bool,
    pub scan_offset: u16,
    pub scan_appearance: ScanAppearance,
    pub finished: bool,
    pub theme: String,
    pub marks: Vec<Anchor>,
    pub margin: i8,
}

#[derive(Serialize, Deserialize)]
struct Record {
    version: u8,
    block: usize,
    byte: usize,
    spread: bool,
    #[serde(default)]
    scrolling: bool,
    #[serde(default)]
    scan_offset: u16,
    #[serde(default)]
    scan_appearance: ScanAppearance,
    #[serde(default)]
    finished: bool,
    theme: String,
    #[serde(default)]
    marks: Vec<(usize, usize)>,
    #[serde(default)]
    margin: i8,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct ReadingPoint {
    anchor: Anchor,
    scan_offset: u16,
    finished: bool,
}
impl From<&Saved> for ReadingPoint {
    fn from(s: &Saved) -> Self {
        Self {
            anchor: s.anchor,
            scan_offset: s.scan_offset,
            finished: s.finished,
        }
    }
}

pub struct LocalState {
    path: PathBuf,
    // The snapshot last observed by this reader, including the initially empty state.
    baseline: std::cell::Cell<Option<ReadingPoint>>,
}

pub fn state_dir() -> io::Result<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".local/state"))
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Cannot locate the reading state directory",
            )
        })
}

#[derive(Serialize, Deserialize)]
pub struct HistoryEntry {
    pub path: PathBuf,
    pub percent: u8,
    pub modified: Option<std::time::SystemTime>,
    pub read_at: std::time::SystemTime,
}
impl HistoryEntry {
    pub fn record(path: &Path, percent: f64) -> io::Result<()> {
        let path = path.canonicalize()?;
        let dir = state_dir()?.join("botui/history");
        fs::create_dir_all(&dir)?;
        let key = Sha256::digest(path.as_os_str().as_encoded_bytes());
        let modified = path.metadata()?.modified().ok();
        let entry = Self {
            path,
            percent: if percent >= 100. {
                100
            } else {
                percent.round().clamp(0., 99.) as u8
            },
            modified,
            read_at: std::time::SystemTime::now(),
        };
        let mut tmp = NamedTempFile::new_in(&dir)?;
        serde_json::to_writer(tmp.as_file_mut(), &entry)?;
        tmp.persist(dir.join(format!("{key:x}.json")))?;
        Ok(())
    }
    pub fn for_root(root: &Path) -> io::Result<Vec<Self>> {
        Self::unfinished(Some(&root.canonicalize()?))
    }
    /// Only durable local paths are recorded; never reopen a server's temporary download.
    pub fn last_unfinished() -> io::Result<Option<Self>> {
        Ok(Self::unfinished(None)?.into_iter().next())
    }
    fn unfinished(root: Option<&Path>) -> io::Result<Vec<Self>> {
        let dir = state_dir()?.join("botui/history");
        let files = match fs::read_dir(dir) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        let mut items: Vec<Self> = files
            .flatten()
            .filter_map(|f| serde_json::from_reader(fs::File::open(f.path()).ok()?).ok())
            .filter(|v: &Self| {
                v.percent < 100
                    && root.is_none_or(|r| v.path.starts_with(r))
                    && v.path.is_file()
                    && v.path.metadata().and_then(|m| m.modified()).ok() == v.modified
            })
            .collect();
        items.sort_by_key(|v| std::cmp::Reverse(v.read_at));
        Ok(items)
    }
}
fn hash_file(file: &mut fs::File) -> io::Result<String> {
    let mut digest = Sha256::new();
    io::copy(file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}
impl LocalState {
    /// Identical files share a resume point even after a move; revised editions do not.
    pub fn for_file(path: &Path) -> io::Result<Self> {
        let state = state_dir()?;
        let dir = state.join("botui/positions");
        let cache = env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .map(|p| p.join(".cache"))
            })
            .map(|p| p.join("botui/file-digests"));
        Self::in_directory(path, &dir, cache.as_deref())
    }

    pub fn in_directory(path: &Path, dir: &Path, cache: Option<&Path>) -> io::Result<Self> {
        let mut file = fs::File::open(path)?;
        #[cfg(unix)]
        let key = fingerprint::digest(&mut file, cache)?;
        #[cfg(not(unix))]
        let key = {
            let _ = cache;
            hash_file(&mut file)?
        };
        Ok(Self {
            path: dir.join(format!("{key}.json")),
            baseline: Default::default(),
        })
    }

    pub fn load(&self) -> io::Result<Option<Saved>> {
        let current = self.read_saved()?;
        self.baseline.set(Some(
            current.as_ref().map(ReadingPoint::from).unwrap_or_default(),
        ));
        Ok(current)
    }
    fn read_saved(&self) -> io::Result<Option<Saved>> {
        let file = match fs::File::open(&self.path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            other => other?,
        };
        if file.metadata()?.len() > 256 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Reading state file is too large",
            ));
        }
        let record: Record = serde_json::from_reader(file)?;
        if record.version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unknown reading state version",
            ));
        }
        Ok(Some(Saved {
            anchor: Anchor {
                block: record.block,
                byte: record.byte,
            },
            spread: record.spread,
            scrolling: record.scrolling,
            scan_offset: record.scan_offset,
            scan_appearance: record.scan_appearance,
            finished: record.finished,
            margin: record.margin.clamp(-2, 4),
            theme: record.theme,
            marks: record
                .marks
                .into_iter()
                .map(|(block, byte)| Anchor { block, byte })
                .collect(),
        }))
    }

    fn lock(&self) -> io::Result<fs::File> {
        let dir = self.path.parent().expect("state path includes directory");
        fs::create_dir_all(dir)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.path.with_extension("position.lock"))?;
        lock.lock()?;
        Ok(lock)
    }

    /// A position checkpoint from another window may carry an older bookmark list.
    pub fn save_position(&self, saved: &Saved) -> io::Result<()> {
        let _lock = self.lock()?;
        let current = self.read_saved()?;
        let point = ReadingPoint::from(saved);
        let mut merged;
        let snapshot = if self.baseline.get() == Some(point)
            && let Some(disk) = &current
            && ReadingPoint::from(disk) != point
        {
            // A theme/width change in a stale window must not rewind the book.
            merged = saved.clone();
            merged.anchor = disk.anchor;
            merged.scan_offset = disk.scan_offset;
            merged.finished = disk.finished;
            &merged
        } else {
            saved
        };
        let marks = current
            .as_ref()
            .map_or(saved.marks.as_slice(), |s| &s.marks);
        self.write(snapshot, marks)?;
        self.baseline.set(Some(point));
        Ok(())
    }

    pub fn set_bookmark(
        &self,
        initial: &Saved,
        at: Anchor,
        present: bool,
    ) -> io::Result<Vec<Anchor>> {
        let _lock = self.lock()?;
        let mut saved = self.read_saved()?.unwrap_or_else(|| initial.clone());
        saved.marks.sort_unstable();
        saved.marks.dedup();
        match (saved.marks.binary_search(&at), present) {
            (Err(i), true) if saved.marks.len() < 2048 => saved.marks.insert(i, at),
            (Err(_), true) => return Err(io::Error::other("Bookmark limit reached (2048)")),
            (Ok(i), false) => {
                saved.marks.remove(i);
            }
            _ => return Ok(saved.marks),
        }
        self.write(&saved, &saved.marks)?;
        Ok(saved.marks)
    }

    fn write(&self, saved: &Saved, marks: &[Anchor]) -> io::Result<()> {
        let dir = self.path.parent().expect("state path includes directory");
        let mut tmp = NamedTempFile::new_in(dir)?;
        serde_json::to_writer(
            tmp.as_file_mut(),
            &Record {
                version: 1,
                block: saved.anchor.block,
                byte: saved.anchor.byte,
                spread: saved.spread,
                scrolling: saved.scrolling,
                scan_offset: saved.scan_offset,
                scan_appearance: saved.scan_appearance,
                finished: saved.finished,
                margin: saved.margin,
                theme: saved.theme.clone(),
                marks: marks.iter().map(|a| (a.block, a.byte)).collect(),
            },
        )?;
        tmp.persist(&self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_appearance_in_a_second_window_preserves_a_newer_reading_point() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let window = || LocalState {
            path: directory.path().join("same-book.json"),
            baseline: Default::default(),
        };
        let a = window();
        let b = window();
        let mut first = a.load()?.unwrap_or_default();
        let mut second = b.load()?.unwrap_or_default();
        first.anchor = Anchor {
            block: 25,
            byte: 10,
        };
        first.scan_offset = 3400;
        a.save_position(&first)?;
        second.theme = "sepia".into();
        b.save_position(&second)?;
        second.spread = true;
        b.save_position(&second)?;
        let actual = window().load()?.unwrap();
        assert_eq!(actual.anchor, first.anchor);
        assert_eq!(actual.scan_offset, 3400);
        assert_eq!(actual.theme, "sepia");
        // Deliberately reading backwards still updates other windows.
        second.anchor = Anchor { block: 3, byte: 5 };
        b.save_position(&second)?;
        assert_eq!(window().load()?.unwrap().anchor, second.anchor);
        Ok(())
    }

    #[test]
    fn stale_positions_preserve_bookmark_edits_from_both_readers() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let state = LocalState {
            path: directory.path().join("edition.json"),
            baseline: Default::default(),
        };
        fs::write(
            &state.path,
            r#"{"version":1,"block":0,"byte":0,"spread":false,"theme":"sepia","marks":[[1,9]]}"#,
        )?;
        let old = state.load()?.unwrap();
        let a = Anchor { block: 1, byte: 9 };
        let b = Anchor { block: 3, byte: 1 };
        let c = Anchor { block: 5, byte: 2 };
        assert_eq!(old.scan_appearance, ScanAppearance::default());
        state.set_bookmark(&old, b, true)?;
        state.set_bookmark(&old, c, true)?;
        state.set_bookmark(&old, a, false)?;
        state.save_position(&old)?;
        assert_eq!(state.load()?.unwrap().marks, vec![b, c]);
        Ok(())
    }

    #[test]
    fn matches_bytes_across_moves_but_never_another_edition() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("книга.epub");
        let dir = tmp.path().join("state");
        fs::write(&path, b"first edition")?;
        let first = LocalState::in_directory(&path, &dir, None)?;
        first.save_position(&Saved {
            anchor: Anchor { block: 5, byte: 14 },
            spread: true,
            scrolling: false,
            scan_offset: 0,
            scan_appearance: ScanAppearance::default(),
            finished: false,
            margin: 0,
            theme: "sepia".into(),
            marks: vec![],
        })?;
        let renamed = tmp.path().join("другое имя.epub");
        fs::rename(&path, &renamed)?;
        let after = LocalState::in_directory(&renamed, &dir, None)?
            .load()?
            .unwrap();
        assert_eq!(
            (after.anchor.block, after.anchor.byte, after.spread),
            (5, 14, true)
        );
        fs::write(&path, b"revised edition")?;
        assert!(
            LocalState::in_directory(&path, &dir, None)?
                .load()?
                .is_none()
        );
        Ok(())
    }
}
