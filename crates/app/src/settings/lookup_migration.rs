//! Translate old provider names to explicit HTTP data once, before typed decoding.
use std::io;
use toml_edit::{DocumentMut, value};

pub(super) fn migrate(doc: &mut DocumentMut) -> io::Result<bool> {
    let recipes: DocumentMut = include_str!("../../../../config/lookup-migrations.toml")
        .parse()
        .map_err(|_| io::Error::other("Invalid bundled migration data"))?;
    let Some(profiles) = doc
        .get_mut("lookup")
        .and_then(|x| x.get_mut("profiles"))
        .and_then(|x| x.as_array_of_tables_mut())
    else {
        return Ok(false);
    };
    let mut changed = false;
    for profile in profiles.iter_mut() {
        let name = profile
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let name = if name == "deep-l" { "deepl" } else { name };
        let Some(recipe) = recipes.get(name).and_then(|v| v.as_table()) else {
            continue;
        };
        let endpoint = profile
            .get("endpoint")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| recipe["endpoint"].as_str().unwrap())
            .to_owned();
        for (key, item) in recipe.iter() {
            if key != "endpoint_suffix" {
                profile.insert(key, item.clone());
            }
        }
        let suffix = recipe
            .get("endpoint_suffix")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        profile.insert(
            "endpoint",
            value(format!("{}{}", endpoint.trim_end_matches('/'), suffix)),
        );
        profile.insert("provider", value("http"));
        changed = true;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_credentials_names_custom_hosts_and_unrelated_comments() {
        let mut doc: DocumentMut = r#"# personal configuration
[lookup]
dictionary = "Mine"
[[lookup.profiles]]
name = "Mine"
provider = "free-dictionary"
endpoint = "http://localhost/custom/"
api_key = "fixture"
key_command = ["pass", "show", "my-key"]
"#
        .parse()
        .unwrap();
        assert!(migrate(&mut doc).unwrap());
        assert!(!migrate(&mut doc).unwrap());
        let s: crate::settings::Settings = toml_edit::de::from_str(&doc.to_string()).unwrap();
        let p = &s.lookup.profiles[0];
        assert_eq!(p.endpoint, "http://localhost/custom/{text}");
        assert_eq!(p.name, "Mine");
        assert_eq!(p.api_key, "fixture");
        assert_eq!(p.key_command, ["pass", "show", "my-key"]);
        assert!(doc.to_string().starts_with("# personal configuration"));
    }
}
