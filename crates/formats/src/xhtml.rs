use botui_core::Block;
use ego_tree::iter::Edge;
use scraper::{Html, Node};

/// Collapse book whitespace and remove control bytes before text reaches a terminal.
pub(super) fn clean(input: &str) -> String {
    let mut output = String::new();
    let mut space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            space = !output.is_empty();
        } else if !ch.is_control() {
            if space {
                output.push(' ');
                space = false;
            }
            output.push(ch);
        }
    }
    output
}

fn heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

fn boundary(name: &str) -> bool {
    heading(name)
        || matches!(
            name,
            "p" | "div"
                | "section"
                | "article"
                | "blockquote"
                | "li"
                | "br"
                | "tr"
                | "pre"
                | "hr"
                | "figcaption"
                | "dt"
                | "dd"
        )
}

fn flush(buffer: &mut String, is_heading: bool, blocks: &mut Vec<Block>) {
    let text = clean(buffer);
    if !text.is_empty() {
        blocks.push(Block {
            text,
            heading: is_heading,
        });
    }
    buffer.clear();
}

pub(super) struct RawLink {
    pub block: usize,
    pub image: bool,
    pub label: String,
    pub href: String,
}
pub(super) fn append_blocks(
    html: &str,
    blocks: &mut Vec<Block>,
    links: &mut Vec<RawLink>,
) -> Vec<(String, usize)> {
    let dom = Html::parse_document(html);
    let Some(body) = dom
        .root_element()
        .child_elements()
        .find(|e| e.value().name() == "body")
    else {
        return vec![];
    };
    let mut ids = Vec::new();
    let mut buffer = String::new();
    let mut is_heading = false;
    let mut hidden = None;
    for edge in body.traverse() {
        match edge {
            Edge::Open(node) if hidden.is_none() => match node.value() {
                Node::Element(el) => {
                    let name = el.name();
                    if let Some(id) = el.attr("id").or_else(|| el.attr("name")) {
                        let next =
                            blocks.len() + usize::from(boundary(name) && !buffer.trim().is_empty());
                        ids.push((id.to_owned(), next));
                    }
                    if matches!(name, "script" | "style" | "template" | "svg" | "noscript")
                        || el.attr("hidden").is_some()
                    {
                        hidden = Some(node.id());
                    } else if matches!(name, "a" | "img") {
                        let image = name == "img";
                        if let Some(href) = el
                            .attr(if image { "src" } else { "href" })
                            .filter(|v| !v.trim().is_empty())
                        {
                            let label = if image {
                                el.attr("alt")
                                    .or_else(|| el.attr("title"))
                                    .unwrap_or("")
                                    .to_owned()
                            } else {
                                scraper::ElementRef::wrap(node)
                                    .map(|e| clean(&e.text().collect::<String>()))
                                    .unwrap_or_default()
                            };
                            links.push(RawLink {
                                block: blocks.len(),
                                image,
                                label,
                                href: href.to_owned(),
                            });
                        }
                    } else if boundary(name) {
                        flush(&mut buffer, is_heading, blocks);
                        if heading(name) {
                            is_heading = true;
                        }
                    } else if matches!(name, "td" | "th") {
                        buffer.push(' ');
                    }
                }
                Node::Text(text) => buffer.push_str(text),
                _ => {}
            },
            Edge::Close(node) => {
                if hidden == Some(node.id()) {
                    hidden = None;
                } else if hidden.is_none()
                    && let Some(el) = node.value().as_element()
                    && boundary(el.name())
                {
                    flush(&mut buffer, is_heading, blocks);
                    if heading(el.name()) {
                        is_heading = false;
                    }
                }
            }
            _ => {}
        }
    }
    flush(&mut buffer, is_heading, blocks);
    ids
}
