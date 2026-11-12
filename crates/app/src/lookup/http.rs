//! User-defined HTTP requests. Templates substitute data once, never code.
use super::Profile;
use serde_json::Value;
use std::io;
pub fn expand(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some((token, value)) = values.iter().find(|(token, _)| rest.starts_with(token)) {
            out.push_str(value);
            rest = &rest[token.len()..];
        } else {
            out.push('{');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}
fn encoded(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
fn json_values(value: &mut Value, values: &[(&str, &str)]) {
    match value {
        Value::String(s) => *s = expand(s, values),
        Value::Array(a) => a.iter_mut().for_each(|v| json_values(v, values)),
        Value::Object(o) => {
            o.retain(|_, v| {
                !v.as_str().is_some_and(|s| {
                    matches!(s, "{source?}" | "{source:upper?}")
                        && values
                            .iter()
                            .any(|(k, v)| *k == "{source}" && matches!(*v, "auto" | ""))
                })
            });
            o.values_mut().for_each(|v| json_values(v, values));
        }
        _ => (),
    }
}
pub fn validate(p: &Profile) -> Result<(), &'static str> {
    if !matches!(
        p.method.as_str(),
        "" | "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
    ) {
        return Err("Choose a supported HTTP method.");
    }
    if !p.body.is_empty() && serde_json::from_str::<Value>(&p.body).is_err() {
        return Err("Request body must be valid JSON. Put placeholders inside JSON strings.");
    }
    if !p.response_path.is_empty() && !p.response_path.starts_with('/') {
        return Err("Text path must start with /, or be empty for the JSON root.");
    }
    if !matches!(p.response_format.as_str(), "" | "json" | "text") {
        return Err("Response format must be json or text.");
    }
    for (name, value) in &p.headers {
        if reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err()
            || reqwest::header::HeaderValue::from_str(value).is_err()
        {
            return Err("Invalid HTTP header name or value.");
        }
    }
    Ok(())
}
pub fn request(
    client: &reqwest::blocking::Client,
    p: &Profile,
    text: &str,
    source: &str,
    target: &str,
    key: &str,
) -> io::Result<reqwest::blocking::RequestBuilder> {
    validate(p).map_err(io::Error::other)?;
    let source_upper = source.to_uppercase();
    let target_upper = target.to_uppercase();
    let values = [
        ("{text}", text),
        ("{source}", source),
        ("{target}", target),
        ("{key}", key),
        ("{source?}", source),
        ("{source:upper}", source_upper.as_str()),
        ("{source:upper?}", source_upper.as_str()),
        ("{target:upper}", target_upper.as_str()),
    ];
    let encoded_values: Vec<_> = values.iter().map(|(k, v)| (*k, encoded(v))).collect();
    let refs: Vec<_> = encoded_values
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    let mut url = reqwest::Url::parse(&expand(&p.endpoint, &refs))
        .map_err(|_| io::Error::other("Invalid API endpoint"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(io::Error::other(
            "Use an HTTP(S) endpoint without embedded credentials",
        ));
    }
    let method = if p.method.is_empty() {
        "GET"
    } else {
        &p.method
    };
    if !p.query.is_empty() {
        let mut query = url.query_pairs_mut();
        for (name, value) in &p.query {
            query.append_pair(name, &expand(value, &values));
        }
    }
    let mut req = client.request(
        reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| io::Error::other("Invalid HTTP method"))?,
        url,
    );
    if !p.body.is_empty() {
        let mut body: Value =
            serde_json::from_str(&p.body).map_err(|_| io::Error::other("Invalid JSON body"))?;
        json_values(&mut body, &values);
        req = req
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    for (name, value) in &p.headers {
        req = req.header(name, expand(value, &values));
    }
    Ok(req)
}
pub fn decode(p: &Profile, bytes: &[u8]) -> io::Result<String> {
    if p.response_format == "text" {
        return String::from_utf8(bytes.to_vec())
            .map_err(|_| io::Error::other("Response is not UTF-8 text"));
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| io::Error::other("Service returned invalid JSON"))?;
    let parts: Vec<_> = p
        .response_path
        .strip_prefix('/')
        .map(|s| s.split('/').collect())
        .unwrap_or_default();
    fn collect(v: &Value, parts: &[&str], out: &mut Vec<String>) {
        if let Some((part, tail)) = parts.split_first() {
            if *part == "*" {
                if let Value::Array(a) = v {
                    for item in a {
                        collect(item, tail, out);
                    }
                }
            } else {
                let token = part.replace("~1", "/").replace("~0", "~");
                let next = match v {
                    Value::Array(a) => token.parse::<usize>().ok().and_then(|i| a.get(i)),
                    Value::Object(o) => o.get(&token),
                    _ => None,
                };
                if let Some(next) = next {
                    collect(next, tail, out);
                }
            }
        } else if let Value::String(s) = v {
            out.push(s.clone());
        }
    }
    let mut output = vec![];
    collect(&value, &parts, &mut output);
    let result = output.join(&p.response_separator);
    if result.trim().is_empty() {
        Err(io::Error::other("No text at the configured response path"))
    } else {
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_source_and_uppercase_codes_are_data_templates() {
        let p = Profile {
            endpoint: "http://localhost/translate".into(),
            method: "POST".into(),
            body: r#"{"source":"{source:upper?}","target":"{target:upper}","text":"{text}"}"#
                .into(),
            ..Default::default()
        };
        let client = reqwest::blocking::Client::new();
        for source in ["auto", "en"] {
            let r = request(&client, &p, "literal {source}", source, "ru", "")
                .unwrap()
                .build()
                .unwrap();
            let body: Value =
                serde_json::from_slice(r.body().unwrap().as_bytes().unwrap()).unwrap();
            assert_eq!(body["target"], "RU");
            assert_eq!(body["text"], "literal {source}");
            if source == "auto" {
                assert!(body.get("source").is_none());
            } else {
                assert_eq!(body["source"], "EN");
            }
        }
    }
    #[test]
    fn templates_escape_data_without_recursive_substitution() {
        let client = reqwest::blocking::Client::new();
        let p = Profile {
            endpoint: "http://localhost/word/{text}".into(),
            method: "POST".into(),
            query: [("q".into(), "{text}".into())].into(),
            headers: [("Authorization".into(), "Bearer {key}".into())].into(),
            body: r#"{"query":"{text}","language":"{target}"}"#.into(),
            ..Default::default()
        };
        let text = "a/字\" {key}";
        let req = request(&client, &p, text, "auto", "ru", "secret")
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(req.url().path(), "/word/a%2F%E5%AD%97%22%20%7Bkey%7D");
        let body: Value = serde_json::from_slice(req.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(req.url().query_pairs().next().unwrap().1, text);
        assert_eq!(req.headers()["Authorization"], "Bearer secret");
        assert_eq!(body["query"], text);
        assert_eq!(body["language"], "ru");
    }
    #[test]
    fn custom_response_paths_and_plain_text() {
        let mut p = Profile {
            response_path: "/items/*/a~1b".into(),
            response_separator: " | ".into(),
            ..Default::default()
        };
        assert_eq!(
            decode(&p, br#"{"items":[{"a/b":"one"},{"a/b":"two"}]}"#).unwrap(),
            "one | two"
        );
        assert!(decode(&p, b"{}").is_err());
        p.response_format = "text".into();
        assert_eq!(decode(&p, b"definition").unwrap(), "definition");
    }
}
