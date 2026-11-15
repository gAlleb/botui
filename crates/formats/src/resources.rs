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
fn is_footnote(el: scraper::ElementRef<'_>) -> bool {
    el.value().name() == "aside"
        || ["epub:type", "role"]
            .iter()
            .filter_map(|attr| el.attr(attr))
            .flat_map(str::split_ascii_whitespace)
            .any(|t| matches!(t, "footnote" | "endnote" | "doc-footnote" | "doc-endnote"))
}

fn return_arrow(label: &str) -> bool {
    matches!(label, "↩" | "↵" | "↩︎" | "⏎")
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
    let (note, backlink) =
        supplementary_document(path, member, fragment, origin, &Default::default())?;
    Ok(Supplement {
        text: note
            .blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
        backlink,
    })
}

/// Read the selected note or notes section. Its passages point into the original book.
pub fn supplementary_document(
    path: &Path,
    member: &str,
    fragment: &str,
    origin: Option<(&str, &str)>,
    book: &botui_core::Document,
) -> Result<(botui_core::Document, Option<String>)> {
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
        .find(|e| is_footnote(*e) || matches!(e.value().name(), "li" | "dt" | "dd"))
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
    let links = scraper::Selector::parse("a[href]").unwrap();
    let mut footer = None;
    // Some publishers put the return link in the paragraph just after the note.
    if let Some(next) = target.next_siblings().find_map(scraper::ElementRef::wrap)
        && next.attr("id").is_none()
        && matches!(next.value().name(), "p" | "a" | "span")
    {
        let next_links = std::iter::once(next).chain(next.select(&links));
        for link in next_links {
            let Some((file, id)) = link
                .attr("href")
                .and_then(|href| epub::link_address(member, href))
            else {
                continue;
            };
            let label = xhtml::clean(&link.text().collect::<String>());
            let marked = ["epub:type", "role"]
                .iter()
                .filter_map(|attr| link.attr(attr))
                .flat_map(str::split_ascii_whitespace)
                .any(|v| matches!(v, "backlink" | "doc-backlink"))
                || return_arrow(&label);
            if marked && !label.is_empty() {
                if origin == Some((&file, &id)) {
                    footer = Some(label);
                    break;
                }
                if origin.is_none() && is_footnote(target) {
                    elements.push(next);
                    break;
                }
            }
        }
    }

    let mut note = botui_core::Document::default();
    let mut raw = Vec::new();
    for part in elements {
        xhtml::extract_element(
            part,
            &mut note.blocks,
            &mut raw,
            &mut Default::default(),
            None,
            &mut Default::default(),
        );
    }
    for link in raw.into_iter().filter(|link| !link.image) {
        let Some(key) = epub::link_address(member, &link.href) else {
            continue;
        };
        if !link.label.is_empty()
            && (origin == Some((&key.0, &key.1))
                || (origin.is_none() && (link.backlink || return_arrow(&link.label))))
        {
            footer.get_or_insert_with(|| link.label.clone());
        }
        let Some(&anchor) = book
            .epub_anchors
            .get(&key)
            .filter(|a| a.block < book.blocks.len())
        else {
            continue;
        };
        let Some(bytes) = link.text_range(&note.blocks) else {
            continue;
        };
        note.references.push(botui_core::Reference {
            source: link.block,
            bytes: Some(bytes),
            label: link.label,
            target: botui_core::Target::Passage(anchor),
            backlink: true,
        });
    }
    if note.blocks.is_empty() {
        return Err(invalid("No readable text at this link").into());
    }
    Ok((note, footer))
}

pub(super) fn raster(mime: &str) -> bool {
    matches!(
        mime,
        "image/jpeg" | "image/jpg" | "image/png" | "image/webp"
    )
}
