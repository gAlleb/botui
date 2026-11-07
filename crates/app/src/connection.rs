use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct Connection {
    #[serde(default)]
    pub(crate) server: String,
    #[serde(default)]
    pub(crate) username: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) list_view: bool,
}

pub(crate) fn settings_file() -> io::Result<PathBuf> {
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".config"))
        })
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Не найден каталог настроек botui")
        })?;
    Ok(config.join("botui/connection.json"))
}

impl Connection {
    pub(crate) fn load() -> io::Result<Self> {
        let file = match fs::File::open(settings_file()?) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            other => other?,
        };
        const LIMIT: usize = 16 * 1024;
        let mut bytes = Vec::new();
        file.take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Слишком большой файл настроек подключения",
            ));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    pub(crate) fn with_server(mut self, server: Option<String>) -> Self {
        if let Some(server) = server {
            if self.server.trim_end_matches('/') != server.trim_end_matches('/') {
                self.username.clear();
            }
            self.server = server;
        }
        self
    }
}

// Persist only the address and username. Credentials belong to the live session.
pub(crate) fn remember(server: &str, username: &str, list_view: bool) -> io::Result<()> {
    let path = settings_file()?;
    save(
        &path,
        &Connection {
            server: server.into(),
            username: username.into(),
            list_view,
        },
    )
}

fn save(path: &Path, data: &Connection) -> io::Result<()> {
    let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(dir)?;
    let mut partial = NamedTempFile::new_in(dir)?;
    serde_json::to_writer(partial.as_file_mut(), data)?;
    partial.persist(path)?;
    Ok(())
}
