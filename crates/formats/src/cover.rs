use super::{FileKind, Result, decode_xml, epub, invalid};
use base64::{Engine, engine::general_purpose::STANDARD};
use roxmltree::Document;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

const MAX_COVER: u64 = 8 << 20;
pub fn cover(path: &Path) -> Result<Option<Vec<u8>>> {
    match FileKind::from_path(path) {
        Some(FileKind::Cbz | FileKind::Cbr) => {
            let doc = super::comic::open(path)?;
            super::comic::image(path, &doc.image_pages[0]).map(Some)
        }
        Some(FileKind::Epub) => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
            let mut budget = 4 << 20;
            let container = epub::read_member(&mut zip, "META-INF/container.xml", &mut budget)?;
            let dom = Document::parse(&container)?;
            let Some(opf) = dom
                .descendants()
                .find(|n| n.has_tag_name("rootfile"))
                .and_then(|n| n.attribute("full-path"))
            else {
                return Ok(None);
            };
            let xml = epub::read_member(&mut zip, opf, &mut budget)?;
            let doc = Document::parse(&xml)?;
            let id = doc
                .descendants()
                .find(|n| n.has_tag_name("meta") && n.attribute("name") == Some("cover"))
                .and_then(|n| n.attribute("content"));
            let image = doc.descendants().find(|n| {
                n.has_tag_name("item")
                    && n.attribute("media-type")
                        .is_some_and(|t| t.starts_with("image/"))
                    && (n
                        .attribute("properties")
                        .unwrap_or("")
                        .split_whitespace()
                        .any(|v| v == "cover-image")
                        || id.is_some() && n.attribute("id") == id)
            });
            let Some(href) = image.and_then(|n| n.attribute("href")) else {
                return Ok(None);
            };
            let member_path = epub::member_path(opf, href)?;
            if image.and_then(|n| n.attribute("media-type")) == Some("image/svg+xml") {
                return super::svg::read(&mut zip, &member_path, &mut (8 << 20)).map(Some);
            }
            let member = zip.by_name(&member_path)?;
            if member.size() > MAX_COVER {
                return Ok(None);
            }
            let mut bytes = Vec::new();
            member.take(MAX_COVER + 1).read_to_end(&mut bytes)?;
            Ok((bytes.len() <= MAX_COVER as usize).then_some(bytes))
        }
        Some(FileKind::Fb2) => {
            let f = File::open(path)?;
            const LIMIT: u64 = 32 << 20;
            if f.metadata()?.len() > LIMIT {
                return Ok(None);
            }
            let mut bytes = Vec::new();
            f.take(LIMIT + 1).read_to_end(&mut bytes)?;
            if bytes.len() > LIMIT as usize {
                return Ok(None);
            }
            let xml = decode_xml(&bytes)?;
            let doc = Document::parse(&xml)?;
            let image = doc
                .descendants()
                .find(|n| n.has_tag_name("coverpage"))
                .and_then(|n| n.descendants().find(|c| c.has_tag_name("image")));
            let Some(id) = image
                .and_then(|n| n.attributes().find(|a| a.name() == "href"))
                .and_then(|a| a.value().strip_prefix('#'))
            else {
                return Ok(None);
            };
            let Some(data) = doc
                .descendants()
                .find(|n| n.has_tag_name("binary") && n.attribute("id") == Some(id))
                .and_then(|b| b.text())
            else {
                return Ok(None);
            };
            if data.len() as u64 > MAX_COVER * 2 {
                return Ok(None);
            }
            let bytes = STANDARD
                .decode(
                    data.bytes()
                        .filter(|b| !b.is_ascii_whitespace())
                        .collect::<Vec<_>>(),
                )
                .map_err(|_| invalid("Invalid FB2 cover"))?;
            Ok((bytes.len() <= MAX_COVER as usize).then_some(bytes))
        }
        _ => Ok(None),
    }
}
