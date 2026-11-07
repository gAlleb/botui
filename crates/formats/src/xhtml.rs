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
    tables: &mut std::collections::BTreeMap<usize, botui_core::TableRow>,
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
    let mut table_start = None;
    let mut table_rows = vec![];
    let mut columns = 0;
    for edge in body.traverse() {
        match edge {
            Edge::Open(node) if hidden.is_none() => match node.value() {
                Node::Element(el) => {
                    let name = el.name();
                    if name == "svg" && el.attr("hidden").is_none() {
                        for n in node
                            .descendants()
                            .filter_map(scraper::ElementRef::wrap)
                            .filter(|n| n.value().name() == "image")
                        {
                            if let Some((_, href)) = n
                                .value()
                                .attrs()
                                .find(|(key, _)| matches!(*key, "href" | "xlink:href"))
                            {
                                links.push(RawLink {
                                    block: blocks.len(),
                                    image: true,
                                    label: String::new(),
                                    href: href.into(),
                                });
                            }
                        }
                    }
                    if name == "tr" && el.attr("hidden").is_none() {
                        flush(&mut buffer, is_heading, blocks);
                        let cells: Vec<_> = scraper::ElementRef::wrap(node)
                            .into_iter()
                            .flat_map(|e| e.child_elements())
                            .filter(|c| matches!(c.value().name(), "td" | "th"))
                            .collect();
                        let header = cells.iter().any(|c| c.value().name() == "th");
                        table_start = Some((
                            blocks.len(),
                            header,
                            cells
                                .iter()
                                .map(|c| clean(&c.text().collect::<Vec<_>>().join(" ")))
                                .collect::<Vec<_>>(),
                        ));
                    }
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
                    && node
                        .value()
                        .as_element()
                        .is_some_and(|el| el.name() == "table")
                {
                    for start in table_rows.drain(..) {
                        if let Some(row) = tables.get_mut(&start) {
                            row.columns = columns;
                        }
                    }
                    columns = 0;
                } else if hidden.is_none()
                    && let Some(el) = node.value().as_element()
                    && boundary(el.name())
                {
                    flush(&mut buffer, is_heading, blocks);
                    if el.name() == "tr"
                        && let Some((start, header, cells)) = table_start.take()
                        && start < blocks.len()
                        && !cells.is_empty()
                    {
                        columns = columns.max(cells.len());
                        table_rows.push(start);
                        tables.insert(
                            start,
                            botui_core::TableRow {
                                end: blocks.len(),
                                header,
                                cells,
                                columns: 0,
                            },
                        );
                    }
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
