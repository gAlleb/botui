//! An OPDS 1.x/Atom source: no BookOrbit routes, tokens or identifiers.
pub use reqwest::Url as Address;
use reqwest::{
    Url,
    blocking::{Client, Response},
    redirect::Policy,
};
use roxmltree::Node;
use std::{
    error::Error,
    io::{self, Read, Write},
    net::IpAddr,
    sync::atomic::{AtomicBool, Ordering},
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
    pub title: String,
    pub items: Vec<Entry>,
    pub previous: Option<Url>,
    pub next: Option<Url>,
}
pub enum Entry {
    Directory(String, Url),
    Book {
        title: String,
        authors: Vec<String>,
        files: Vec<File>,
        cover: Option<Url>,
    },
}
pub struct File {
    pub href: Url,
    pub format: String,
    pub size: Option<u64>,
}
fn validated(mut u: Url) -> Result<Url> {
    if !u.username().is_empty() || u.password().is_some() {
        return Err("Учётные данные нужно вводить отдельно от адреса.".into());
    }
    let h = u.host_str().ok_or("Нужен сетевой адрес OPDS.")?;
    if u.scheme() == "http" && h == "localhost" {
        u.set_host(Some("127.0.0.1"))?;
    } else if u.scheme() != "https"
        && !(u.scheme() == "http"
            && h.trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|i| i.is_loopback()))
    {
        return Err("Нужен HTTPS либо локальный HTTP.".into());
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
            origin: validated(Url::parse(address).map_err(|_| "Некорректный адрес OPDS")?)?,
            auth: (!user.is_empty()).then_some((user, pass)),
        })
    }
    pub fn root(&self) -> Url {
        self.origin.clone()
    }
    fn get(&self, mut href: Url, accept: &str, timeout: u64) -> Result<Response> {
        for _ in 0..5 {
            href = validated(href)?;
            let mut req = self
                .http
                .get(href.clone())
                .header("Accept", accept)
                .timeout(Duration::from_secs(timeout));
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
                        .ok_or("Пустой редирект OPDS")?
                        .to_str()?,
                )?;
                continue;
            }
            if !response.status().is_success() {
                return Err(
                    format!("Каталог OPDS вернул HTTP {}.", response.status().as_u16()).into(),
                );
            }
            return Ok(response);
        }
        Err("Слишком много перенаправлений OPDS".into())
    }
    pub fn feed(&self, href: Url) -> Result<Feed> {
        let response = self.get(href, "application/atom+xml, application/xml;q=0.8", 20)?;
        let base = response.url().clone();
        let bytes = bounded(response, 4 << 20)?;
        parse(&base, std::str::from_utf8(&bytes)?)
    }
    pub fn cover(&self, href: Url) -> Result<Vec<u8>> {
        bounded(self.get(href, "image/*", 12)?, 2 << 20)
    }
    pub fn download(&self, href: Url, writer: &mut dyn Write, cancel: &AtomicBool) -> Result<()> {
        let resp = self.get(href, "*/*", 180)?;
        let expected = resp.content_length();
        let limit = 512 << 20;
        if expected.is_some_and(|n| n > limit) {
            return Err("Файл превышает 512 МиБ".into());
        }
        let mut src = resp.take(limit + 1);
        let mut buf = [0; 32768];
        let mut count = 0;
        while !cancel.load(Ordering::Acquire) {
            let n = src.read(&mut buf)?;
            if n == 0 {
                if count > 0 && expected.is_none_or(|len| len == count) {
                    return Ok(());
                }
                break;
            }
            count += n as u64;
            if count > limit {
                break;
            }
            writer.write_all(&buf[..n])?;
        }
        Err("Загрузка OPDS прервана или размер не совпал.".into())
    }
}
fn bounded(r: Response, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    r.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("Ответ OPDS слишком велик").into());
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
        return Err("Ожидалась лента OPDS 1.x (Atom)".into());
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
            if rel == "http://opds-spec.org/image/thumbnail"
                || (cover.is_none() && rel == "http://opds-spec.org/image")
            {
                cover = Some(href);
            } else if rel == "http://opds-spec.org/acquisition"
                || rel == "http://opds-spec.org/acquisition/open-access"
            {
                let fmt = match mime {
                    "application/epub+zip" => "epub",
                    "application/pdf" => "pdf",
                    "image/vnd.djvu" | "image/x-djvu" => "djvu",
                    "application/fb2+xml" | "application/x-fictionbook+xml" => "fb2",
                    _ => href
                        .path()
                        .rsplit('/')
                        .next()
                        .and_then(|v| v.rsplit_once('.'))
                        .map(|(_, ext)| ext)
                        .filter(|v| v.len() <= 12)
                        .unwrap_or("файл"),
                }
                .to_owned();
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
                cover,
            });
        } else if let Some(href) = sub {
            items.push(Entry::Directory(title, href));
        }
    }
    Ok(Feed {
        title: label(root, "title"),
        items,
        next: link("next"),
        previous: link("previous"),
    })
}
