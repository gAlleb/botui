use crate::{MAX_TEXT_BYTES, Result, decode_xml, invalid, xhtml::clean};
use botui_core::{Block, Document};
use roxmltree::{Document as Xml, Node};
use std::{fs::File, io::Read, path::Path};

pub(super) fn open(path: &Path) -> Result<Document> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_TEXT_BYTES {
        return Err(invalid("FB2 exceeds the size limit").into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err(invalid("FB2 exceeds the size limit").into());
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
        return Err(invalid("Missing FictionBook root element").into());
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
    let mut fixed_breaks = std::collections::BTreeSet::new();
    let mut inline = std::collections::BTreeMap::new();
    let mut contents = Vec::new();
    let mut raw = vec![];
    let mut tables = std::collections::BTreeMap::new();
    let mut ids = std::collections::HashMap::new();
    let mut note_targets = std::collections::HashMap::new();
    for body in root.children().filter(|n| n.has_tag_name("body")) {
        if matches!(body.attribute("name"), Some(name) if name.eq_ignore_ascii_case("notes") || name.eq_ignore_ascii_case("comments"))
        {
            for section in body.descendants().filter(|n| n.has_tag_name("section")) {
                if let Some(id) = section.attribute("id") {
                    note_targets.insert(id, section);
                }
            }
            continue;
        }
        let mut titled_sections = std::collections::HashMap::new();
        let paths = super::locations::FbPaths::new(body);
        // The preorder traversal emits a paragraph before visiting its inline children.
        let mut current_block = None;
        for node in body.descendants().filter(|n| n.is_element()) {
            if node.has_tag_name("empty-line") {
                fixed_breaks.insert(blocks.len());
            }
            if let Some(id) = node.attribute("id") {
                let block = current_block
                    .filter(|&(_, owner)| node.ancestors().any(|n| n.id() == owner))
                    .map_or(blocks.len(), |(at, _)| at);
                ids.insert(id, block);
            }
            if node.has_tag_name("title")
                && node
                    .parent_element()
                    .is_some_and(|p| p.has_tag_name("body") || p.has_tag_name("section"))
            {
                let parts: Vec<_> = node
                    .children()
                    .filter(|n| n.has_tag_name("p"))
                    .map(text)
                    .filter(|s| !s.is_empty())
                    .collect();
                let label = if parts.is_empty() {
                    text(node)
                } else {
                    parts.join(" ")
                };
                if !label.is_empty() {
                    let owner = node.parent_element().unwrap();
                    let mut entry = botui_core::ContentsEntry::passage(
                        botui_core::Anchor {
                            block: blocks.len(),
                            byte: 0,
                        },
                        label,
                    );
                    entry.parent = owner
                        .ancestors()
                        .skip(1)
                        .find_map(|parent| titled_sections.get(&parent.id()).copied());
                    // A title directly in <body> is front matter, alongside the outer sections.
                    if owner.has_tag_name("section") {
                        titled_sections.insert(owner.id(), contents.len());
                    }
                    contents.push(entry);
                }
            }
            if matches!(node.tag_name().name(), "a" | "image") {
                if let Some(attr) = node.attributes().find(|a| a.name() == "href") {
                    let parent_is_block = node.ancestors().skip(1).any(|n| {
                        n.has_tag_name("p") || n.has_tag_name("v") || n.has_tag_name("subtitle")
                    });
                    let block = if parent_is_block
                        && (!node.has_tag_name("image")
                            || node
                                .ancestors()
                                .skip(1)
                                .find(|n| {
                                    n.has_tag_name("p")
                                        || n.has_tag_name("v")
                                        || n.has_tag_name("subtitle")
                                })
                                .is_some_and(|n| !text(n).is_empty()))
                    {
                        blocks.len().saturating_sub(1)
                    } else {
                        blocks.len()
                    };
                    raw.push((block, node.has_tag_name("image"), text(node), attr.value()));
                }
                continue;
            }
            if node.has_tag_name("tr") {
                let mut header = false;
                let cells: Vec<_> = node
                    .children()
                    .filter(|c| c.has_tag_name("td") || c.has_tag_name("th"))
                    .map(|c| {
                        header |= c.has_tag_name("th");
                        text(c)
                    })
                    .collect();
                let count = cells.iter().filter(|s| !s.is_empty()).count();
                if count > 0 {
                    tables.insert(
                        blocks.len(),
                        botui_core::TableRow {
                            end: blocks.len() + count,
                            columns: cells.len(),
                            cells,
                            header,
                        },
                    );
                }
            }
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
                if matches!(node.tag_name().name(), "v" | "text-author") {
                    fixed_breaks.extend([blocks.len(), blocks.len() + 1]);
                }
                if let Some(runs) = paths.paragraph(node, &content) {
                    locations.0.insert(blocks.len(), runs);
                }
                current_block = Some((blocks.len(), node.id()));
                let spans = faces(node, blocks.len());
                if spans.iter().any(|s| s.face != Default::default()) {
                    inline.insert(blocks.len(), spans);
                }
                blocks.push(Block {
                    text: content,
                    heading,
                });
            }
        }
    }
    if blocks.is_empty() {
        let binaries: std::collections::HashSet<_> = root
            .children()
            .filter(|n| {
                n.has_tag_name("binary")
                    && n.attribute("content-type")
                        .is_some_and(super::resources::raster)
                    && n.text().is_some_and(|s| !s.trim().is_empty())
            })
            .filter_map(|n| n.attribute("id"))
            .collect();
        for (block, image, label, href) in &mut raw {
            if *image
                && href
                    .strip_prefix('#')
                    .is_some_and(|id| binaries.contains(id))
            {
                *block = blocks.len();
                blocks.push(Block {
                    text: if label.is_empty() {
                        format!("Plate {}", blocks.len() + 1)
                    } else {
                        label.clone()
                    },
                    heading: false,
                });
            }
        }
        if blocks.is_empty() {
            return Err(invalid("No readable text or illustrations in this FB2").into());
        }
    }
    use botui_core::{Anchor, Reference, Target};
    let mut previous = (usize::MAX, 0);
    let references = raw
        .into_iter()
        .filter_map(|(source, image, label, href)| {
            let label = if label.is_empty() {
                href.trim_start_matches('#').into()
            } else {
                label
            };
            let target = if let Some(id) = href.strip_prefix('#') {
                if image {
                    Target::Image(id.into())
                } else if let Some(n) = note_targets.get(id) {
                    let parts: Vec<_> = n
                        .descendants()
                        .filter(|n| n.has_tag_name("p"))
                        .map(text)
                        .collect();
                    Target::Note(parts.join("\n\n"))
                } else {
                    Target::Passage(Anchor {
                        block: *ids.get(id).filter(|&&i| i < blocks.len())?,
                        byte: 0,
                    })
                }
            } else if super::resources::web(href) && !image {
                Target::Web(href.into())
            } else {
                return None;
            };
            let bytes = (!image)
                .then(|| {
                    let text = &blocks.get(source)?.text;
                    let start = if previous.0 == source { previous.1 } else { 0 };
                    let from = start + text.get(start..)?.find(&label)?;
                    let range = from..from + label.len();
                    previous = (source, range.end);
                    Some(range)
                })
                .flatten();
            Some(Reference {
                bytes,
                backlink: false,
                source: if image {
                    source.min(blocks.len())
                } else {
                    source.min(blocks.len() - 1)
                },
                label,
                target,
            })
        })
        .collect();
    Ok(Document {
        fixed_breaks,
        inline,
        styled: Default::default(),
        image_pages: vec![],
        tables,
        references,
        contents,
        title,
        blocks,
        locations,
    })
}

