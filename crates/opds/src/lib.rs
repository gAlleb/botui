//! OPDS Atom and JSON catalogs: no BookOrbit routes, tokens or identifiers.
mod json;
mod search;
pub use reqwest::Url as Address;
use reqwest::{
    Url,
    blocking::{Client, Response},
    redirect::Policy,
};
use roxmltree::Node;
pub use search::Search;
use std::{
    error::Error,
    io::{self, Read, Write},
    net::IpAddr,
    sync::atomic::AtomicBool,
    time::Duration,
};
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
#[derive(Clone)]
pub struct Catalog {
    http: Client,
    origin: Url,
    auth: Option<(String, String)>,
}
pub struct Feed {
    pub search: Option<Search>,
    pub title: String,
    pub items: Vec<Entry>,
    pub previous: Option<Url>,
    pub next: Option<Url>,
}
#[derive(Clone)]
pub enum Entry {
    Group(String, Vec<Entry>),
    Directory(String, Url),
    Book {
        title: String,
        authors: Vec<String>,
        files: Vec<File>,
        cover: Option<Url>,
        full_cover: Option<Url>,
    },
}
#[derive(Clone)]
pub struct File {
    pub href: Url,
    pub format: String,
    pub size: Option<u64>,
}
fn validated(mut u: Url) -> Result<Url> {
    if !u.username().is_empty() || u.password().is_some() {
        return Err("Enter credentials separately from the feed address.".into());
    }
    let h = u.host_str().ok_or("Expected an OPDS network address.")?;
    if u.scheme() == "http" && h == "localhost" {
        u.set_host(Some("127.0.0.1"))?;
    } else if u.scheme() != "https"
        && !(u.scheme() == "http"
            && h.trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|i| i.is_loopback()))
    {
        return Err("Use HTTPS or local HTTP.".into());
    }
    u.set_fragment(None);
    Ok(u)
}
impl Catalog {
    pub fn new(address: &str, user: String, pass: String) -> Result<Self> {
        Ok(Self {
            http: Client::builder()
                .redirect(Policy::none())
                .connect_timeout(Duration::from_secs(6))
                .build()?,
            origin: validated(Url::parse(address).map_err(|_| "Invalid OPDS address")?)?,
            auth: (!user.is_empty()).then_some((user, pass)),
        })
    }
    pub fn root(&self) -> Url {
        self.origin.clone()
    }
    fn get(&self, mut href: Url, accept: &str, timeout: Option<u64>) -> Result<Response> {
        for _ in 0..5 {
            href = validated(href)?;
            let mut req = self.http.get(href.clone()).header("Accept", accept);
            if let Some(seconds) = timeout {
                req = req.timeout(Duration::from_secs(seconds));
            }
            if href.origin() == self.origin.origin()
                && let Some((u, p)) = &self.auth
            {
                req = req.basic_auth(u, Some(p));
            }
            let response = req.send().map_err(|e| e.without_url())?;
            if response.status().is_redirection() {
                href = href.join(
                    response
                        .headers()
                        .get("location")
                        .ok_or("Empty OPDS redirect")?
                        .to_str()?,
                )?;
                continue;
            }
            if !response.status().is_success() {
                return Err(format!("OPDS returned HTTP {}.", response.status().as_u16()).into());
            }
            return Ok(response);
        }
        Err("Too many OPDS redirects".into())
    }
    pub fn feed(&self, href: Url) -> Result<Feed> {
        let response = self.get(href, "application/opds+json, application/atom+xml, application/opds-publication+json, application/xml;q=0.8, application/json;q=0.8", Some(20))?;
        let base = response.url().clone();
        let bytes = bounded(response, 4 << 20)?;
        let text = std::str::from_utf8(&bytes)?
            .trim_start_matches('﻿')
            .trim_start();
        if text.starts_with('{') {
            json::parse(&base, text)
        } else {
            parse(&base, text)
        }
    }
    pub fn cover(&self, href: Url) -> Result<Vec<u8>> {
        bounded(self.get(href, "image/*", Some(12))?, 2 << 20)
    }
    pub fn full_cover(&self, href: Url) -> Result<Vec<u8>> {
        bounded(self.get(href, "image/*", Some(20))?, 32 << 20)
    }

