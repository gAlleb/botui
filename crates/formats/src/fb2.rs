use crate::{MAX_TEXT_BYTES, Result, decode_xml, invalid, xhtml::clean};
use botui_core::{Block, Document};
use roxmltree::{Document as Xml, Node};
use std::{fs::File, io::Read, path::Path};

pub(super) fn open(path: &Path) -> Result<Document> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_TEXT_BYTES {
        return Err(invalid("Превышен лимит размера FB2").into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err(invalid("Превышен лимит размера FB2").into());
    }
    parse(
        &bytes,
        &path.file_stem().unwrap_or_default().to_string_lossy(),
    )
}

fn text(node: Node<'_, '_>) -> String {
    let mut text = String::new();
    for child in node.descendants().filter(|n| n.is_text()) {
        text.push_str(child.text().unwrap_or_default());
    }
    clean(&text)
}

pub(super) fn parse(bytes: &[u8], fallback: &str) -> Result<Document> {
    let xml = decode_xml(bytes)?;
    let doc = Xml::parse(&xml)?;
    let root = doc.root_element();
    if !root.has_tag_name("FictionBook") {
        return Err(invalid("Не найден корневой элемент FictionBook").into());
    }
    let title = root
        .children()
        .find(|n| n.has_tag_name("description"))
        .and_then(|d| d.children().find(|n| n.has_tag_name("title-info")))
        .and_then(|info| info.children().find(|n| n.has_tag_name("book-title")))
        .map(text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| clean(fallback));
    let mut locations = botui_core::Locations::default();
    let mut blocks = Vec::new();
    for body in root.children().filter(|n| n.has_tag_name("body")) {
        if matches!(body.attribute("name"), Some(name) if name.eq_ignore_ascii_case("notes") || name.eq_ignore_ascii_case("comments"))
        {
            continue;
        }
        let paths = super::locations::FbPaths::new(body);
        for node in body.descendants().filter(|n| n.is_element()) {
            let heading = match node.tag_name().name() {
                "title"
                    if !node
                        .children()
                        .any(|n| n.is_element() && n.has_tag_name("p")) =>
                {
                    true
                }
                "p" => node
                    .parent_element()
                    .is_some_and(|p| p.has_tag_name("title")),
                "subtitle" => true,
                "v" | "text-author" | "td" | "th" => false,
                _ => continue,
            };
            let content = text(node);
            if !content.is_empty() {
                if let Some(runs) = paths.paragraph(node, &content) {
                    locations.0.insert(blocks.len(), runs);
                }
                blocks.push(Block {
                    text: content,
                    heading,
                });
            }
        }
    }
    if blocks.is_empty() {
        return Err(invalid("В FB2 не найден текст для чтения").into());
    }
    Ok(Document {
        title,
        blocks,
        locations,
    })
}
