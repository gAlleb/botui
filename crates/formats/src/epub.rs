use crate::{MAX_TEXT_BYTES, Result, decode_xml, invalid, xhtml};
const MAX_MEMBER: u64 = 16 << 20;
use botui_core::{Block, Document};
use percent_encoding::percent_decode_str;
use roxmltree::{Document as Xml, Node};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use zip::ZipArchive;

/// Open an EPUB without unpacking anything onto disk or resolving external resources.
pub fn open_epub(path: &Path) -> Result<Document> {
    let fallback = path.file_stem().unwrap_or_default().to_string_lossy();
    read_book(BufReader::new(File::open(path)?), &fallback)
}

pub(super) fn read_book(reader: impl Read + Seek, fallback: &str) -> Result<Document> {
    let mut archive = ZipArchive::new(reader)?;
    let mut remaining = MAX_TEXT_BYTES;
    let container = read_member(&mut archive, "META-INF/container.xml", &mut remaining)?;
    let container = Xml::parse(&container)?;
    let package_path = container
        .descendants()
        .find(|n| tag(*n, "rootfile") && n.attribute("full-path").is_some())
        .and_then(|n| n.attribute("full-path"))
        .ok_or_else(|| invalid("В EPUB не указан путь к описанию книги (OPF)"))?;
    let package_path = member_path("", package_path)?;
    let package = read_member(&mut archive, &package_path, &mut remaining)?;
    let package = Xml::parse(&package)?;
    let root = package.root_element();
    let section = |name| root.children().find(|n| tag(*n, name));
    let title = section("metadata")
        .and_then(|meta| meta.children().find(|n| tag(*n, "title")))
        .map(|n| {
            xhtml::clean(
                &n.descendants()
                    .filter(|n| n.is_text())
                    .filter_map(|n| n.text())
                    .collect::<String>(),
            )
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| xhtml::clean(fallback));
    let manifest = section("manifest").ok_or_else(|| invalid("В EPUB отсутствует manifest"))?;
    let items: HashMap<_, _> = manifest
        .children()
        .filter(|n| tag(*n, "item"))
        .filter_map(|n| Some((n.attribute("id")?, n)))
        .collect();
    let spine = section("spine").ok_or_else(|| invalid("В EPUB не указан порядок глав"))?;
    let mut locations = botui_core::Locations::default();
    let mut blocks = Vec::<Block>::new();
    for item in spine
        .children()
        .filter(|n| tag(*n, "itemref") && n.attribute("linear") != Some("no"))
    {
        let spine_path = super::locations::steps(item);
        let id = item
            .attribute("idref")
            .ok_or_else(|| invalid("В spine отсутствует idref"))?;
        let item = items
            .get(id)
            .ok_or_else(|| invalid(format!("В manifest отсутствует глава {id:?}")))?;
        if !matches!(
            item.attribute("media-type"),
            Some("application/xhtml+xml" | "text/html")
        ) {
            continue;
        }
        let href = item
            .attribute("href")
            .ok_or_else(|| invalid("У главы отсутствует href"))?;
        let member = member_path(&package_path, href)?;
        let html = read_member(&mut archive, &member, &mut remaining)?;
        let start = blocks.len();
        xhtml::append_blocks(&html, &mut blocks);
        super::locations::epub(&html, &spine_path, start, &blocks[start..], &mut locations);
    }
    if blocks.is_empty() {
        return Err(invalid("В EPUB не найден текст для чтения").into());
    }
    Ok(Document {
        title,
        blocks,
        locations,
    })
}

fn tag(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

// Resolve the ZIP member relative to the referring document, including legitimate "../".
pub(super) fn member_path(base: &str, href: &str) -> Result<String> {
    let path = href.split(['#', '?']).next().unwrap_or_default();
    let path = percent_decode_str(path).decode_utf8()?;
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .next()
            .is_some_and(|first| first.contains(':'))
    {
        return Err(invalid(format!("Некорректный путь EPUB: {href:?}")).into());
    }
    let mut parts: Vec<_> = base.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts
                    .pop()
                    .ok_or_else(|| invalid("Путь главы ведёт за пределы EPUB"))?;
            }
            name => parts.push(name),
        }
    }
    Ok(parts.join("/"))
}

fn read_member<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    remaining: &mut u64,
) -> Result<String> {
    let mut item = archive
        .by_name(path)
        .map_err(|e| invalid(format!("Не удалось открыть {path:?} в EPUB: {e}")))?;
    let limit = MAX_MEMBER.min(*remaining);
    if item.size() > limit {
        return Err(invalid("Превышен лимит размера текста EPUB").into());
    }
    let mut bytes = Vec::new();
    (&mut item).take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("Превышен лимит размера текста EPUB").into());
    }
    *remaining -= bytes.len() as u64;
    Ok(decode_xml(&bytes)?.into_owned())
}
