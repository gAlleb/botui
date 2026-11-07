use super::LocalState;
use serde::{Deserialize, Serialize};
use std::{fs, io};
#[derive(Default, Deserialize, Serialize)]
pub struct Stats {
    pub sessions: u64,
    pub seconds: u64,
}
impl LocalState {
    pub fn stats(&self) -> io::Result<Stats> {
        match fs::File::open(self.path.with_extension("stats")) {
            Ok(f) => Ok(serde_json::from_reader(f)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Stats::default()),
            Err(e) => Err(e),
        }
    }
    pub fn finish_session(&self, seconds: u64) -> io::Result<()> {
        let path = self.path.with_extension("stats");
        let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(dir)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.path.with_extension("stats.lock"))?;
        lock.lock()?;
        let mut s = self.stats()?;
        s.seconds = s.seconds.saturating_add(seconds);
        s.sessions = s.sessions.saturating_add(1);
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(tmp.as_file_mut(), &s)?;
        tmp.persist(path)?;
        Ok(())
    }
}

#[derive(Default)]
pub struct LibraryStats {
    pub books: u64,
    pub sessions: u64,
    pub seconds: u64,
    pub unreadable: u64,
}
impl LibraryStats {
    pub fn on_device() -> io::Result<Self> {
        let dir = super::state_dir()?.join("botui/positions");
        let mut total = Self::default();
        let entries = match fs::read_dir(dir) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(total),
            Err(e) => return Err(e),
        };
        for e in entries {
            let p = e?.path();
            if p.extension().is_none_or(|ext| ext != "stats") {
                continue;
            }
            match fs::File::open(p).and_then(|f| Ok(serde_json::from_reader::<_, Stats>(f)?)) {
                Ok(s) => {
                    total.books += 1;
                    total.sessions = total.sessions.saturating_add(s.sessions);
                    total.seconds = total.seconds.saturating_add(s.seconds);
                }
                Err(_) => total.unreadable += 1,
            }
        }
        Ok(total)
    }
}
