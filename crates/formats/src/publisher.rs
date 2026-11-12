//! Preserve the connection between CSS boxes and the original EPUB text.
//! Only the private layout DOM is annotated. CFIs address the unmodified XHTML.
use crate::xhtml::TextOrigins;
use botui_core::{
    Block, Locations, Run,
    publisher::{Chapter, SourceText, Stylesheets},
};
use scraper::{ElementRef, Html};
use std::sync::Arc;

pub(super) fn uses_layout(dom: &Html) -> bool {
    dom.tree.nodes().filter_map(ElementRef::wrap).any(|e| {
        let el = e.value();
        super::tex::marked(el)
            || matches!(
                el.name(),
                "table"
                    | "math"
                    | "pre"
                    | "style"
                    | "ol"
                    | "ul"
                    | "blockquote"
                    | "strong"
                    | "em"
                    | "b"
                    | "i"
                    | "code"
                    | "img"
                    | "svg"
                    | "object"
            )
            || el.attr("style").is_some()
            || (el.name() == "link"
                && el.attr("rel").is_some_and(|r| {
                    r.split_ascii_whitespace()
                        .any(|s| s.eq_ignore_ascii_case("stylesheet"))
                }))
    })
}

pub(super) fn chapter(
    dom: &mut Html,
    origins: TextOrigins,
    end: usize,
    member: &str,
    sheets: Arc<Stylesheets>,
    figures: impl Iterator<Item = (ego_tree::NodeId, usize, Option<(usize, usize)>)>,
) -> Chapter {
    let mut elements = Vec::new();
    let mut sources = Vec::new();
    for el in dom.tree.nodes().filter_map(ElementRef::wrap) {
        let spans: Vec<_> = el
            .children()
            .enumerate()
            .filter_map(|(index, child)| {
                origins.get(&child.id()).map(|&(start, end)| SourceText {
                    display: None,
                    start,
                    end,
                    child: index,
                })
            })
            .collect();
        if !spans.is_empty() {
            elements.push(el.id());
            sources.push(spans);
        }
    }
    // Replace any book-supplied markers, including on elements without text.
    for node in dom.tree.values_mut() {
        if let scraper::Node::Element(el) = node {
            el.attrs.retain(|(n, _)| {
                !matches!(
                    &*n.local,
                    "data-botui-text" | "data-botui-image" | "data-botui-equation"
                )
            });
        }
    }
    for (i, id) in elements.into_iter().enumerate() {
        if let scraper::Node::Element(el) = dom.tree.get_mut(id).unwrap().value() {
            set_attr(el, "data-botui-text", &i.to_string());
        }
    }
    let mut images = Vec::new();
    for (node, raw, size) in figures {
        let mut node = dom.tree.get_mut(node).unwrap();
        let scraper::Node::Element(el) = node.value() else {
            continue;
        };
        if el.attr("data-botui-image").is_some() {
            continue;
        }
        set_attr(el, "data-botui-image", &images.len().to_string());
        if let Some((w, h)) = size.filter(|(w, h)| *w > 0 && *h > 0) {
            let style = format!(
                "--botui-intrinsic:{}px;--botui-ratio:{};{}",
                w,
                w as f64 / h as f64,
                el.attr("style").unwrap_or("")
            );
            set_attr(el, "style", &style);
        }
        images.push(Some(raw));
        // Vector labels belong to the illustration. Never feed them to the text
        // layout a second time. Keep the root tag so publisher svg/object CSS
        // selectors and dimensions continue to match.
        if matches!(el.name(), "svg" | "object") {
            while let Some(mut child) = node.first_child() {
                child.detach();
            }
        }
    }
    normalize_spans(dom);
    let preserve_structure = dom.tree.nodes().filter_map(ElementRef::wrap).any(|e| {
        matches!(
            e.value().name(),
            "math" | "table" | "pre" | "ol" | "ul" | "img" | "svg"
        ) || super::tex::marked(e.value())
    });
    super::tex::convert(dom);
    // scraper serializes HTML void elements such as <img> and <br> without
    // XML closing tags. Select the HTML parser even when the root declares XHTML.
    let html = format!("<!doctype html>\n{}", dom.root_element().html());
    let mut chapter = Chapter::new(end, html, member, sheets, sources);
    chapter.images = images;
    chapter.preserve_structure = preserve_structure;
    chapter
}

