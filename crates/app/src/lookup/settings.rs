use super::*;
use crate::{AppTerminal, prompt, settings::Settings};
use std::io;
fn choose(
    term: &mut AppTerminal,
    title: &str,
    items: &[String],
    selected: usize,
) -> io::Result<Option<usize>> {
    prompt::choose(term, title, items, selected)
}
fn field(term: &mut AppTerminal, title: &str, current: &str) -> io::Result<Option<String>> {
    prompt::edit(term, title, "[ Apply ]", current, |_| {})
}
pub fn language(term: &mut AppTerminal, current: &str, source: bool) -> io::Result<Option<String>> {
    let languages = [
        ("en", "English"),
        ("ru", "Russian"),
        ("de", "German"),
        ("fr", "French"),
        ("es", "Spanish"),
        ("it", "Italian"),
        ("pt", "Portuguese"),
        ("uk", "Ukrainian"),
        ("pl", "Polish"),
        ("ja", "Japanese"),
        ("zh", "Chinese"),
        ("ko", "Korean"),
        ("hi", "Hindi"),
        ("ar", "Arabic"),
    ];
    let mut items: Vec<_> = languages
        .iter()
        .map(|(code, name)| (code.to_string(), format!("{name} · {code}")))
        .collect();
    if source {
        items.insert(0, ("auto".into(), "Detect automatically".into()));
    }
    let selected = items.iter().position(|(c, _)| c == current).unwrap_or(0);
    let mut labels: Vec<_> = items.iter().map(|(_, s)| s.clone()).collect();
    labels.push("Other language code…".into());
    match choose(
        term,
        if source {
            "Source language"
        } else {
            "Translation / explanation language"
        },
        &labels,
        selected,
    )? {
        Some(i) if i < items.len() => Ok(Some(items[i].0.clone())),
        Some(_) => {
            let value = field(term, "Language code (e.g. ru, en, pt-BR)", current)?;
            if value.as_ref().is_some_and(|s| {
                s.is_empty() || !s.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
            }) {
                return Err(io::Error::other("Enter a language code"));
            }
            Ok(value)
        }
        None => Ok(None),
    }
}

