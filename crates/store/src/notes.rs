use super::LocalState;
use botui_core::Anchor;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};
use tempfile::NamedTempFile;

#[derive(Clone, Deserialize, Serialize)]
pub struct Note {
    block: usize,
    byte: usize,
    pub body: String,
}
impl Note {
    pub fn anchor(&self) -> Anchor {
        Anchor {
            block: self.block,
            byte: self.byte,
        }
    }
}
impl LocalState {
    fn note_path(&self) -> PathBuf {
        self.path.with_extension("notes")
    }
    pub fn notes(&self) -> io::Result<Vec<Note>> {
        match fs::File::open(self.note_path()) {
            Ok(f) => Ok(serde_json::from_reader(f)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e),
        }
    }
    pub fn edit_note(&self, at: Anchor, body: String) -> io::Result<Vec<Note>> {
        let path = self.note_path();
        let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(dir)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path.with_extension("notes.lock"))?;
        lock.lock()?;
        let mut notes = self.notes()?;
        notes.retain(|n| n.anchor() != at);
        if !body.trim().is_empty() {
            notes.push(Note {
                block: at.block,
                byte: at.byte,
                body,
            });
            notes.sort_by_key(Note::anchor);
        }
        let mut tmp = NamedTempFile::new_in(dir)?;
        serde_json::to_writer(tmp.as_file_mut(), &notes)?;
        tmp.persist(path)?;
        Ok(notes)
    }
}
