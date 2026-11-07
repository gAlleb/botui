//! OPDS 1.x can advertise either an Atom template or an OpenSearch description.
use super::{ATOM, Catalog, Result, Url, bounded, validated};
use roxmltree::Node;

#[derive(Clone)]
pub enum Search {
    Template(Url, String),
    Json(Url, String),
    Description(Url),
}

fn base_url(base: &Url, node: Node<'_, '_>) -> Option<Url> {
    let mut url = base.clone();
    let parts: Vec<_> = node
        .ancestors()
        .filter_map(|a| a.attribute(("http://www.w3.org/XML/1998/namespace", "base")))
        .collect();
    for part in parts.into_iter().rev() {
        url = url.join(part).ok()?;
    }
    Some(url)
}
pub(super) fn advertised(base: &Url, root: Node<'_, '_>) -> Option<Search> {
    let mut description = None;
    for node in root
        .children()
        .filter(|n| n.has_tag_name((ATOM, "link")) && n.attribute("rel") == Some("search"))
    {
        let Some(href) = node.attribute("href") else {
            continue;
        };
        let Some(base) = base_url(base, node) else {
            continue;
        };
        if href.contains("{searchTerms}") {
            return Some(Search::Template(base, href.into()));
        }
        if node
            .attribute("type")
            .is_some_and(|t| t.starts_with("application/opensearchdescription+xml"))
        {
            description = base
                .join(href)
                .ok()
                .and_then(|u| validated(u).ok())
                .map(Search::Description);
        }
    }
    description
}

/// Substitute query text as data even when it contains URI delimiters.
fn expand(base: &Url, template: &str, terms: &str) -> Result<Url> {
    let value = encode(terms)?;
    let mut s = template.replace("{searchTerms}", &value);
    while let Some((_, tail)) = s.split_once('{') {
        let key = tail.split_once('}').ok_or("Invalid OpenSearch template")?.0;
        if !key.ends_with('?') {
            return Err("Unsupported required OpenSearch parameter".into());
        }
        s = s.replacen(&format!("{{{key}}}"), "", 1);
    }
    validated(base.join(&s)?)
}
impl Catalog {
    pub fn search_url(&self, search: &Search, terms: &str) -> Result<Url> {
        match search {
            Search::Json(base, s) => json_url(base, s, terms),
            Search::Template(base, template) => expand(base, template, terms),
            Search::Description(u) => {
                let r = self.get(u.clone(), "application/opensearchdescription+xml", 15)?;
                let base = r.url().clone();
                let data = bounded(r, 256 << 10)?;
                let doc = roxmltree::Document::parse(std::str::from_utf8(&data)?)?;
                for n in doc.root_element().children().filter(|n| {
                    n.tag_name().name() == "Url"
                        && n.attribute("type")
                            .is_some_and(|s| s.starts_with("application/atom+xml"))
                        && n.attribute("method")
                            .is_none_or(|s| s.eq_ignore_ascii_case("GET"))
                }) {
                    if let Some(t) = n
                        .attribute("template")
                        .filter(|v| v.contains("{searchTerms}"))
                        && let Some(base) = base_url(&base, n)
                        && let Ok(url) = expand(&base, t, terms)
                    {
                        return Ok(url);
                    }
                }
                Err("No compatible Atom search template in the catalog description".into())
            }
        }
    }
}

fn encode(terms: &str) -> Result<String> {
    let mut value = String::new();
    for b in terms.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            value.push(b as char);
        } else {
            use std::fmt::Write;
            write!(value, "%{b:02X}")?;
        }
    }
    Ok(value)
}

/// OPDS 2 uses RFC 6570 query expansions. Send only the provided keyword,
/// dropping optional fields like title/author without leaking them into the path.
pub(super) fn json_url(base: &Url, template: &str, terms: &str) -> Result<Url> {
    let mut rest = template;
    let mut url = String::new();
    let mut found = false;
    let value = encode(terms)?;
    while let Some((prefix, tail)) = rest.split_once('{') {
        url.push_str(prefix);
        let (expr, suffix) = tail.split_once('}').ok_or("Invalid OPDS search template")?;
        if expr == "query" {
            url.push_str(&value);
            found = true;
        } else if let Some(vars) = expr.strip_prefix(['?', '&']) {
            if vars.split(',').any(|v| v == "query") {
                url.push(if expr.starts_with('?') { '?' } else { '&' });
                url.push_str("query=");
                url.push_str(&value);
                found = true;
            }
        } else {
            return Err("Unsupported OPDS search expression".into());
        }
        rest = suffix;
    }
    if !found {
        return Err("No query parameter in OPDS search template".into());
    }
    url.push_str(rest);
    validated(base.join(&url)?)
}
