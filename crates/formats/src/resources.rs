//! Open only embedded resources named by the document; never fetch a remote illustration.
use super::{FileKind, Result, decode_xml, epub, invalid, xhtml};
use base64::{Engine, engine::general_purpose::STANDARD};
use scraper::Html;
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
        Some(FileKind::Epub) => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
            let file = zip.by_name(member)?;
            if file.size() > LIMIT {
                return Err(invalid("Illustration exceeds size limit").into());
            }
            let mut bytes = vec![];
            file.take(LIMIT + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > LIMIT {
                return Err(invalid("Illustration exceeds size limit").into());
            }
            Ok(bytes)
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
pub fn supplement(path: &Path, member: &str, fragment: &str) -> Result<String> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    let html = epub::read_member(&mut zip, member, &mut (4 << 20))?;
    let dom = Html::parse_document(&html);
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
    let node = if node.text().all(|v| v.trim().is_empty()) {
        node.parent()
            .and_then(scraper::ElementRef::wrap)
            .unwrap_or(node)
    } else {
        node
    };
    let mut parts = vec![];
    for child in node.select(&scraper::Selector::parse("p,li,h1,h2,h3").unwrap()) {
        let t = xhtml::clean(&child.text().collect::<String>());
        if !t.is_empty() {
            parts.push(t);
        }
    }
    let s = if parts.is_empty() {
        xhtml::clean(&node.text().collect::<String>())
    } else {
        parts.join("\n\n")
    };
    if s.is_empty() {
        return Err(invalid("No readable text at this link").into());
    }
    Ok(s)
}

pub(super) fn raster(mime: &str) -> bool {
    matches!(
        mime,
        "image/jpeg" | "image/jpg" | "image/png" | "image/webp"
    )
}
