use super::LocalState;
use botui_core::{Anchor, TextRange};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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
    /// PDF word rectangles in unrotated, normalized page coordinates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pdf_regions: Vec<PdfRegion>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PdfRegion {
    /// Physical page, starting at one.
    pub page: u32,
    pub bounds: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
    Purple,
    Orange,
    Red,
    Olive,
    Cyan,
    Gray,
    Rgb([u8; 3]),
}
impl HighlightColor {
    pub const ALL: [Self; 10] = [
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Pink,
        Self::Purple,
        Self::Orange,
        Self::Red,
        Self::Olive,
        Self::Cyan,
        Self::Gray,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Pink => "Pink",
            Self::Purple => "Purple",
            Self::Orange => "Orange",
            Self::Red => "Red",
            Self::Olive => "Olive",
            Self::Cyan => "Cyan",
            Self::Gray => "Gray",
            Self::Rgb(_) => "Custom",
        }
    }
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Yellow => [0xFA, 0xCC, 0x15],
            Self::Green => [0x4A, 0xDE, 0x80],
            Self::Blue => [0x38, 0xBD, 0xF8],
            Self::Pink => [0xF4, 0x72, 0xB6],
            Self::Purple => [0xC0, 0x84, 0xFC],
            Self::Orange => [0xFB, 0x92, 0x3C],
            Self::Red => [0xF8, 0x71, 0x71],
            Self::Olive => [0x84, 0xCC, 0x16],
            Self::Cyan => [0x22, 0xD3, 0xEE],
            Self::Gray => [0x9C, 0xA3, 0xAF],
            Self::Rgb(rgb) => rgb,
        }
    }
    pub fn hex(self) -> String {
        let [r, g, b] = self.rgb();
        format!("#{r:02X}{g:02X}{b:02X}")
    }
    pub fn from_hex(value: &str) -> Option<Self> {
        let hex = value.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let n = u32::from_str_radix(hex, 16).ok()?;
        let rgb = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        Some(
            Self::ALL
                .into_iter()
                .find(|c| c.rgb() == rgb)
                .unwrap_or(Self::Rgb(rgb)),
        )
    }
}

