//! Import the three pre-TOML files once. Keep their exact originals outside the settings directory.
use super::{Settings, apply, read};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use toml_edit::DocumentMut;
const FILES: [&str; 3] = ["config.json", "connection.json", "reader-keys.json"];

fn backup_dir() -> io::Result<PathBuf> {
    Ok(botui_store::state_dir()?.join("botui/config-backup"))
}

fn backup(name: &str, text: &str) -> io::Result<()> {
    let dir = backup_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
    tmp.write_all(text.as_bytes())?;
    tmp.as_file().sync_all()?;
    match tmp.persist_noclobber(&path) {
        Ok(_) => Ok(()),
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists && read(&path)? == text => Ok(()),
        Err(e) => Err(e.error),
    }
}
pub(super) fn import(dir: &Path, doc: &mut DocumentMut, settings: &mut Settings) -> io::Result<()> {
    for name in FILES {
        let text = match read(&dir.join(name)) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let invalid = |_: serde_json::Error| {
            io::Error::other(format!(
                "Could not migrate {name}. Check its JSON syntax and setting types."
            ))
        };
        match name {
            "config.json" => {
                let old: Settings = serde_json::from_str(&text).map_err(invalid)?;
                apply(doc, settings, |s| {
                    s.library_root = old.library_root;
                    s.zen = old.zen;
                    s.local_grid = old.local_grid;
                    s.preserve_printed_lines = old.preserve_printed_lines;
                    s.opaque_background = old.opaque_background;
                    if old.theme.is_some() {
                        s.theme = old.theme;
                    }
                })?;
            }
            "connection.json" => {
                let connection = serde_json::from_str(&text).map_err(invalid)?;
                apply(doc, settings, |s| s.bookorbit = connection)?;
            }
            _ => {
                let keys = serde_json::from_str(&text).map_err(invalid)?;
                apply(doc, settings, |s| s.keys = keys)?;
            }
        }
        backup(name, &text)?;
    }
    Ok(())
}
pub(super) fn retire_backed_up_files(dir: &Path) -> io::Result<()> {
    for name in FILES {
        let path = dir.join(name);
        if path.is_file()
            && let Ok(copy) = read(&backup_dir()?.join(name))
            && read(&path)? == copy
        {
            fs::remove_file(path)?;
        }
    }
    // Ignore an in-use lock left by a concurrently running pre-TOML binary.
    let path = dir.join(".config.lock");
    if let Ok(lock) = fs::File::open(&path)
        && lock.metadata()?.len() == 0
        && lock.try_lock().is_ok()
    {
        fs::remove_file(path)?;
    }
    Ok(())
}
