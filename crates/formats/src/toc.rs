//! Read optional EPUB 3 navigation or EPUB 2 NCX, keeping spine fragment targets.
use super::{epub::member_path, xhtml::clean};
use botui_core::Anchor;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};

pub fn read(
    path: &str,
    xml: &str,
    is_ncx: bool,
    starts: &HashMap<String, usize>,
    ids: &HashMap<(String, String), usize>,
    count: usize,
) -> Vec<(Anchor, String)> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut link = |href: &str, label: String| {
        let Ok(file) = member_path(path, href) else {
            return;
        };
        let block = match href.split_once('#') {
            Some((_, frag)) if !frag.is_empty() => percent_encoding::percent_decode_str(frag)
                .decode_utf8()
                .ok()
                .and_then(|id| ids.get(&(file, id.into_owned())).copied()),
            _ => starts.get(&file).copied(),
        };
        let title = clean(&label);
        if let Some(block) = block
            && block < count
            && !title.is_empty()
            && seen.insert((block, title.clone()))
        {
            entries.push((Anchor { block, byte: 0 }, title));
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
        let doc = Html::parse_document(xml);
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