/// Separate definitions from translation in both navigation and configuration.
pub fn rows(c: &Config) -> Vec<(String, &'static str)> {
    vec![
        (
            format!(
                "Word definitions · {} connections",
                c.profiles(Purpose::Dictionary).len()
            ),
            "Dictionaries for word meanings and examples. Add, configure, test or remove a dictionary.",
        ),
        (
            format!(
                "Translation · {} connections",
                c.profiles(Purpose::Translation).len()
            ),
            "Services for translating selected text. Add, configure, test or remove a translator and choose languages.",
        ),
    ]
}
fn providers(purpose: Purpose) -> Vec<Provider> {
    let all = [
        Provider::FreeDictionary,
        Provider::GoogleWeb,
        Provider::DeepL,
        Provider::GoogleCloud,
        Provider::LibreTranslate,
        Provider::OpenAi,
        Provider::Command,
    ];
    all.into_iter()
        .filter(|p| {
            supports_both(*p)
                || match purpose {
                    Purpose::Dictionary => *p == Provider::FreeDictionary,
                    Purpose::Translation => *p != Provider::FreeDictionary,
                }
        })
        .collect()
}
pub fn open(term: &mut AppTerminal, section: usize) -> io::Result<()> {
    let purpose = if section == 0 {
        Purpose::Dictionary
    } else {
        Purpose::Translation
    };
    let mut selected = 0;
    loop {
        let c = Config::from_settings(&Settings::load()?);
        let indices: Vec<_> = c
            .profiles
            .iter()
            .enumerate()
            .filter(|(_, p)| p.purpose == purpose)
            .map(|(i, _)| i)
            .collect();
        let mut rows: Vec<_> = indices
            .iter()
            .map(|&i| {
                let p = &c.profiles[i];
                format!(
                    "{}{} · {}",
                    p.name,
                    if c.selected(purpose) == p.name {
                        " [default]"
                    } else {
                        ""
                    },
                    status(p)
                )
            })
            .collect();
        rows.push(
            if purpose == Purpose::Dictionary {
                "+ Add dictionary…"
            } else {
                "+ Add translator…"
            }
            .into(),
        );
        if purpose == Purpose::Translation {
            rows.push(format!("Source language: {}", c.source_language));
        }
        rows.push(format!(
            "{}: {}",
            if purpose == Purpose::Dictionary {
                "AI explanation language"
            } else {
                "Translate into"
            },
            c.target_language
        ));
        let title = if purpose == Purpose::Dictionary {
            "Word definitions"
        } else {
            "Translation"
        };
        let hint = if purpose == Purpose::Dictionary {
            "Dictionaries explain words and give examples. Select a dictionary to configure it, set it as default, test it or delete it. Free Dictionary provides English definitions; the explanation language applies to AI services."
        } else {
            "Translators translate a selected passage. Select a service to configure it, set it as default, test it or delete it. Choose input and output languages below the list."
        };
        let Some(i) = super::settings_ui::choose(term, title, hint, &rows, selected)? else {
            return Ok(());
        };
        selected = i;
        let index = if i < indices.len() {
            indices[i]
        } else if i == indices.len() {
            c.profiles.len()
        } else if purpose == Purpose::Translation && i == indices.len() + 1 {
            c.profiles.len() + 1
        } else {
            c.profiles.len() + 2
        };
        edit_connection(term, index, purpose)?;
    }
}
fn purpose_label(p: Purpose) -> &'static str {
    match p {
        Purpose::Dictionary => "Dictionary",
        Purpose::Translation => "Translation",
    }
}
fn supports_both(p: Provider) -> bool {
    matches!(p, Provider::Command | Provider::OpenAi)
}
fn has_key(p: Provider) -> bool {
    matches!(
        p,
        Provider::GoogleCloud | Provider::DeepL | Provider::LibreTranslate | Provider::OpenAi
    )
}
fn status(p: &Profile) -> &'static str {
    match p.provider {
        Provider::FreeDictionary | Provider::GoogleWeb => "No API key needed",
        Provider::Command if p.command.first().is_none_or(|s| s.trim().is_empty()) => {
            "Choose a program"
        }
        Provider::Command => "Program configured",
        Provider::OpenAi if p.model.trim().is_empty() => "Choose a model",
        Provider::GoogleCloud | Provider::DeepL
            if p.api_key.is_empty() && p.key_command.is_empty() =>
        {
            "API key required"
        }
        _ => "Configured · test connection",
    }
}
fn description(p: &Profile) -> &'static str {
    match p.provider {
        Provider::FreeDictionary => {
            "English word definitions, pronunciation and examples. Ready to try; no account or API key needed. Does not translate text."
        }
        Provider::GoogleWeb => {
            "Translate text between languages without an API key. Uses Google's unofficial web interface; availability can change."
        }
        Provider::GoogleCloud => {
            "Google Cloud Translation API. Requires your API key and an enabled translation service in your Google Cloud account."
        }
        Provider::DeepL => {
            "DeepL translation API. Enter your API key. The default address is for API Free; API Pro uses api.deepl.com."
        }
        Provider::LibreTranslate => {
            "Your LibreTranslate server. Set its address; an API key is optional and depends on the server."
        }
        Provider::OpenAi => {
            "Definitions or translation through an OpenAI-compatible service. Set the server address and model. Local servers may not need an API key."
        }
        Provider::Command => {
            "Run a local dictionary or translation program. Add {text} as an argument for the selected passage; {source} and {target} insert language codes. No shell is used."
        }
    }
}
fn edit_connection(term: &mut AppTerminal, index: usize, purpose: Purpose) -> io::Result<()> {
    let mut c = Config::from_settings(&Settings::load()?);
    let count = c.profiles.len();
    if index == count + 1 || index == count + 2 {
        let source = index == count + 1;
        if let Some(value) = language(
            term,
            if source {
                &c.source_language
            } else {
                &c.target_language
            },
            source,
        )? {
            if source {
                c.source_language = value;
            } else {
                c.target_language = value;
            }
        } else {
            return Ok(());
        }
    } else {
        let existing = index < count;
        let p = if existing {
            c.profiles[index].clone()
        } else {
            let available = providers(purpose);
            let Some(n) = choose(
                term,
                "Add connection · choose service",
                &available
                    .iter()
                    .map(|p| p.label().into())
                    .collect::<Vec<_>>(),
                0,
            )?
            else {
                return Ok(());
            };
            let provider = available[n];
            let base = provider.label();
            let mut name = base.to_owned();
            let mut suffix = 2;
            while c.profiles.iter().any(|p| p.name == name) {
                name = format!("{base} {suffix}");
                suffix += 1;
            }
            Profile {
                name,
                provider,
                purpose,
                ..Profile::default()
            }
        };
        let is_default = c.selected(p.purpose) == p.name;
        let old_name = p.name.clone();
        let Some(result) = edit_profile(term, p, is_default, existing, &c.profiles)? else {
            return Ok(());
        };
        match result {
            EditResult::Save(p, default) => {
                if existing {
                    c.profiles[index] = p.clone();
                } else {
                    c.profiles.push(p.clone());
                }
                for purpose in [Purpose::Dictionary, Purpose::Translation] {
                    let selected = if purpose == Purpose::Dictionary {
                        &mut c.dictionary
                    } else {
                        &mut c.translation
                    };
                    if *selected == old_name {
                        selected.clear();
                    }
                    if (default || is_default) && p.purpose == purpose {
                        *selected = p.name.clone();
                    }
                }
            }
            EditResult::Delete => {
                c.profiles.remove(index);
            }
        }
        for purpose in [Purpose::Dictionary, Purpose::Translation] {
            if !c
                .profiles
                .iter()
                .any(|p| p.purpose == purpose && p.name == c.selected(purpose))
            {
                let name = c
                    .profiles
                    .iter()
                    .find(|p| p.purpose == purpose)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                if purpose == Purpose::Dictionary {
                    c.dictionary = name;
                } else {
                    c.translation = name;
                }
            }
        }
    }
    Settings::update(|s| {
        s.lookup = c;
        s.lookup_command.clear();
    })?;
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Name,
    Purpose,
    Endpoint,
    Key,
    KeyCommand,
    Program,
    Model,
    Default,
    Test,
    Save,
    Delete,
}
struct Row {
    label: String,
    action: Action,
}
fn editor_rows(p: &Profile, default: bool, existing: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut add = |action, label| rows.push(Row { action, label });
    add(Action::Name, format!("Name: {}", p.name));
    if supports_both(p.provider) {
        add(
            Action::Purpose,
            format!("Use for: {}", purpose_label(p.purpose)),
        );
    }
    if p.provider == Provider::Command {
        add(
            Action::Program,
            format!(
                "Program: {}",
                p.command
                    .first()
                    .map(String::as_str)
                    .unwrap_or("Choose program…")
            ),
        );
    } else {
        add(
            Action::Endpoint,
            format!(
                "Server: {}",
                if p.endpoint.is_empty() {
                    p.provider.endpoint()
                } else {
                    &p.endpoint
                }
            ),
        );
    }
    if has_key(p.provider) {
        add(
            Action::Key,
            format!(
                "API key: {}",
                if !p.key_command.is_empty() {
                    "Provided by command"
                } else if !p.api_key.is_empty() {
                    "Saved (hidden)"
                } else if matches!(p.provider, Provider::OpenAi | Provider::LibreTranslate) {
                    "Optional · none"
                } else {
                    "Required · enter key…"
                }
            ),
        );
        add(
            Action::KeyCommand,
            format!(
                "Get key from password manager: {}",
                if p.key_command.is_empty() {
                    "Off"
                } else {
                    "On"
                }
            ),
        );
    }
    if p.provider == Provider::OpenAi {
        add(
            Action::Model,
            format!(
                "Model: {}",
                if p.model.is_empty() {
                    "Enter model name…"
                } else {
                    &p.model
                }
            ),
        );
    }
    add(
        Action::Default,
        format!(
            "Use by default for {}: {}",
            purpose_label(p.purpose).to_lowercase(),
            if default {
                "Yes"
            } else {
                "No · select to enable"
            }
        ),
    );
    add(Action::Test, "Test connection with ‘hello’…".into());
    add(Action::Save, "Save changes & back".into());
    if existing {
        add(Action::Delete, "Delete connection…".into());
    }
    rows
}
enum EditResult {
    Save(Profile, bool),
    Delete,
}
fn validate(p: &Profile) -> Result<(), &'static str> {
    if p.name.trim().is_empty() {
        return Err("Enter a name for this connection.");
    }
    if p.provider == Provider::Command {
        if p.command.first().is_none_or(|s| s.trim().is_empty()) {
            return Err("Choose the local program before saving.");
        }
    } else {
        let url = reqwest::Url::parse(if p.endpoint.is_empty() {
            p.provider.endpoint()
        } else {
            &p.endpoint
        })
        .map_err(|_| "Enter a complete server address, starting with http:// or https://.")?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Use an HTTP(S) server address without a username or password in it.");
        }
    }
    if p.provider == Provider::OpenAi && p.model.trim().is_empty() {
        return Err("Enter the model name used by your server.");
    }
    Ok(())
}
fn edit_profile(
    term: &mut AppTerminal,
    mut p: Profile,
    mut default: bool,
    existing: bool,
    profiles: &[Profile],
) -> io::Result<Option<EditResult>> {
    let original = p.clone();
    let original_default = default;
    let mut selected = 0;
    loop {
        let rows = editor_rows(&p, default, existing);
        let Some(i) = super::settings_ui::choose(
            term,
            &p.name,
            description(&p),
            &rows.iter().map(|r| r.label.clone()).collect::<Vec<_>>(),
            selected,
        )?
        else {
            if p == original && default == original_default {
                return Ok(None);
            }
            match choose(
                term,
                "Unsaved changes",
                &[
                    "Keep editing".into(),
                    "Save changes".into(),
                    "Discard changes".into(),
                ],
                0,
            )? {
                Some(1) => (),
                Some(2) => return Ok(None),
                _ => continue,
            }
            // Reuse validation and saving below.
            if let Err(error) = validate(&p) {
                prompt::message(term, "Connection", error)?;
                continue;
            }
            if profiles
                .iter()
                .any(|other| other.name == p.name && (!existing || other.name != original.name))
            {
                prompt::message(
                    term,
                    "Connection",
                    "That name is already used. Choose a unique name.",
                )?;
                continue;
            }
            return Ok(Some(EditResult::Save(p, default)));
        };
        selected = i;
        match rows[i].action {
            Action::Name => {
                if let Some(value) = field(term, "Connection name", &p.name)? {
                    p.name = value;
                }
            }
            Action::Purpose => {
                if let Some(i) = choose(
                    term,
                    "Use this connection for",
                    &["Word definitions".into(), "Translation".into()],
                    usize::from(p.purpose == Purpose::Translation),
                )? {
                    p.purpose = if i == 0 {
                        Purpose::Dictionary
                    } else {
                        Purpose::Translation
                    };
                }
            }
            Action::Endpoint => {
                if let Some(value) = field(
                    term,
                    "Server address · empty restores default",
                    if p.endpoint.is_empty() {
                        p.provider.endpoint()
                    } else {
                        &p.endpoint
                    },
                )? {
                    p.endpoint = value;
                }
            }
            Action::Key => {
                if let Some(value) = prompt::field(
                    term,
                    "API key · hidden",
                    "[ Apply ]",
                    &p.api_key,
                    true,
                    |_| {},
                )? {
                    if !p.key_command.is_empty()
                        && choose(
                            term,
                            "Use this key instead of the password manager?",
                            &["Cancel".into(), "Use entered key".into()],
                            0,
                        )? != Some(1)
                    {
                        continue;
                    }
                    p.api_key = value;
                    p.key_command.clear();
                }
            }
            Action::KeyCommand => {
                if let Some(args) = edit_command(term, &p.key_command, true)? {
                    p.key_command = args;
                    if !p.key_command.is_empty() {
                        p.api_key.clear();
                    }
                }
            }
            Action::Program => {
                if let Some(args) = edit_command(term, &p.command, false)? {
                    p.command = args;
                }
            }
            Action::Model => {
                if let Some(value) = field(term, "Model name", &p.model)? {
                    p.model = value;
                }
            }
            Action::Default => default = true,
            Action::Test => super::window::test(term, p.clone())?,
            Action::Save => {
                if let Err(error) = validate(&p) {
                    prompt::message(term, "Connection", error)?;
                    continue;
                }
                if profiles
                    .iter()
                    .any(|other| other.name == p.name && (!existing || other.name != original.name))
                {
                    prompt::message(
                        term,
                        "Connection",
                        "That name is already used. Choose a unique name.",
                    )?;
                    continue;
                }
                return Ok(Some(EditResult::Save(p, default)));
            }
            Action::Delete => {
                if choose(
                    term,
                    "Delete this connection?",
                    &["Cancel".into(), "Delete connection".into()],
                    0,
                )? == Some(1)
                {
                    return Ok(Some(EditResult::Delete));
                }
            }
        }
    }
}
fn edit_command(
    term: &mut AppTerminal,
    original: &[String],
    key: bool,
) -> io::Result<Option<Vec<String>>> {
    let mut args = original.to_vec();
    if args.is_empty() {
        args.push(String::new());
    }
    loop {
        let mut rows = vec![format!(
            "Program: {}",
            if args[0].is_empty() {
                "Enter program name…"
            } else {
                &args[0]
            }
        )];
        rows.extend(
            args.iter()
                .enumerate()
                .skip(1)
                .map(|(i, a)| format!("Argument {i}: {a}")),
        );
        rows.extend(["+ Add argument…".into(), "Apply command".into()]);
        if key {
            rows.push("Turn off password manager".into());
        }
        let Some(i) = super::settings_ui::choose(
            term,
            if key {
                "Password manager command"
            } else {
                "Local program"
            },
            if key {
                "Example: program pass, arguments show and books/api-key. Standard output supplies the API key. Each argument is separate; no shell quoting is needed."
            } else {
                "Example: program sdcv, arguments -n and {text}. {text} inserts the selection, {source}/{target} insert languages. Each argument is separate; no shell quoting is needed."
            },
            &rows,
            0,
        )?
        else {
            return Ok(None);
        };
        if i < args.len() {
            if let Some(value) = field(
                term,
                if i == 0 {
                    "Program name or full path"
                } else {
                    "Argument · empty removes it"
                },
                &args[i],
            )? {
                if i > 0 && value.is_empty() {
                    args.remove(i);
                } else {
                    args[i] = value;
                }
            }
        } else if i == args.len() {
            if let Some(value) = field(term, "New argument", "")?
                && !value.is_empty()
            {
                args.push(value);
            }
        } else if i == args.len() + 1 {
            if args[0].trim().is_empty() {
                prompt::message(term, "Program", "Enter a program name first.")?;
            } else {
                return Ok(Some(args));
            }
        } else {
            return Ok(Some(Vec::new()));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtins_show_effective_addresses_without_irrelevant_credentials() {
        for p in Config::default().profiles {
            let rows = editor_rows(&p, true, true);
            assert!(rows.iter().any(|r| r.label.contains(p.provider.endpoint())));
            assert!(!rows.iter().any(|r| matches!(
                r.action,
                Action::Key | Action::KeyCommand | Action::Model | Action::Program
            )));
            assert_eq!(status(&p), "No API key needed");
            assert!(validate(&p).is_ok());
        }
    }
    #[test]
    fn editor_fields_follow_service_and_keep_secrets_hidden() {
        for provider in Provider::ALL {
            let p = Profile {
                provider,
                api_key: "secret-value".into(),
                ..Profile::default()
            };
            let rows = editor_rows(&p, false, false);
            assert_eq!(
                rows.iter().any(|r| r.action == Action::Model),
                provider == Provider::OpenAi
            );
            assert_eq!(
                rows.iter().any(|r| r.action == Action::Program),
                provider == Provider::Command
            );
            assert_eq!(
                rows.iter().any(|r| r.action == Action::Key),
                has_key(provider)
            );
            assert!(!rows.iter().any(|r| r.label.contains("secret-value")));
        }
    }
}
