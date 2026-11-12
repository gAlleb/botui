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
    FreeDictionary,
    GoogleWeb,
    GoogleCloud,
    #[serde(rename = "deepl", alias = "deep-l")]
    DeepL,
    LibreTranslate,
    #[serde(rename = "openai", alias = "open-ai")]
    OpenAi,
}
impl Provider {
    #[cfg(test)]
    pub const ALL: [Self; 8] = [
        Self::Http,
        Self::Command,
        Self::FreeDictionary,
        Self::GoogleWeb,
        Self::GoogleCloud,
        Self::DeepL,
        Self::LibreTranslate,
        Self::OpenAi,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP API",
            Self::Command => "Local command",
            Self::FreeDictionary => "Free Dictionary · English definitions",
            Self::GoogleWeb => "Google Translate · web (unofficial)",
            Self::GoogleCloud => "Google Cloud Translation",
            Self::DeepL => "DeepL",
            Self::LibreTranslate => "LibreTranslate",
            Self::OpenAi => "OpenAI-compatible API",
        }
    }
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Command | Self::Http => "",
            Self::FreeDictionary => "https://api.dictionaryapi.dev/api/v2/entries/en",
            Self::GoogleWeb => "https://translate.googleapis.com/translate_a/single",
            Self::GoogleCloud => "https://translation.googleapis.com/language/translate/v2",
            Self::DeepL => "https://api-free.deepl.com/v2/translate",
            Self::LibreTranslate => "http://localhost:5000/translate",
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
    fn sample_toml_and_profiles_round_trip() {
        let s: crate::settings::Settings =
            toml_edit::de::from_str(include_str!("../../../config.example.toml")).unwrap();
        assert!(s.lookup.profiles.is_empty());
        let mut c = s.lookup;
        c.profiles.push(Profile {
            name: "Local".into(),
            command: vec!["sdcv".into(), "{text}".into()],
            ..Default::default()
        });
        let encoded = toml_edit::ser::to_string(&c).unwrap();
        let copy: Config = toml_edit::de::from_str(&encoded).unwrap();
        assert_eq!(copy.profiles[0].command, ["sdcv", "{text}"]);
    }
}
