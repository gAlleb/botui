//! Read optional EPUB 3 navigation or EPUB 2 NCX, keeping spine fragment targets.
use super::{epub::member_path, xhtml::clean};
use botui_core::{Anchor, ContentsEntry, Target};
use scraper::Selector;
use std::collections::{HashMap, HashSet};

pub fn read(
    path: &str,
    xml: &str,
    is_ncx: bool,
    starts: &HashMap<String, usize>,
    ids: &HashMap<(String, String), Anchor>,
    resources: &HashMap<String, &str>,
    count: usize,
) -> Vec<ContentsEntry> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut link = |href: &str, label: String, parent: Option<usize>| {
        let (reference, fragment) = href.split_once('#').unwrap_or((href, ""));
        let file = if reference.is_empty() && !fragment.is_empty() {
            path.to_owned()
        } else if let Ok(file) = member_path(path, reference) {
            file
        } else {
            return None;
        };
        let fragment = percent_encoding::percent_decode_str(fragment)
            .decode_utf8()
            .ok()?
            .into_owned();
        let title = clean(&label);
        if title.is_empty() {
            return None;
        }
        let target = if let Some(&start) = starts.get(&file) {
            let anchor = if fragment.is_empty() {
                Anchor {
                    block: start,
                    byte: 0,
                }
            } else {
                *ids.get(&(file.clone(), fragment.clone()))?
            };
            if anchor.block >= count {
                return None;
            }
            Target::Passage(anchor)
        } else if matches!(
            resources.get(&file),
            Some(&"application/xhtml+xml" | &"text/html")
        ) {
            Target::Supplement {
                member: file.clone(),
                fragment: fragment.clone(),
                origin: None,
            }
        } else {
            return None;
        };
        if !seen.insert((parent, file, fragment, title.clone())) {
            return None;
        }
        let index = entries.len();
        entries.push(ContentsEntry {
            target,
            title,
            parent,
        });
        Some(index)
    };
    if is_ncx {
        if let Ok(doc) = roxmltree::Document::parse_with_options(
            xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        ) {
            let mut parents = HashMap::new();
            for n in doc.descendants().filter(|n| n.has_tag_name("navPoint")) {
                let src = n
                    .children()
                    .find(|c| c.has_tag_name("content"))
                    .and_then(|c| c.attribute("src"));
                let label = n.children().find(|c| c.has_tag_name("navLabel"));
                if let (Some(h), Some(l)) = (src, label) {
                    let parent = n
                        .ancestors()
                        .skip(1)
                        .find_map(|a| parents.get(&a.id()).copied());
                    if let Some(index) = link(
                        h,
                        l.descendants()
                            .filter(|c| c.is_text())
                            .filter_map(|c| c.text())
                            .collect(),
                        parent,
                    ) {
                        parents.insert(n.id(), index);
                    }
                }
            }
        }
    } else {
        let doc = super::xhtml::parse_document(xml);
        let selector = Selector::parse("nav").unwrap();
        let labels = Selector::parse("a[href], li > span").unwrap();
        let anchors = Selector::parse("a[href]").unwrap();
        for nav in doc.select(&selector).filter(|n| {
            n.value().attrs().any(|(k, v)| {
                (k == "type" || k == "epub:type" || k == "role")
                    && v.split_whitespace().any(|t| t == "toc" || t == "doc-toc")
            })
        }) {
            let mut parents = HashMap::new();
            for label in nav.select(&labels) {
                let owner = label
                    .ancestors()
                    .filter_map(scraper::ElementRef::wrap)
                    .take_while(|e| e.id() != nav.id())
                    .find(|e| e.value().name() == "li");
                let parent =
                    owner.and_then(|e| e.ancestors().find_map(|a| parents.get(&a.id()).copied()));
                // A non-link group label opens the first destination in its list.
                let href = label
                    .attr("href")
                    .or_else(|| owner?.select(&anchors).next()?.attr("href"));
                if let Some(index) = href.and_then(|h| link(h, label.text().collect(), parent))
                    && let Some(e) = owner
                {
                    parents.insert(e.id(), index);
                }
            }
        }
    }
    entries
}
