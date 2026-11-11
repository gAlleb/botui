//! Personal collections reference local files by path and are shared by local library roots.
use super::{Entry, LocalView, Picker};
use crate::{AppTerminal, prompt};
use botui_formats::FileKind;
use ratatui::widgets::ListState;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

type Collections = BTreeMap<String, BTreeSet<PathBuf>>;

fn read(file: &Path) -> io::Result<Collections> {
    match fs::File::open(file) {
        Ok(f) => Ok(serde_json::from_reader(f)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Collections::new()),
        Err(e) => Err(e),
    }
}

fn file() -> io::Result<PathBuf> {
    Ok(crate::cache::data_dir()?.join("collections.json"))
}

fn edit_collections(edit: impl FnOnce(&mut Collections) -> io::Result<()>) -> io::Result<()> {
    let dir = crate::cache::data_dir()?;
    fs::create_dir_all(&dir)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("collections.lock"))?;
    lock.lock()?;
    let file = file()?;
    let mut all = read(&file)?;
    edit(&mut all)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer(tmp.as_file_mut(), &all)?;
    tmp.persist(file)?;
    Ok(())
}

fn update(name: &str, create: bool, member: Option<(&Path, bool)>) -> io::Result<()> {
    edit_collections(|all| {
        if create && all.contains_key(name) {
            return Err(io::Error::other(
                "A collection with this name already exists.",
            ));
        }
        let books = if create {
            all.entry(name.to_owned()).or_default()
        } else {
            all.get_mut(name)
                .ok_or_else(|| io::Error::other("Collection no longer exists."))?
        };
        if let Some((path, add)) = member {
            if add {
                books.insert(path.to_owned());
            } else {
                books.remove(path);
            }
        }
        Ok(())
    })
}

impl Picker {
    pub(super) fn delete_collection(&mut self, term: &mut AppTerminal) -> io::Result<()> {
        let LocalView::Collection(name) = &self.view else {
            return Ok(());
        };
        if prompt::confirm_delete_collection(term, name, "this device")? {
            edit_collections(|all| {
                all.remove(name);
                Ok(())
            })?;
            self.browse(LocalView::Directory)?;
        }
        Ok(())
    }

    pub(super) fn show_collection(&mut self, name: String) -> io::Result<()> {
        let mut all = read(&file()?)?;
        self.remember_directory();
        self.index.clear();
        self.entries = all
            .remove(&name)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| {
                let kind = FileKind::from_path(&path)?;
                let label = path.file_name()?.to_string_lossy().into_owned();
                Some(Entry {
                    path,
                    name: label,
                    kind: Some(kind),
                    meta: None,
                    percent: None,
                })
            })
            .collect();
        self.list = ListState::default().with_selected((!self.entries.is_empty()).then_some(0));
        self.view = LocalView::Collection(name);
        self.navigation.show_shelf(&self.view);
        self.error = None;
        self.warm_directory();
        Ok(())
    }

    pub(super) fn collections_menu(
        &mut self,
        term: &mut AppTerminal,
        book: Option<PathBuf>,
    ) -> io::Result<()> {
        let all = read(&file()?)?;
        let mut rows: Vec<String> = all
            .iter()
            .map(|(name, files)| match &book {
                Some(path) if files.contains(path) => format!("Remove from {name}"),
                Some(_) => format!("Add to {name}"),
                None => format!("{name}  ·  {}", files.len()),
            })
            .collect();
        rows.push("New collection…".into());
        let title = if book.is_some() {
            "Collections for this book"
        } else {
            "Local collections"
        };
        let Some(i) = prompt::choose(term, title, &rows, 0)? else {
            return Ok(());
        };
        if let Some((name, files)) = all.iter().nth(i) {
            if let Some(path) = &book {
                update(name, false, Some((path, !files.contains(path))))?;
            } else {
                return self.show_collection(name.clone());
            }
        } else if let Some(name) = prompt::collection_name(term, |f| self.draw(f))? {
            update(&name, true, book.as_deref().map(|p| (p, true)))?;
            if book.is_none() {
                return self.show_collection(name);
            }
        } else {
            return Ok(());
        }
        if let LocalView::Collection(name) = &self.view {
            self.show_collection(name.clone())?;
        }
        self.error = Some("Collection updated.".into());
        Ok(())
    }
}
