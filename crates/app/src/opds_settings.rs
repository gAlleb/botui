//! Named OPDS connections live in the same TOML document as reader preferences.
use crate::{
    AppTerminal,
    connection::{Connection, PasswordCommand},
    prompt,
    settings::Settings,
};
use serde::{Deserialize, Serialize};
use std::{io, sync::atomic::AtomicBool};
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: Option<String>,
    pub password_command: Option<PasswordCommand>,
}
impl Catalog {
    pub fn password(&self, cancel: &AtomicBool) -> io::Result<String> {
        if self.password_command.is_none() {
            return Ok(self.password.clone().unwrap_or_default());
        }
        Connection {
            password: self.password.clone(),
            password_command: self.password_command.clone(),
            ..Connection::default()
        }
        .resolve(cancel)
    }
}
pub fn configure(term: &mut AppTerminal) -> io::Result<()> {
    loop {
        let mut catalogs = Settings::load()?.opds;
        let mut rows: Vec<_> = catalogs.iter().map(|c| c.name.clone()).collect();
        rows.push("Add catalog…".into());
        let Some(i) = prompt::choose(term, "OPDS catalogs", &rows, 0)? else {
            return Ok(());
        };
        let mut c = catalogs.get(i).cloned().unwrap_or_default();
        loop {
            let rows = vec![
                format!("Name: {}", c.name),
                format!("URL: {}", c.url),
                format!("Username: {}", c.username),
                format!(
                    "Password: {}",
                    if c.password.is_some() {
                        "••••••••"
                    } else {
                        "not set"
                    }
                ),
                format!(
                    "Password command: {}",
                    if c.password_command.is_some() {
                        "configured"
                    } else {
                        "not set"
                    }
                ),
                "Save catalog".into(),
                "Delete catalog…".into(),
            ];
            let Some(n) = prompt::choose(term, "OPDS connection", &rows, 0)? else {
                break;
            };
            match n {
                0..=2 => {
                    let field = match n {
                        0 => &mut c.name,
                        1 => &mut c.url,
                        _ => &mut c.username,
                    };
                    if let Some(s) = prompt::edit(
                        term,
                        ["Name", "OPDS URL", "Username"][n],
                        "[ Save ]",
                        field,
                        |_| {},
                    )? {
                        *field = s;
                    }
                }
                3 => {
                    if let Some(s) = prompt::field(
                        term,
                        "OPDS password",
                        "[ Save ]",
                        c.password.as_deref().unwrap_or(""),
                        true,
                        |_| {},
                    )? {
                        c.password = (!s.is_empty()).then_some(s);
                    }
                }
                4 => {
                    let value = c
                        .password_command
                        .as_ref()
                        .map(|v| serde_json::to_string(v).unwrap_or_default())
                        .unwrap_or_else(|| "[]".into());
                    if let Some(s) = prompt::edit(
                        term,
                        "Password command · JSON argument array",
                        "[ Save ]",
                        &value,
                        |_| {},
                    )? {
                        match serde_json::from_str::<Vec<String>>(&s) {
                            Ok(a) => {
                                c.password_command =
                                    (!a.is_empty()).then_some(PasswordCommand::Argv(a))
                            }
                            Err(_) => prompt::message(
                                term,
                                "OPDS",
                                "Enter an array, for example [\"pass\", \"show\", \"books\"]",
                            )?,
                        }
                    }
                }
                5 => {
                    let valid = reqwest::Url::parse(&c.url).is_ok_and(|u| {
                        matches!(u.scheme(), "http" | "https")
                            && u.username().is_empty()
                            && u.password().is_none()
                    });
                    if c.name.trim().is_empty() || !valid {
                        prompt::message(
                            term,
                            "OPDS",
                            "Enter a name and an HTTP(S) URL. Use the separate fields for credentials.",
                        )?;
                        continue;
                    }
                    if i == catalogs.len() {
                        catalogs.push(c);
                    } else {
                        catalogs[i] = c;
                    }
                    Settings::update(|s| s.opds = catalogs)?;
                    break;
                }
                6 if i < catalogs.len()
                    && prompt::choose(
                        term,
                        "Delete catalog?",
                        &["Cancel".into(), "Delete".into()],
                        0,
                    )? == Some(1) =>
                {
                    catalogs.remove(i);
                    Settings::update(|s| s.opds = catalogs)?;
                    break;
                }
                _ => (),
            }
        }
    }
}
