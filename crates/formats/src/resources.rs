//! Open only embedded resources named by the document; never fetch a remote illustration.
use super::{FileKind, Result, decode_xml, epub, invalid, xhtml};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};
pub(super) fn web(value: &str) -> bool {
    (value.starts_with("https://") || value.starts_with("http://"))
        && !value.chars().any(char::is_control)
}
pub fn illustration(path: &Path, member: &str) -> Result<Vec<u8>> {
    const LIMIT: u64 = 16 << 20;
    match FileKind::from_path(path) {
        Some(FileKind::Markdown) => {
            let member = percent_encoding::percent_decode_str(member).decode_utf8()?;
            let base = path
                .canonicalize()?
                .parent()
                .ok_or_else(|| invalid("Missing Markdown directory"))?
                .to_path_buf();
            let image = base.join(member.as_ref()).canonicalize()?;
            if !image.starts_with(&base) {
                return Err(
                    invalid("Markdown images must be inside the document directory").into(),
                );
            }
            let mut bytes = Vec::new();
            File::open(image)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > LIMIT {
                return Err(invalid("Illustration exceeds size limit").into());
            }
            Ok(bytes)
        }
        Some(FileKind::Cbz | FileKind::Cbr) => super::comic::image(path, member),
        Some(FileKind::Epub) => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
            let mut budget = LIMIT;
            super::svg::read(&mut zip, member, &mut budget)
        }
        Some(FileKind::Fb2) => {
            let mut bytes = vec![];
            File::open(path)?
                .take(super::MAX_TEXT_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > super::MAX_TEXT_BYTES {
                return Err(invalid("FB2 exceeds size limit").into());
            }
            let xml = decode_xml(&bytes)?;
            let doc = roxmltree::Document::parse(&xml)?;
            let data = doc
                .descendants()
                .find(|n| n.has_tag_name("binary") && n.attribute("id") == Some(member))
                .and_then(|v| v.text())
                .ok_or_else(|| invalid("Embedded FB2 image not found"))?;
            if data.len() as u64 > LIMIT * 2 {
                return Err(invalid("Illustration exceeds size limit").into());
            }
            Ok(STANDARD.decode(
                data.bytes()
                    .filter(|v| !v.is_ascii_whitespace())
                    .collect::<Vec<_>>(),
            )?)
        }
        _ => Err(invalid("Unknown image source").into()),
    }
}
pub struct Supplement {
    pub text: String,
    pub backlink: Option<String>,
}
pub fn supplement(path: &Path, member: &str, fragment: &str) -> Result<String> {
    Ok(supplementary_note(path, member, fragment, None)?.text)
}
pub fn supplementary_note(
    path: &Path,
    member: &str,
    fragment: &str,
    origin: Option<(&str, &str)>,
) -> Result<Supplement> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    let html = epub::read_member(&mut zip, member, &mut (4 << 20))?;
    let dom = xhtml::parse_document(&html);
    let node = dom
        .tree
        .nodes()
        .filter_map(scraper::ElementRef::wrap)
        .find(|e| {
            if fragment.is_empty() {
                e.value().name() == "body"
            } else {
                e.attr("id") == Some(fragment) || e.attr("name") == Some(fragment)
            }
        })
        .ok_or_else(|| invalid("Link target not found in this edition"))?;
    // A bookmark is frequently an inline number or empty anchor inside an entire
    // footnote. In a glossary the definition follows the term as its sibling.
    let target = std::iter::once(node)
        .chain(node.ancestors().filter_map(scraper::ElementRef::wrap))
        .find(|e| {
            matches!(e.value().name(), "aside" | "li" | "dt" | "dd")
                || ["epub:type", "role"]
                    .iter()
                    .filter_map(|attr| e.attr(attr))
                    .flat_map(str::split_ascii_whitespace)
                    .any(|t| matches!(t, "footnote" | "endnote" | "doc-footnote" | "doc-endnote"))
        })
        .unwrap_or_else(|| {
            if matches!(node.value().name(), "a" | "span" | "sup") {
                node.ancestors()
                    .filter_map(scraper::ElementRef::wrap)
                    .find(|e| matches!(e.value().name(), "p" | "div" | "section"))
                    .unwrap_or(node)
            } else {
                node
            }
        });
    let mut elements = vec![target];
    if target.value().name() == "dt" {
        elements.extend(
            target
                .next_siblings()
                .filter_map(scraper::ElementRef::wrap)
                .take_while(|e| e.value().name() == "dd"),
        );
    }
    let mut parts = vec![];
    let links = scraper::Selector::parse("a[href]").unwrap();
    let back_label = |anchor: scraper::ElementRef<'_>| {
        let label = xhtml::clean(&anchor.text().collect::<String>());
        let marked = ["epub:type", "role"]
            .iter()
            .filter_map(|attr| anchor.attr(attr))
            .flat_map(str::split_ascii_whitespace)
            .any(|v| matches!(v, "backlink" | "doc-backlink"))
            || matches!(label.as_str(), "↩" | "↵" | "↩︎" | "⏎");
        (marked && !label.is_empty()).then_some(label)
    };
    // Some editions put the return arrow in its own paragraph immediately after
    // the aside. Only adopt an adjacent link pointing back to this particular citation.
    let mut backlink = origin.and_then(|(source, id)| {
        let next = target.next_siblings().find_map(scraper::ElementRef::wrap)?;
        if next.attr("id").is_some() || !matches!(next.value().name(), "p" | "a" | "span") {
            return None;
        }
        std::iter::once(next)
            .chain(next.select(&links))
            .find_map(|link| {
                let (href, fragment) = link.attr("href")?.split_once('#')?;
                let dest = if href.is_empty() {
                    member.to_owned()
                } else {
                    epub::member_path(member, href).ok()?
                };
                let fragment = percent_encoding::percent_decode_str(fragment)
                    .decode_utf8()
                    .ok()?;
                if dest != source || fragment != id {
                    return None;
                }
                back_label(link)
            })
    });
    for part in elements {
        backlink = backlink.or_else(|| part.select(&links).find_map(back_label));
        let selector = scraper::Selector::parse("p,h1,h2,h3,h4,dt,dd").unwrap();
        let paragraphs: Vec<_> = part
            .select(&selector)
            .filter(|e| e.select(&selector).next().is_none())
            .map(|e| xhtml::clean(&e.text().collect::<String>()))
            .filter(|s| !s.is_empty())
            .collect();
        if paragraphs.is_empty() {
            parts.push(xhtml::clean(&part.text().collect::<String>()));
        } else {
            parts.extend(paragraphs);
        }
    }
    parts.retain(|t| !t.is_empty());
    if parts.is_empty() {
        return Err(invalid("No readable text at this link").into());
    }
    Ok(Supplement {
        text: parts.join("\n\n"),
        backlink,
    })
}

pub(super) fn raster(mime: &str) -> bool {
    matches!(
        mime,
        "image/jpeg" | "image/jpg" | "image/png" | "image/webp"
    )
}
