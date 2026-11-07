//! User-editable preferences. Old installs still read their connection and per-book state.
use ratatui::{Frame, style::Color};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Settings {
    pub library_root: Option<PathBuf>,
    pub zen: bool,
    pub local_grid: bool,
    pub preserve_printed_lines: bool,
    pub opaque_background: bool,
    pub theme: Option<String>,
    #[serde(flatten)]
    extra: std::collections::BTreeMap<String, serde_json::Value>,
}

static OPAQUE: AtomicBool = AtomicBool::new(false);

pub fn path() -> io::Result<PathBuf> {
    Ok(crate::connection::settings_file()?.with_file_name("config.json"))
}

impl Settings {
    pub fn load() -> io::Result<Self> {
        match fs::File::open(path()?) {
            Ok(file) => Ok(serde_json::from_reader(file)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }
    pub fn update(change: impl FnOnce(&mut Self)) -> io::Result<Self> {
        let path = path()?;
        let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(dir)?;
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(".config.lock"))?;
        lock.lock()?;
        let mut settings = Self::load()?;
        change(&mut settings);
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer_pretty(tmp.as_file_mut(), &settings)?;
        tmp.persist(path)?;
        settings.activate();
        Ok(settings)
    }
    pub fn activate(&self) {
        OPAQUE.store(self.opaque_background, Ordering::Relaxed);
    }
}
pub fn paint(frame: &mut Frame) {
    if OPAQUE.load(Ordering::Relaxed) {
        for cell in &mut frame.buffer_mut().content {
            if cell.bg == Color::Reset {
                cell.bg = Color::Rgb(31, 33, 37);
                if cell.fg == Color::Reset {
                    cell.fg = Color::Rgb(231, 229, 220);
                }
            }
        }
    }
}
