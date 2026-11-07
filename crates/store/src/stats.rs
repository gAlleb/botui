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
