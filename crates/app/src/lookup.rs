//! Explicit dictionary/translation requests; no network traffic while reading.
mod http;
mod providers;
pub(crate) mod settings;
mod settings_ui;
mod window;
use serde::{Deserialize, Serialize};
pub use window::show;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Purpose {
    #[default]
    Dictionary,
    Translation,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    #[default]
    Command,
    Http,
    #[serde(rename = "openai", alias = "open-ai")]
    OpenAi,
}
impl Provider {
    #[cfg(test)]
    pub const ALL: [Self; 3] = [Self::Http, Self::Command, Self::OpenAi];
    pub fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP API",
            Self::Command => "Local command",
            Self::OpenAi => "OpenAI-compatible API",
        }
    }
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Command | Self::Http => "",
            Self::OpenAi => "http://localhost:11434/v1/chat/completions",
        }
    }
}
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub purpose: Purpose,
    pub provider: Provider,
    pub endpoint: String,
    pub api_key: String,
    pub key_command: Vec<String>,
    pub command: Vec<String>,
    pub model: String,
    pub description: String,
    pub target_language: String,
    pub method: String,
    pub query: std::collections::BTreeMap<String, String>,
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: String,
    pub response_format: String,
    pub response_path: String,
    pub response_separator: String,
}
impl Profile {
    pub fn uses_language(&self, source: bool) -> bool {
        if self.provider == Provider::OpenAi {
            return true;
        }
        let tokens: &[&str] = if source {
            &["{source}", "{source?}", "{source:upper}", "{source:upper?}"]
        } else {
            &["{target}", "{target:upper}"]
        };
        let contains = |s: &str| tokens.iter().any(|t| s.contains(t));
        if self.provider == Provider::Command {
            self.command.iter().any(|s| contains(s))
        } else {
            contains(&self.endpoint)
                || contains(&self.body)
                || self
                    .query
                    .values()
                    .chain(self.headers.values())
                    .any(|s| contains(s))
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub profiles: Vec<Profile>,
    pub dictionary: String,
    pub translation: String,
    pub source_language: String,
    pub target_language: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            profiles: vec![],
            dictionary: String::new(),
            translation: String::new(),
            source_language: "auto".into(),
            target_language: "en".into(),
        }
    }
}
impl Config {
    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        let mut config = s.lookup.clone();
        if !s.lookup_command.is_empty()
            && !config.profiles.iter().any(|p| p.name == "Legacy command")
        {
            config.profiles.insert(
                0,
                Profile {
                    name: "Legacy command".into(),
                    command: s.lookup_command.clone(),
                    ..Profile::default()
                },
            );
            if config.dictionary.is_empty() || config.dictionary == "English dictionary" {
                config.dictionary = "Legacy command".into();
            }
        }
        config
    }
    pub fn profiles(&self, purpose: Purpose) -> Vec<Profile> {
        self.profiles
            .iter()
            .filter(|p| p.purpose == purpose)
            .cloned()
            .collect()
    }
    pub fn selected(&self, purpose: Purpose) -> &str {
        if purpose == Purpose::Dictionary {
            &self.dictionary
        } else {
            &self.translation
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_profiles_parse_and_merriam_webster_decodes_definitions() {
        let example = include_str!("../../../config.example.toml");
        let mut profiles = Vec::new();
        for block in example.split("# [[lookup.profiles]]").skip(1) {
            let fields = block
                .lines()
                .map_while(|line| {
                    if line.is_empty() {
                        return Some("");
                    }
                    let line = line.strip_prefix("# ")?;
                    let key = line.split_once(" = ")?.0;
                    key.chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_')
                        .then_some(line)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let profile: Profile = toml_edit::de::from_str(&fields).unwrap();
            if profile.provider == Provider::Http {
                http::validate(&profile).unwrap();
            }
            profiles.push(profile);
        }
        assert_eq!(profiles.len(), 5);
        let mw = profiles
            .iter()
            .find(|p| p.name == "Merriam-Webster Collegiate")
            .unwrap();
        let request = http::request(
            &reqwest::blocking::Client::new(),
            mw,
            "test word",
            "auto",
            "ru",
            "fixture-key",
        )
        .unwrap()
        .build()
        .unwrap();
        assert!(request.url().path().ends_with("/test%20word"));
        assert_eq!(
            request.url().query_pairs().collect::<Vec<_>>(),
            [("key".into(), "fixture-key".into())]
        );
        assert_eq!(
            http::decode(
                mw,
                br#"[{"shortdef":["first sense","second sense"]},{"shortdef":["another entry"]}]"#
            )
            .unwrap(),
            "first sense\nsecond sense\nanother entry"
        );
        assert!(http::decode(mw, br#"["spelling suggestion"]"#).is_err());
    }
    #[test]
    fn sample_toml_and_profiles_round_trip() {
        let s: crate::settings::Settings =
            toml_edit::de::from_str(include_str!("../../../config.example.toml")).unwrap();
        assert_eq!(s.lookup.profiles.len(), 2);
        let mut c = s.lookup;
        c.profiles.push(Profile {
            name: "Local".into(),
            command: vec!["sdcv".into(), "{text}".into()],
            ..Default::default()
        });
        let encoded = toml_edit::ser::to_string(&c).unwrap();
        let copy: Config = toml_edit::de::from_str(&encoded).unwrap();
        assert_eq!(copy.profiles.last().unwrap().command, ["sdcv", "{text}"]);
    }
}
