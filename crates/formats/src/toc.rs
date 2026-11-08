//! Read optional EPUB 3 navigation or EPUB 2 NCX, keeping spine fragment targets.
use super::{epub::member_path, xhtml::clean};
use botui_core::Anchor;
use scraper::Selector;
use std::collections::{HashMap, HashSet};

pub fn read(
    path: &str,
    xml: &str,
    is_ncx: bool,
    starts: &HashMap<String, usize>,
    ids: &HashMap<(String, String), Anchor>,
    count: usize,
) -> Vec<(Anchor, String)> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut link = |href: &str, label: String| {
        let (reference, fragment) = href.split_once('#').unwrap_or((href, ""));
        let file = if reference.is_empty() && !fragment.is_empty() {
            path.to_owned()
        } else if let Ok(file) = member_path(path, reference) {
            file
        } else {
            return;
        };
        let block = if fragment.is_empty() {
            starts.get(&file).map(|block| Anchor {
                block: *block,
                byte: 0,
            })
        } else {
            percent_encoding::percent_decode_str(fragment)
                .decode_utf8()
                .ok()
                .and_then(|id| ids.get(&(file, id.into_owned())).copied())
        };
        let title = clean(&label);
        if let Some(block) = block
            && block.block < count
            && !title.is_empty()
            && seen.insert((block.block, block.byte, title.clone()))
        {
            entries.push((block, title));
        }
    };
    if is_ncx {
        if let Ok(doc) = roxmltree::Document::parse_with_options(
            xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        ) {
            for n in doc.descendants().filter(|n| n.has_tag_name("navPoint")) {
                let src = n
                    .children()
                    .find(|c| c.has_tag_name("content"))
                    .and_then(|c| c.attribute("src"));
                let label = n.children().find(|c| c.has_tag_name("navLabel"));
                if let (Some(h), Some(l)) = (src, label) {
                    link(
                        h,
                        l.descendants()
                            .filter(|c| c.is_text())
                            .filter_map(|c| c.text())
                            .collect(),
                    );
                }
            }
        }
    } else {
        let doc = super::xhtml::parse_document(xml);
        let selector = Selector::parse("nav").unwrap();
        let anchors = Selector::parse("a[href]").unwrap();
        for nav in doc.select(&selector).filter(|n| {
            n.value().attrs().any(|(k, v)| {
                (k == "type" || k == "epub:type" || k == "role")
                    && v.split_whitespace().any(|t| t == "toc" || t == "doc-toc")
            })
        }) {
            for a in nav.select(&anchors) {
                link(a.attr("href").unwrap(), a.text().collect());
            }
        }
    }
    entries
}
