//! OPDS 2 collections and publication records, including older acquisition relations.
use super::{Entry, Feed, File, Result, Search, Url, validated};
use serde_json::Value as V;
fn seq(v: &V) -> impl Iterator<Item = &V> {
    v.as_array().into_iter().flatten()
}
fn localized(v: &V) -> Option<&str> {
    v.as_str()
        .or_else(|| v.get("en").and_then(V::as_str))
        .or_else(|| v.as_object()?.values().find_map(V::as_str))
}
fn rel(v: &V, r: &str) -> bool {
    v["rel"].as_str() == Some(r) || seq(&v["rel"]).any(|s| s.as_str() == Some(r))
}
fn url(base: &Url, v: &V) -> Option<Url> {
    validated(base.join(v["href"].as_str()?).ok()?).ok()
}
fn address(base: &Url, links: &V, r: &str) -> Option<Url> {
    seq(links).filter(|l| rel(l, r)).find_map(|l| url(base, l))
}
fn name(v: &V) -> Option<&str> {
    localized(&v["metadata"]["title"])
}
fn navigation(base: &Url, v: &V) -> Option<Entry> {
    let href = url(base, v)?;
    Some(Entry::Directory(
        localized(&v["title"]).unwrap_or(href.path()).to_owned(),
        href,
    ))
}
fn publication(base: &Url, p: &V) -> Option<Entry> {
    let title = name(p)?.to_owned();
    let contributors = &p["metadata"]["author"];
    let authors = contributors
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(std::slice::from_ref(contributors))
        .iter()
        .filter_map(|a| a.as_str().or_else(|| localized(&a["name"])))
        .map(str::to_owned)
        .collect();
    let mut files = Vec::new();
    for l in seq(&p["links"]) {
        if ![
            "download",
            "http://opds-spec.org/acquisition",
            "http://opds-spec.org/acquisition/open-access",
        ]
        .iter()
        .any(|r| rel(l, r))
            || l["properties"].get("indirectAcquisition").is_some()
        {
            continue;
        }
        if let Some(href) = url(base, l) {
            let mime = l["type"]
                .as_str()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("");
            if mime == "text/html" {
                continue;
            }
            files.push(File {
                format: super::format(mime, &href),
                href,
                size: l["length"].as_u64(),
            });
        }
    }
    let cover = seq(&p["images"])
        .filter(|v| {
            v["type"]
                .as_str()
                .is_none_or(|m| matches!(m, "image/jpeg" | "image/png" | "image/webp"))
        })
        .filter_map(|v| {
            Some((
                v["width"].as_u64().filter(|w| *w > 0).unwrap_or(u64::MAX),
                url(base, v)?,
            ))
        })
        .min_by_key(|(w, _)| *w)
        .map(|(_, u)| u);
    Some(Entry::Book {
        title,
        authors,
        files,
        cover,
    })
}
fn contents(base: &Url, v: &V) -> Vec<Entry> {
    let mut out: Vec<_> = seq(&v["navigation"])
        .filter_map(|l| navigation(base, l))
        .collect();
    out.extend(seq(&v["publications"]).filter_map(|p| publication(base, p)));
    out
}
pub(super) fn parse(base: &Url, data: &str) -> Result<Feed> {
    let v: V = serde_json::from_str(data)?;
    let title = name(&v)
        .ok_or("Expected an OPDS 2 catalog or publication with a title")?
        .to_owned();
    let mut items = contents(base, &v);
    let links = &v["links"];
    if ["navigation", "publications", "groups", "facets"]
        .iter()
        .all(|k| v.get(k).is_none())
    {
        if !seq(links).any(|l| l.get("rel").is_some()) {
            return Err("Expected an OPDS 2 feed or publication".into());
        }
        items.extend(publication(base, &v));
    }
    for g in seq(&v["groups"]) {
        let Some(title) = name(g) else { continue };
        let mut entries = contents(base, g);
        if let Some(href) = address(base, &g["links"], "self") {
            entries.push(Entry::Directory(format!("View all · {title}"), href));
        }
        items.push(Entry::Group(title.into(), entries));
    }
    for g in seq(&v["facets"]) {
        if let Some(name) = name(g) {
            items.push(Entry::Group(
                name.into(),
                seq(&g["links"])
                    .filter_map(|l| navigation(base, l))
                    .collect(),
            ));
        }
    }
    let search = seq(links).filter(|l| rel(l, "search")).find_map(|l| {
        let s = l["href"].as_str()?;
        super::search::json_url(base, s, "test")
            .ok()
            .map(|_| Search::Json(base.clone(), s.to_owned()))
    });
    Ok(Feed {
        title,
        search,
        items,
        next: address(base, links, "next"),
        previous: address(base, links, "previous"),
    })
}