impl Note {
    pub fn imported(
        id: String,
        range: Option<TextRange>,
        at: Anchor,
        quote: String,
        body: String,
        color: Option<HighlightColor>,
        pdf_regions: Vec<PdfRegion>,
    ) -> Self {
        let at = range.map_or(at, |r| r.start);
        Self {
            block: at.block,
            byte: at.byte,
            end: range.map(|r| (r.end.block, r.end.byte)),
            id,
            quote,
            body,
            color,
            pdf_regions,
            updated: now(),
        }
    }
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
// Sync baselines and visible notes share one atomic replacement. Legacy arrays remain readable.
#[derive(Default, Serialize, Deserialize)]
struct NoteFile {
    notes: Vec<Note>,
    #[serde(default)]
    sync: std::collections::BTreeMap<String, serde_json::Value>,
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
    fn note_file(&self) -> io::Result<NoteFile> {
        let mut file = match fs::File::open(self.note_path()) {
            Ok(f) => {
                #[derive(Deserialize)]
                #[serde(untagged)]
                enum Stored {
                    Legacy(Vec<Note>),
                    Current(NoteFile),
                }
                match serde_json::from_reader(f)? {
                    Stored::Legacy(notes) => NoteFile {
                        notes,
                        ..Default::default()
                    },
                    Stored::Current(file) => file,
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => NoteFile::default(),
            Err(e) => return Err(e),
        };
        for note in &mut file.notes {
            if note.id.is_empty() {
                note.id = format!("legacy-{}-{}", note.block, note.byte);
            }
        }
        Ok(file)
    }
    pub fn notes(&self) -> io::Result<Vec<Note>> {
        Ok(self.note_file()?.notes)
    }
    pub fn annotation_state(&self, scope: &str) -> io::Result<(Vec<Note>, serde_json::Value)> {
        let mut file = self.note_file()?;
        Ok((file.notes, file.sync.remove(scope).unwrap_or_default()))
    }
    /// Serializes sync workers across processes without holding the editor's notes lock.
    pub fn annotation_sync_lock(&self) -> io::Result<fs::File> {
        let path = self.path.with_extension("annotations.lock");
        fs::create_dir_all(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        file.lock()?;
        Ok(file)
    }
    pub fn update_annotations<T>(
        &self,
        scope: &str,
        edit: impl FnOnce(&mut Vec<Note>, &mut serde_json::Value) -> io::Result<T>,
    ) -> io::Result<T> {
        self.update_file(|file| edit(&mut file.notes, file.sync.entry(scope.into()).or_default()))
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
                    pdf_regions: vec![],
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
        self.annotate_regions(id, range, quote, color, body, vec![])
    }
    pub fn annotate_regions(
        &self,
        id: Option<&str>,
        range: TextRange,
        quote: String,
        color: Option<HighlightColor>,
        body: String,
        pdf_regions: Vec<PdfRegion>,
    ) -> io::Result<Vec<Note>> {
        if pdf_regions.iter().any(|r| {
            r.page == 0
                || r.bounds
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || r.bounds[0] >= r.bounds[2]
                || r.bounds[1] >= r.bounds[3]
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid PDF annotation coordinates",
            ));
        }
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
                pdf_regions,
            });
        })
    }
    pub fn delete_note(&self, id: &str) -> io::Result<Vec<Note>> {
        self.update_notes(|notes| notes.retain(|n| n.id != id))
    }
    fn update_notes(&self, update: impl FnOnce(&mut Vec<Note>)) -> io::Result<Vec<Note>> {
        self.update_file(|file| {
            update(&mut file.notes);
            file.notes.sort_by_key(Note::anchor);
            Ok(file.notes.clone())
        })
    }
    fn update_file<T>(&self, update: impl FnOnce(&mut NoteFile) -> io::Result<T>) -> io::Result<T> {
        let path = self.note_path();
        let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(dir)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path.with_extension("notes.lock"))?;
        lock.lock()?;
        let mut file = self.note_file()?;
        let result = update(&mut file)?;
        file.notes.sort_by_key(Note::anchor);
        let mut tmp = NamedTempFile::new_in(dir)?;
        // Keep the original on-disk shape until this book actually uses synchronization.
        if file.sync.is_empty() {
            serde_json::to_writer(tmp.as_file_mut(), &file.notes)?;
        } else {
            serde_json::to_writer(tmp.as_file_mut(), &file)?;
        }
        tmp.persist(path)?;
        Ok(result)
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
    fn pdf_ranges_survive_reopen_and_reject_invalid_coordinates() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalState {
            path: dir.path().join("reading.json"),
            baseline: Default::default(),
        };
        let range = TextRange::new(Anchor { block: 2, byte: 7 }, Anchor { block: 3, byte: 10 });
        let regions = vec![
            PdfRegion {
                page: 3,
                bounds: [0.1, 0.2, 0.4, 0.3],
            },
            PdfRegion {
                page: 4,
                bounds: [0.2, 0.1, 0.8, 0.2],
            },
        ];
        let notes = store
            .annotate_regions(
                None,
                range,
                "quoted\n\nwords".into(),
                Some(HighlightColor::Blue),
                "comment".into(),
                regions.clone(),
            )
            .unwrap();
        let reopened = store.notes().unwrap();
        assert_eq!(reopened[0].range(), Some(range));
        assert_eq!(reopened[0].pdf_regions, regions);
        store
            .edit_note(Anchor { block: 2, byte: 0 }, "whole page".into())
            .unwrap();
        assert_eq!(store.notes().unwrap().len(), 2);
        assert!(
            store
                .annotate_regions(
                    Some(&notes[0].id),
                    range,
                    "bad".into(),
                    None,
                    String::new(),
                    vec![PdfRegion {
                        page: 3,
                        bounds: [0., 0., f32::NAN, 1.]
                    }]
                )
                .is_err()
        );
        assert_eq!(store.notes().unwrap()[1].quote, "quoted\n\nwords");
        store.delete_note(&notes[0].id).unwrap();
        assert_eq!(store.notes().unwrap()[0].body, "whole page");
    }
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

#[cfg(test)]
mod color_tests {
    use super::*;
    #[test]
    fn existing_names_and_exact_rgb_colors_survive_storage() {
        assert_eq!(
            serde_json::from_str::<HighlightColor>("\"yellow\"").unwrap(),
            HighlightColor::Yellow
        );
        for c in HighlightColor::ALL
            .into_iter()
            .chain([HighlightColor::Rgb([1, 2, 3])])
        {
            assert_eq!(HighlightColor::from_hex(&c.hex().to_lowercase()), Some(c));
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(serde_json::from_str::<HighlightColor>(&json).unwrap(), c);
        }
        for invalid in ["#1", "#1234567", "#zzzzzz", "#😀12", "123456"] {
            assert_eq!(HighlightColor::from_hex(invalid), None);
        }
    }
}
