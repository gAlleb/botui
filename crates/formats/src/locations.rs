//! Map only source nodes that reproduce the displayed paragraph exactly.
use botui_core::{Block, Locations, Run};
use roxmltree::{Document, Node};
use std::{collections::HashMap, sync::Arc};
pub fn steps(node: Node<'_, '_>) -> String {
    let mut parts = Vec::new();
    for n in node
        .ancestors()
        .take_while(|n| n.parent_element().is_some())
    {
        parts.push(
            n.prev_siblings().filter(Node::is_element).count() * 2 + usize::from(n.is_text()),
        );
    }
    parts.iter().rev().map(|i| format!("/{i}")).collect()
}
fn build(
    node: Node<'_, '_>,
    expected: &str,
    mut point: impl FnMut(Node<'_, '_>) -> Option<(String, usize)>,
) -> Option<Vec<Run>> {
    let mut text = String::new();
    let mut pending = false;
    let mut runs = Vec::<Run>::new();
    for n in node.descendants().filter(Node::is_text) {
        let raw = n.text()?;
        if raw.trim().is_empty() {
            pending |= !text.is_empty() && !raw.is_empty();
            continue;
        }
        let (p, mut units) = point(n)?;
        let path: Arc<str> = p.into();
        let mut merge = false;
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == ' '
                && merge
                && chars
                    .peek()
                    .is_some_and(|v| !v.is_whitespace() && !v.is_control())
            {
                text.push(c);
                runs.last_mut()?.end = text.len();
            } else if c.is_whitespace() {
                pending = !text.is_empty();
                merge = false;
            } else if !c.is_control() {
                if pending {
                    text.push(' ');
                    pending = false;
                }
                let start = text.len();
                text.push(c);
                if merge {
                    runs.last_mut()?.end = text.len();
                } else {
                    runs.push(Run {
                        byte: start,
                        end: text.len(),
                        utf16: units,
                        path: path.clone(),
                    });
                }
                merge = true;
            } else {
                merge = false;
            }
            units += c.len_utf16();
        }
    }
    (text == expected && !runs.is_empty()).then_some(runs)
}
pub fn epub(html: &str, spine: &str, start: usize, blocks: &[Block], map: &mut Locations) {
    let Ok(dom) = Document::parse_with_options(
        html,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 500_000,
            entity_resolver: None,
        },
    ) else {
        return;
    };
    let Some(body) = dom
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
    else {
        return;
    };
    let nodes: Vec<_> = body
        .descendants()
        .filter(|n| {
            n.is_element()
                && matches!(
                    n.tag_name().name(),
                    "p" | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "li"
                        | "blockquote"
                        | "div"
                        | "pre"
                )
                && !n.ancestors().filter(Node::is_element).any(|a| {
                    a.has_attribute("hidden")
                        || matches!(
                            a.tag_name().name(),
                            "script" | "style" | "template" | "svg" | "noscript"
                        )
                })
                && !n.children().any(|c| {
                    c.is_element()
                        && matches!(
                            c.tag_name().name(),
                            "p" | "div" | "section" | "li" | "h1" | "h2" | "h3"
                        )
                })
        })
        .collect();
    let mut by_text = HashMap::new();
    for n in nodes {
        let t = super::xhtml::clean(
            &n.descendants()
                .filter(Node::is_text)
                .filter_map(|n| n.text())
                .collect::<String>(),
        );
        if !t.is_empty() {
            by_text
                .entry(t)
                .or_insert_with(std::collections::VecDeque::new)
                .push_back(n);
        }
    }
    let mut counts = HashMap::new();
    for b in blocks {
        *counts.entry(&b.text).or_insert(0) += 1;
    }
    // Memoize sibling numbers, so large chapters do not walk every preceding paragraph for each word.
    let mut numbers = HashMap::new();
    for n in dom.descendants().filter(Node::is_element) {
        let mut elems = 0;
        let mut offset = 0;
        for c in n.children() {
            if c.is_element() {
                elems += 1;
                offset = 0;
                numbers.insert(c.id(), (elems * 2, 0));
            } else if c.is_text() {
                numbers.insert(c.id(), (elems * 2 + 1, offset));
                offset += c.text().unwrap_or("").encode_utf16().count();
            }
        }
    }
    // Both parsers walk visible prose in document order. Match repeated paragraphs
    // only when every occurrence has a source, never the first equal string by default.
    by_text.retain(|text, nodes| counts.get(text) == Some(&nodes.len()));
    for (i, b) in blocks.iter().enumerate() {
        if let Some(n) = by_text.get_mut(&b.text).and_then(|ns| ns.pop_front())
            && let Some(runs) = build(n, &b.text, |c| {
                let mut steps = vec![];
                for n in c.ancestors().take_while(|n| n.parent_element().is_some()) {
                    steps.push(numbers.get(&n.id())?.0);
                }
                let mut p = format!("{spine}!");
                for i in steps.iter().rev() {
                    p.push_str(&format!("/{i}"));
                }
                Some((p, numbers.get(&c.id())?.1))
            })
        {
            map.0.insert(start + i, runs);
        }
    }
}
// A deliberately restricted subset of the FB2 → XHTML transform: address ordinary prose,
// headings and inline markup in the first body. Skip poetry/notes rather than guess a CFI.
fn accepts(parent: &str, name: &str) -> bool {
    match parent {
        "body" => matches!(name, "image" | "title" | "epigraph" | "section"),
        "section" | "cite" | "epigraph" => matches!(
            name,
            "title"
                | "epigraph"
                | "image"
                | "annotation"
                | "section"
                | "p"
                | "poem"
                | "subtitle"
                | "cite"
                | "empty-line"
                | "table"
                | "text-author"
        ),
        "title" => matches!(name, "p" | "empty-line"),
        "p" | "subtitle" | "text-author" | "emphasis" | "strong" | "style" | "a"
        | "strikethrough" | "sup" | "sub" | "code" => matches!(
            name,
            "emphasis"
                | "strong"
                | "style"
                | "a"
                | "strikethrough"
                | "sup"
                | "sub"
                | "code"
                | "image"
        ),
        _ => false,
    }
}
pub struct FbPaths(HashMap<roxmltree::NodeId, (String, usize)>);
impl FbPaths {
    pub fn new(body: Node<'_, '_>) -> Self {
        let mut paths = HashMap::new();
        if body.prev_siblings().skip(1).any(|n| n.has_tag_name("body")) {
            return Self(paths);
        }
        let mut stack = Vec::new();
        for (i, n) in body
            .children()
            .filter(|n| n.is_element() && accepts("body", n.tag_name().name()))
            .enumerate()
        {
            stack.push((n, format!("/6/{}!/4/2", 2 * i + 2)));
        }
        while let Some((n, path)) = stack.pop() {
            let tag = n.tag_name().name();
            if !matches!(
                tag,
                "section"
                    | "title"
                    | "p"
                    | "subtitle"
                    | "text-author"
                    | "strong"
                    | "emphasis"
                    | "style"
                    | "a"
                    | "strikethrough"
                    | "sup"
                    | "sub"
                    | "code"
                    | "cite"
                    | "epigraph"
            ) {
                continue;
            }
            let mut element = 0;
            let mut utf16 = 0;
            for c in n.children() {
                if c.is_element() && accepts(tag, c.tag_name().name()) {
                    element += 1;
                    utf16 = 0;
                    stack.push((c, format!("{path}/{}", element * 2)));
                } else if c.is_text() {
                    paths.insert(c.id(), (format!("{path}/{}", element * 2 + 1), utf16));
                    utf16 += c.text().unwrap_or("").encode_utf16().count();
                }
            }
        }
        Self(paths)
    }
    pub fn paragraph(&self, node: Node<'_, '_>, expected: &str) -> Option<Vec<Run>> {
        build(node, expected, |n| self.0.get(&n.id()).cloned())
    }
}