// Match clean() byte for byte so emphasis never changes progress/link addresses.
fn faces(node: Node<'_, '_>, block: usize) -> Vec<botui_core::publisher::TextSpan> {
    if !node
        .descendants()
        .any(|n| matches!(n.tag_name().name(), "emphasis" | "strong" | "strikethrough"))
    {
        return Vec::new();
    }
    use botui_core::{
        Anchor,
        publisher::{Face, TextSpan},
    };
    let mut out = String::new();
    let mut spans: Vec<TextSpan> = Vec::new();
    for text in node.descendants().filter(|n| n.is_text()) {
        let mut face = Face::default();
        for parent in text.ancestors().take_while(|n| n.id() != node.id()) {
            match parent.tag_name().name() {
                "emphasis" => face.italic = true,
                "strong" => face.bold = true,
                "strikethrough" => face.strike = true,
                _ => {}
            }
        }
        let start = out.len();
        for ch in text.text().unwrap_or_default().chars() {
            if ch.is_whitespace() {
                if !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
            } else if !ch.is_control() {
                out.push(ch);
            }
        }
        if out.len() > start {
            if let Some(last) = spans.last_mut().filter(|s| s.face == face) {
                last.bytes.end = out.len();
                last.source_end = out.len();
            } else {
                spans.push(TextSpan {
                    bytes: start..out.len(),
                    source: Some(Anchor { block, byte: start }),
                    source_end: out.len(),
                    face,
                });
            }
        }
    }
    if out.ends_with(' ') {
        out.pop();
        if let Some(last) = spans.last_mut() {
            last.bytes.end = out.len();
            last.source_end = out.len();
        }
        spans.retain(|s| !s.bytes.is_empty());
    }
    spans
}
