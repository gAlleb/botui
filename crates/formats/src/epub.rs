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
        .ok_or_else(|| invalid("Missing EPUB package path (OPF)"))?;
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
    let manifest = section("manifest").ok_or_else(|| invalid("Missing EPUB manifest"))?;
    let items: HashMap<_, _> = manifest
        .children()
        .filter(|n| tag(*n, "item"))
        .filter_map(|n| Some((n.attribute("id")?, n)))
        .collect();
    let resources: HashMap<_, _> = items
        .values()
        .filter_map(|n| {
            Some((
                member_path(&package_path, n.attribute("href")?).ok()?,
                n.attribute("media-type")?,
            ))
        })
        .collect();
    let spine = section("spine").ok_or_else(|| invalid("Missing EPUB reading order"))?;
    let mut locations = botui_core::Locations::default();
    let mut blocks = Vec::<Block>::new();
    let mut starts = HashMap::new();
    let mut ids = HashMap::new();
    let mut raw = vec![];
    let mut tables = Default::default();
    for item in spine
        .children()
        .filter(|n| tag(*n, "itemref") && n.attribute("linear") != Some("no"))
    {
        let spine_path = super::locations::steps(item);
        let id = item
            .attribute("idref")
            .ok_or_else(|| invalid("Missing spine idref"))?;
        let item = items
            .get(id)
            .ok_or_else(|| invalid(format!("Chapter {id:?} is missing from the manifest")))?;
        if !matches!(
            item.attribute("media-type"),
            Some("application/xhtml+xml" | "text/html")
        ) {
            continue;
        }
        let href = item
            .attribute("href")
            .ok_or_else(|| invalid("Chapter has no href"))?;
        let member = member_path(&package_path, href)?;
        let html = read_member(&mut archive, &member, &mut remaining)?;
        let start = blocks.len();
        starts.insert(member.clone(), start);
        let mut links = vec![];
        for (id, block) in xhtml::append_blocks(&html, &mut blocks, &mut links, &mut tables) {
            ids.entry((member.clone(), id)).or_insert(block);
        }
        let member: std::sync::Arc<str> = member.into();
        raw.extend(links.into_iter().map(|v| (member.clone(), v)));
        super::locations::epub(&html, &spine_path, start, &blocks[start..], &mut locations);
    }
    if blocks.is_empty() {
        let mut plates = HashMap::<String, (usize, usize)>::new();
        for (base, link) in &mut raw {
            if link.image
                && member_path(base, &link.href)
                    .ok()
                    .and_then(|p| resources.get(&p))
                    .is_some_and(|m| super::resources::raster(m))
            {
                link.block = blocks.len();
                let entry = plates.entry(base.to_string()).or_insert((blocks.len(), 0));
                entry.1 += 1;
                blocks.push(Block {
                    text: if link.label.is_empty() {
                        format!("Plate {}", blocks.len() + 1)
                    } else {
                        link.label.clone()
                    },
                    heading: false,
                });
            }
        }
        starts.retain(|key, pos| {
            plates.get(key).is_some_and(|(start, _)| {
                *pos = *start;
                true
            })
        });
        // A single-plate spine item has one unambiguous destination. Do not invent
        // fragment offsets for a multi-plate chapter which contains no text anchors.
        ids.retain(|(member, _), pos| {
            plates.get(member).is_some_and(|(start, count)| {
                *pos = botui_core::Anchor {
                    block: *start,
                    byte: 0,
                };
                *count == 1
            })
        });
        if blocks.is_empty() {
            return Err(invalid("No readable text or illustrations in this EPUB").into());
        }
    }
    let mut contents = Vec::new();
    let nav = items.values().find(|n| {
        n.attribute("properties")
            .unwrap_or_default()
            .split_whitespace()
            .any(|v| v == "nav")
    });
    let ncx = spine
        .attribute("toc")
        .and_then(|id| items.get(id))
        .or_else(|| {
            items
                .values()
                .find(|n| n.attribute("media-type") == Some("application/x-dtbncx+xml"))
        });
    for (node, is_ncx) in [(nav, false), (ncx, true)] {
        if let Some(path) = node
            .and_then(|n| n.attribute("href"))
            .and_then(|h| member_path(&package_path, h).ok())
            && let Ok(xml) = read_member(&mut archive, &path, &mut remaining)
        {
            contents = super::toc::read(&path, &xml, is_ncx, &starts, &ids, blocks.len());
            if !contents.is_empty() {
                break;
            }
        }
    }
    use botui_core::{Anchor, Reference, Target};
    let references = raw
        .into_iter()
        .filter_map(|(base, link)| {
            let label = if link.label.trim().is_empty() {
                link.href
                    .rsplit('/')
                    .next()
                    .unwrap_or(&link.href)
                    .to_owned()
            } else {
                link.label
            };
            let target = if super::resources::web(&link.href) && !link.image {
                Target::Web(link.href)
            } else {
                let (part, id) = link.href.split_once('#').unwrap_or((&link.href, ""));
                let member = if part.is_empty() {
                    base.to_string()
                } else {
                    member_path(&base, part).ok()?
                };
                let id = percent_decode_str(id).decode_utf8().ok()?.into_owned();
                let mime = resources.get(&member)?;
                if link.image || mime.starts_with("image/") {
                    if !mime.starts_with("image/") {
                        return None;
                    }
                    Target::Image(member)
                } else if !matches!(*mime, "application/xhtml+xml" | "text/html") {
                    return None;
                } else if let Some(anchor) = if id.is_empty() {
                    starts.get(&member).map(|block| Anchor {
                        block: *block,
                        byte: 0,
                    })
                } else {
                    ids.get(&(member.clone(), id.clone())).copied()
                }
                .filter(|a| a.block < blocks.len())
                {
                    Target::Passage(anchor)
                } else {
                    Target::Supplement {
                        member,
                        fragment: id,
                        origin: link.id.map(|id| (base, id)),
                    }
                }
            };
            let bytes = (!link.image)
                .then(|| {
                    let tail = blocks.get(link.block)?.text.get(link.byte..)?;
                    // Leading whitespace in an anchor is collapsed into the surrounding prose.
                    let start = link.byte + tail.len() - tail.trim_start().len();
                    blocks[link.block]
                        .text
                        .get(start..)?
                        .starts_with(&label)
                        .then_some(start..start + label.len())
                })
                .flatten();
            Some(Reference {
                bytes,
                backlink: link.backlink,
                source: if link.image {
                    link.block.min(blocks.len())
                } else {
                    link.block.min(blocks.len() - 1)
                },
                label,
                target,
            })
        })
        .collect();
    Ok(Document {
        tables,
        references,
        contents,
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
        return Err(invalid(format!("Invalid EPUB path: {href:?}")).into());
    }
    let mut parts: Vec<_> = base.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts
                    .pop()
                    .ok_or_else(|| invalid("Chapter path escapes the EPUB"))?;
            }
            name => parts.push(name),
        }
    }
    Ok(parts.join("/"))
}

pub(super) fn read_member<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    remaining: &mut u64,
) -> Result<String> {
    let mut item = archive
        .by_name(path)
        .map_err(|e| invalid(format!("Could not open {path:?} in EPUB: {e}")))?;
    let limit = MAX_MEMBER.min(*remaining);
    if item.size() > limit {
        return Err(invalid("EPUB text exceeds the size limit").into());
    }
    let mut bytes = Vec::new();
    (&mut item).take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("EPUB text exceeds the size limit").into());
    }
    *remaining -= bytes.len() as u64;
    Ok(decode_xml(&bytes)?.into_owned())
}
