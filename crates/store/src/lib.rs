//! Device-local resume points. A display anchor is never a BookOrbit CFI.
mod notes;
use botui_core::Anchor;
pub use notes::Note;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

pub struct Saved {
    pub anchor: Anchor,
    pub spread: bool,
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
    theme: String,
    #[serde(default)]
    marks: Vec<(usize, usize)>,
    #[serde(default)]
    margin: i8,
}

pub struct LocalState {
    path: PathBuf,
}

fn state_dir() -> io::Result<PathBuf> {
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
            percent: percent.round().clamp(0., 100.) as u8,
            modified,
            read_at: std::time::SystemTime::now(),
        };
        let mut tmp = NamedTempFile::new_in(&dir)?;
        serde_json::to_writer(tmp.as_file_mut(), &entry)?;
        tmp.persist(dir.join(format!("{key:x}.json")))?;
        Ok(())
    }
    pub fn for_root(root: &Path) -> io::Result<Vec<Self>> {
        let root = root.canonicalize()?;
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
                    && v.path.starts_with(&root)
                    && v.path.is_file()
                    && v.path.metadata().and_then(|m| m.modified()).ok() == v.modified
            })
            .collect();
        items.sort_by_key(|v| std::cmp::Reverse(v.read_at));
        Ok(items)
    }
}
impl LocalState {
    /// Identical files share a resume point even after a move; revised editions do not.
    pub fn for_file(path: &Path) -> io::Result<Self> {
        let state = state_dir()?;
        Self::in_directory(path, &state.join("botui").join("positions"))
    }

    fn in_directory(path: &Path, dir: &Path) -> io::Result<Self> {
        let mut file = fs::File::open(path)?;
        let mut hash = Sha256::new();
        io::copy(&mut file, &mut hash)?;
        Ok(Self {
            path: dir.join(format!("{:x}.json", hash.finalize())),
        })
    }

    pub fn load(&self) -> io::Result<Option<Saved>> {
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
            margin: record.margin.clamp(-2, 4),
            theme: record.theme,
            marks: record
                .marks
                .into_iter()
                .map(|(block, byte)| Anchor { block, byte })
                .collect(),
        }))
    }

    pub fn save(&self, saved: &Saved) -> io::Result<()> {
        let dir = self.path.parent().expect("state path includes directory");
        fs::create_dir_all(dir)?;
        let mut tmp = NamedTempFile::new_in(dir)?;
        serde_json::to_writer(
            tmp.as_file_mut(),
            &Record {
                version: 1,
                block: saved.anchor.block,
                byte: saved.anchor.byte,
                spread: saved.spread,
                margin: saved.margin,
                theme: saved.theme.clone(),
                marks: saved.marks.iter().map(|a| (a.block, a.byte)).collect(),
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
    fn matches_bytes_across_moves_but_never_another_edition() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("книга.epub");
        let dir = tmp.path().join("state");
        fs::write(&path, b"first edition")?;
        let first = LocalState::in_directory(&path, &dir)?;
        first.save(&Saved {
            anchor: Anchor { block: 5, byte: 14 },
            spread: true,
            margin: 0,
            theme: "sepia".into(),
            marks: vec![],
        })?;
        let renamed = tmp.path().join("другое имя.epub");
        fs::rename(&path, &renamed)?;
        let after = LocalState::in_directory(&renamed, &dir)?.load()?.unwrap();
        assert_eq!(
            (after.anchor.block, after.anchor.byte, after.spread),
            (5, 14, true)
        );
        fs::write(&path, b"revised edition")?;
        assert!(LocalState::in_directory(&path, &dir)?.load()?.is_none());
        Ok(())
    }
}
