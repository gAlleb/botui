use super::LocalState;
use botui_core::{Anchor, TextRange};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Note {
    block: usize,
    byte: usize,
    pub body: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    end: Option<(usize, usize)>,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub color: Option<HighlightColor>,
    #[serde(default)]
    pub updated: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
    Purple,
}
impl HighlightColor {
    pub const ALL: [Self; 5] = [
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Pink,
        Self::Purple,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Pink => "Pink",
            Self::Purple => "Purple",
        }
    }
}
impl Note {
    pub fn range(&self) -> Option<TextRange> {
        let (block, byte) = self.end?;
        Some(TextRange::new(self.anchor(), Anchor { block, byte }))
    }
    pub fn anchor(&self) -> Anchor {
        Anchor {
            block: self.block,
            byte: self.byte,
        }
    }
}
impl LocalState {
    pub fn named_marks(&self) -> io::Result<std::collections::BTreeMap<char, Anchor>> {
        match fs::File::open(self.path.with_extension("named-marks")) {
            Ok(f) => {
                let marks: std::collections::BTreeMap<char, (usize, usize)> =
                    serde_json::from_reader(f)?;
                Ok(marks
                    .into_iter()
                    .map(|(c, (block, byte))| (c, Anchor { block, byte }))
                    .collect())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Default::default()),
            Err(e) => Err(e),
        }
    }
    pub fn set_named_mark(&self, name: char, at: Anchor) -> io::Result<()> {
        let path = self.path.with_extension("named-marks");
        let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(dir)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path.with_extension("named-marks.lock"))?;
        lock.lock()?;
        let mut marks = self.named_marks()?;
        marks.insert(name, at);
        let data: std::collections::BTreeMap<_, _> = marks
            .into_iter()
            .map(|(c, a)| (c, (a.block, a.byte)))
            .collect();
        let mut tmp = NamedTempFile::new_in(dir)?;
        serde_json::to_writer(tmp.as_file_mut(), &data)?;
        tmp.persist(path)?;
        Ok(())
    }
    fn note_path(&self) -> PathBuf {
        self.path.with_extension("notes")
    }
    pub fn notes(&self) -> io::Result<Vec<Note>> {
        match fs::File::open(self.note_path()) {
            Ok(f) => {
                let mut notes: Vec<Note> = serde_json::from_reader(f)?;
                for note in &mut notes {
                    if note.id.is_empty() {
                        note.id = format!("legacy-{}-{}", note.block, note.byte);
                    }
                }
                Ok(notes)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e),
        }
    }
    pub fn edit_note(&self, at: Anchor, body: String) -> io::Result<Vec<Note>> {
        self.update_notes(|notes| {
            notes.retain(|n| n.anchor() != at || n.end.is_some());
            if !body.trim().is_empty() {
                notes.push(Note {
                    block: at.block,
                    byte: at.byte,
                    body,
                    id: format!("legacy-{}-{}", at.block, at.byte),
                    end: None,
                    quote: String::new(),
                    color: None,
                    updated: now(),
                });
            }
        })
    }
    /// Edit by identity, so overlapping highlights and point notes stay independent.
    pub fn annotate(
        &self,
        id: Option<&str>,
        range: TextRange,
        quote: String,
        color: Option<HighlightColor>,
        body: String,
    ) -> io::Result<Vec<Note>> {
        if range.start >= range.end || quote.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Select some text first",
            ));
        }
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = id.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "{}-{}-{}",
                now(),
                std::process::id(),
                SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            )
        });
        self.update_notes(|notes| {
            notes.retain(|n| n.id != id);
            notes.push(Note {
                block: range.start.block,
                byte: range.start.byte,
                end: Some((range.end.block, range.end.byte)),
                id,
                quote,
                color,
                body,
                updated: now(),
            });
        })
    }
    pub fn delete_note(&self, id: &str) -> io::Result<Vec<Note>> {
        self.update_notes(|notes| notes.retain(|n| n.id != id))
    }
    fn update_notes(&self, update: impl FnOnce(&mut Vec<Note>)) -> io::Result<Vec<Note>> {
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
        update(&mut notes);
        notes.sort_by_key(Note::anchor);
        let mut tmp = NamedTempFile::new_in(dir)?;
        serde_json::to_writer(tmp.as_file_mut(), &notes)?;
        tmp.persist(path)?;
        Ok(notes)
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_and_overlapping_annotations_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalState {
            path: dir.path().join("reading.json"),
            baseline: Default::default(),
        };
        fs::write(
            store.note_path(),
            r#"[{"block":0,"byte":0,"body":"old note"}]"#,
        )
        .unwrap();
        let range = TextRange::new(Anchor::default(), Anchor { block: 0, byte: 6 });
        let notes = store
            .annotate(
                None,
                range,
                "Привет".into(),
                Some(HighlightColor::Yellow),
                "new".into(),
            )
            .unwrap();
        assert_eq!(notes.len(), 2);
        let id = notes
            .iter()
            .find(|n| n.range().is_some())
            .unwrap()
            .id
            .clone();
        store
            .edit_note(Anchor::default(), "edited old note".into())
            .unwrap();
        let notes = store
            .annotate(
                Some(&id),
                range,
                "Привет".into(),
                Some(HighlightColor::Blue),
                "edited range".into(),
            )
            .unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(
            notes.iter().find(|n| n.id == id).unwrap().color,
            Some(HighlightColor::Blue)
        );
        assert_eq!(store.delete_note(&id).unwrap()[0].body, "edited old note");
    }
}