    pub fn download(
        &self,
        href: Url,
        writer: &mut dyn Write,
        cancel: &AtomicBool,
        limit: u64,
        progress: &botui_transfer::Progress,
    ) -> Result<()> {
        botui_transfer::check_cancel(cancel)?;
        // Keep the blocking client's per-read timeout without a total transfer deadline.
        let resp = self.get(href, "*/*", None)?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err("OPDS returned an incomplete book.".into());
        }
        if resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|s| s.to_str().ok())
            .is_some_and(|s| {
                s.split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case("text/html")
            })
        {
            return Err("OPDS returned an HTML page instead of a book.".into());
        }
        let expected = resp.content_length();
        botui_transfer::stream(resp, writer, expected, limit, cancel, progress)?;
        Ok(())
    }
}
fn bounded(r: Response, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    r.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("OPDS response too large").into());
    }
    Ok(bytes)
}
const ATOM: &str = "http://www.w3.org/2005/Atom";
fn child<'a, 'i>(n: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name((ATOM, tag)))
}
fn label(n: Node<'_, '_>, tag: &str) -> String {
    child(n, tag)
        .and_then(|c| c.text())
        .unwrap_or("")
        .trim()
        .to_owned()
}
fn url(base: &Url, n: Node<'_, '_>) -> Option<Url> {
    let mut b = base.clone();
    let chain: Vec<_> = n
        .ancestors()
        .filter_map(|a| a.attribute(("http://www.w3.org/XML/1998/namespace", "base")))
        .collect();
    for part in chain.into_iter().rev() {
        b = b.join(part).ok()?;
    }
    validated(b.join(n.attribute("href")?).ok()?).ok()
}
fn parse(base: &Url, xml: &str) -> Result<Feed> {
    let dom = roxmltree::Document::parse(xml)?;
    let root = dom.root_element();
    if !root.has_tag_name((ATOM, "feed")) {
        return Err("Expected an OPDS 1.x Atom feed".into());
    }
    let link = |rel| {
        root.children()
            .filter(|n| n.has_tag_name((ATOM, "link")) && n.attribute("rel") == Some(rel))
            .find_map(|n| url(base, n))
    };
    let mut items = vec![];
    for entry in root.children().filter(|n| n.has_tag_name((ATOM, "entry"))) {
        let title = label(entry, "title");
        let mut files = vec![];
        let mut cover = None;
        let mut original = None;
        let mut sub = None;
        for n in entry.children().filter(|n| n.has_tag_name((ATOM, "link"))) {
            let Some(href) = url(base, n) else { continue };
            let rel = n.attribute("rel").unwrap_or("alternate");
            let mime = n
                .attribute("type")
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            if rel == "http://opds-spec.org/image" {
                original = Some(href);
            } else if rel == "http://opds-spec.org/image/thumbnail" {
                cover = Some(href);
            } else if rel == "http://opds-spec.org/acquisition"
                || rel == "http://opds-spec.org/acquisition/open-access"
            {
                let fmt = format(mime, &href);
                files.push(File {
                    href,
                    format: fmt,
                    size: n.attribute("length").and_then(|n| n.parse().ok()),
                });
            } else if mime == "application/atom+xml" && matches!(rel, "subsection" | "alternate") {
                sub = Some(href);
            }
        }
        if !files.is_empty() {
            let authors = entry
                .children()
                .filter(|c| c.has_tag_name((ATOM, "author")))
                .map(|c| label(c, "name"))
                .collect();
            items.push(Entry::Book {
                title,
                authors,
                files,
                full_cover: original.clone().or_else(|| cover.clone()),
                cover: cover.or(original),
            });
        } else if let Some(href) = sub {
            items.push(Entry::Directory(title, href));
        }
    }
    Ok(Feed {
        search: search::advertised(base, root),
        title: label(root, "title"),
        items,
        next: link("next"),
        previous: link("previous"),
    })
}

fn format(mime: &str, href: &Url) -> String {
    match mime {
        "application/epub+zip" => "epub",
        "application/pdf" => "pdf",
        "application/vnd.comicbook+zip" | "application/x-cbz" => "cbz",
        "application/vnd.comicbook-rar" | "application/x-cbr" => "cbr",
        "image/vnd.djvu" | "image/x-djvu" => "djvu",
        "application/fb2+xml" | "application/x-fictionbook+xml" => "fb2",
        _ => href
            .path()
            .rsplit('/')
            .next()
            .and_then(|v| v.rsplit_once('.'))
            .map(|(_, ext)| ext)
            .filter(|v| v.len() <= 12)
            .unwrap_or("file"),
    }
    .to_owned()
}

#[cfg(test)]
mod artwork_tests {
    use super::*;
    #[test]
    fn thumbnail_and_original_survive_both_feed_formats() {
        let base = Url::parse("https://catalog.invalid/books/").unwrap();
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>Atlas</title>
          <link rel="http://opds-spec.org/image/thumbnail" href="small.jpg"/>
          <link rel="http://opds-spec.org/image" href="large.jpg"/>
          <link rel="http://opds-spec.org/acquisition" href="atlas.epub"/></entry></feed>"#;
        let json = r#"{"metadata":{"title":"Maps"},"publications":[{"metadata":{"title":"Atlas"},
          "links":[{"rel":"download","href":"atlas.epub"}],
          "images":[{"href":"large.jpg","width":800},{"href":"small.jpg","width":100}]}]}"#;
        for feed in [
            parse(&base, atom).unwrap(),
            json::parse(&base, json).unwrap(),
        ] {
            let Entry::Book {
                cover: Some(thumb),
                full_cover: Some(full),
                ..
            } = &feed.items[0]
            else {
                panic!("missing artwork")
            };
            assert_eq!(thumb.path(), "/books/small.jpg");
            assert_eq!(full.path(), "/books/large.jpg");
        }
    }
}
