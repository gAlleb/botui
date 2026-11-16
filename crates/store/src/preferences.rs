//! Per-edition reading preferences live beside the resume record. Each edit merges
//! under the edition lock, so two open readers can change independent fields.
use super::LocalState;
use serde::{Serialize, de::DeserializeOwned};
use std::{fs, io};
use tempfile::NamedTempFile;

impl LocalState {
    pub fn book_preferences<T: DeserializeOwned + Default>(&self) -> io::Result<T> {
        match fs::File::open(self.path.with_extension("preferences")) {
            Ok(f) => Ok(serde_json::from_reader(f)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(e),
        }
    }

    pub fn update_book_preferences<T>(&self, change: impl FnOnce(&mut T)) -> io::Result<()>
    where
        T: DeserializeOwned + Default + Serialize + PartialEq,
    {
        let _lock = self.lock()?;
        let mut current: T = self.book_preferences()?;
        change(&mut current);
        let path = self.path.with_extension("preferences");
        if current == T::default() {
            return match fs::remove_file(path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                result => result,
            };
        }
        let mut tmp = NamedTempFile::new_in(path.parent().expect("reading state directory"))?;
        serde_json::to_writer(tmp.as_file_mut(), &current)?;
        tmp.persist(path)?;
        Ok(())
    }
}