pub(super) fn set_attr(el: &mut scraper::node::Element, name: &str, value: &str) {
    let mut key = el.name.clone();
    key.local = name.into();
    key.ns = Default::default();
    key.prefix = None;
    match el.attrs.binary_search_by(|(k, _)| k.cmp(&key)) {
        Ok(i) => el.attrs[i].1 = value.into(),
        Err(i) => el.attrs.insert(i, (key, value.into())),
    }
}

// HTML rowspan=0 extends to the end of this row group. Give the grid engine
// its concrete size and apply HTML's colspan limit before track allocation.
fn normalize_spans(dom: &mut Html) {
    let mut edits = Vec::new();
    for group in dom
        .tree
        .nodes()
        .filter_map(ElementRef::wrap)
        .filter(|e| matches!(e.value().name(), "table" | "tbody" | "thead" | "tfoot"))
    {
        for (below, row) in group
            .child_elements()
            .filter(|e| e.value().name() == "tr")
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .enumerate()
        {
            for cell in row
                .child_elements()
                .filter(|e| matches!(e.value().name(), "td" | "th"))
            {
                for (name, max) in [("rowspan", below + 1), ("colspan", 1000)] {
                    if let Some(s) = cell.attr(name)
                        && let Ok(n) = s.parse::<u64>()
                    {
                        let value = if n == 0 && name == "rowspan" {
                            max as u64
                        } else {
                            n.clamp(1, max as u64)
                        };
                        if n != value {
                            edits.push((cell.id(), name, value));
                        }
                    }
                }
            }
        }
    }
    for (id, key, value) in edits {
        if let scraper::Node::Element(el) = dom.tree.get_mut(id).unwrap().value() {
            set_attr(el, key, &value.to_string());
        }
    }
}

/// Map table cells and individual MathML tokens, including text split by inline tags.
/// If HTML repair changed the text-node order, the normal prose mapper is still available.
pub(super) fn map_locations(
    html: &str,
    dom: &Html,
    origins: &TextOrigins,
    spine: &str,
    blocks: &[Block],
    map: &mut Locations,
) {
    let Ok(xml) = roxmltree::Document::parse_with_options(
        html,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 500_000,
            entity_resolver: None,
        },
    ) else {
        return;
    };
    let Some(body) = xml
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
    else {
        return;
    };
    let Some(html_body) = dom
        .root_element()
        .child_elements()
        .find(|n| n.value().name() == "body")
    else {
        return;
    };
    let source: Vec<_> = body
        .descendants()
        .filter(|n| n.is_text() && n.text().is_some_and(|s| !s.trim().is_empty()))
        .collect();
    let display: Vec<_> = html_body
        .descendants()
        .filter_map(|n| Some((n.id(), n.value().as_text()?)))
        .filter(|(_, t)| !t.trim().is_empty())
        .collect();
    if source.len() != display.len()
        || !source
            .iter()
            .zip(&display)
            .all(|(n, (_, t))| n.text().unwrap_or("").trim() == t.trim())
    {
        return;
    }
    let numbers = super::locations::sibling_steps(&xml);
    for (node, (id, _)) in source.into_iter().zip(display) {
        let Some(&(at, end)) = origins.get(&id) else {
            continue;
        };
        let Some(block) = blocks.get(at.block) else {
            continue;
        };
        let end = end.min(block.text.len());
        let Some(rendered) = block.text.get(at.byte..end) else {
            continue;
        };
        let Some((path, mut utf16)) = super::locations::text_path(node, spine, &numbers) else {
            continue;
        };
        let path: Arc<str> = path.into();
        let runs = map.0.entry(at.block).or_default();
        let mut bytes = rendered.char_indices().peekable();
        let mut contiguous = false;
        for c in node.text().unwrap_or("").chars() {
            if let Some(&(byte, next)) = bytes.peek()
                && (next == c || next == ' ' && c.is_whitespace())
            {
                let byte = at.byte + byte;
                // Run offsets are linear in UTF-16 only inside consecutive unmodified text.
                if contiguous && let Some(last) = runs.last_mut() {
                    last.end = byte + next.len_utf8();
                } else {
                    runs.push(Run {
                        byte,
                        end: byte + next.len_utf8(),
                        utf16,
                        path: path.clone(),
                    });
                }
                bytes.next();
                contiguous = next == c;
            } else {
                contiguous = false;
            }
            utf16 += c.len_utf16();
        }
    }
}
