use botui_core::Block;
use ego_tree::iter::Edge;
use scraper::{Html, Node};
use xmlparser::{ElementEnd, Token, Tokenizer};

/// HTML ignores the slash in a non-void tag like `<title/>` and can consume
/// the rest of the chapter as title text. Expand XML empty elements for display;
/// CFI mapping still reads the original XHTML.
pub(super) fn parse_document(source: &str) -> Html {
    if !source.contains("/>") {
        return Html::parse_document(source);
    }
    let mut expanded = String::new();
    let mut copied = 0;
    let mut name = "";
    for token in Tokenizer::from(source).map_while(Result::ok) {
        match token {
            Token::ElementStart { span, .. } => name = &source[span.start() + 1..span.end()],
            Token::ElementEnd {
                end: ElementEnd::Empty,
                span,
            } => {
                // An explicit HTML </br> would introduce a second line break.
                if matches!(
                    name,
                    "area"
                        | "base"
                        | "br"
                        | "col"
                        | "embed"
                        | "hr"
                        | "img"
                        | "input"
                        | "link"
                        | "meta"
                        | "param"
                        | "source"
                        | "track"
                        | "wbr"
                ) {
                    continue;
                }
                expanded.push_str(&source[copied..span.start()]);
                expanded.push_str("></");
                expanded.push_str(name);
                expanded.push('>');
                copied = span.end();
            }
            _ => {}
        }
    }
    if copied == 0 {
        return Html::parse_document(source);
    }
    expanded.push_str(&source[copied..]);
    Html::parse_document(&expanded)
}

/// Collapse book whitespace and remove control bytes before text reaches a terminal.
pub(super) fn clean(input: &str) -> String {
    let mut out = String::new();
    append_text(&mut out, input);
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

// Keep one pending ASCII space. Its byte position is also the beginning of
// the next inline run, so extracting thousands of offsets stays linear.
fn append_text(out: &mut String, text: &str) {
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
        } else if !ch.is_control() {
            out.push(ch);
        }
    }
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
    if buffer.ends_with(' ') {
        buffer.pop();
    }
    if !buffer.is_empty() {
        blocks.push(Block {
            text: std::mem::take(buffer),
            heading: is_heading,
        });
    }
}

pub(super) struct RawLink {
    pub block: usize,
    pub byte: usize,
    pub backlink: bool,
    pub id: Option<String>,
    pub image: bool,
    pub label: String,
    pub href: String,
}
pub(super) fn append_blocks(
    dom: &Html,
    blocks: &mut Vec<Block>,
    links: &mut Vec<RawLink>,
    preformatted: &mut std::collections::BTreeSet<usize>,
    tables: &mut std::collections::BTreeMap<usize, botui_core::TableRow>,
) -> Vec<(String, botui_core::Anchor)> {
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
    let mut occupied = Vec::new();
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
                                    byte: 0,
                                    backlink: false,
                                    id: None,
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
                        let (labels, spans) = place_cells(&cells, &mut occupied);
                        table_start = Some((blocks.len(), header, labels, spans));
                    }
                    if el.attr("hidden").is_none()
                        && !matches!(name, "script" | "style" | "template" | "noscript")
                        && let Some(id) = el.attr("id").or_else(|| el.attr("name"))
                    {
                        let next =
                            blocks.len() + usize::from(boundary(name) && !buffer.trim().is_empty());
                        ids.push((
                            id.to_owned(),
                            botui_core::Anchor {
                                block: next,
                                byte: if boundary(name) { 0 } else { buffer.len() },
                            },
                        ));
                    }
                    if matches!(name, "script" | "style" | "template" | "svg" | "noscript")
                        || el.attr("hidden").is_some()
                    {
                        hidden = Some(node.id());
                    } else if name == "pre" {
                        flush(&mut buffer, is_heading, blocks);
                        let element = scraper::ElementRef::wrap(node).unwrap();
                        let mut text = String::new();
                        for edge in element.traverse() {
                            if let Edge::Open(n) = edge {
                                match n.value() {
                                    Node::Element(el) if el.name() == "br" => text.push('\n'),
                                    Node::Text(run) => {
                                        for ch in run.chars() {
                                            match ch {
                                                '\t' => text.push_str("    "),
                                                '\n' => text.push(ch),
                                                ch if !ch.is_control() => text.push(ch),
                                                _ => {}
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        preformatted.insert(blocks.len());
                        blocks.push(Block {
                            text,
                            heading: false,
                        });
                        hidden = Some(node.id());
                    } else if name == "math" {
                        if let Some(element) = scraper::ElementRef::wrap(node) {
                            append_text(&mut buffer, &math_text(element, 0));
                        }
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
                                byte: buffer.len(),
                                id: el.attr("id").or_else(|| el.attr("name")).map(str::to_owned),
                                backlink: ["epub:type", "role"]
                                    .iter()
                                    .filter_map(|attr| el.attr(attr))
                                    .flat_map(str::split_ascii_whitespace)
                                    .any(|v| matches!(v, "backlink" | "doc-backlink")),
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
                Node::Text(text) => append_text(&mut buffer, text),
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
                    occupied.clear();
                } else if hidden.is_none()
                    && let Some(el) = node.value().as_element()
                    && boundary(el.name())
                {
                    flush(&mut buffer, is_heading, blocks);
                    if el.name() == "tr"
                        && let Some((start, header, cells, spans)) = table_start.take()
                        && start < blocks.len()
                        && !cells.is_empty()
                    {
                        columns = columns.max(spans.iter().sum());
                        table_rows.push(start);
                        tables.insert(
                            start,
                            botui_core::TableRow {
                                end: blocks.len(),
                                header,
                                cells,
                                spans,
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

/// Keep a logical column occupied until the final row of a vertical span.
fn place_cells(
    cells: &[scraper::ElementRef<'_>],
    occupied: &mut Vec<usize>,
) -> (Vec<String>, Vec<usize>) {
    let mut labels = Vec::new();
    let mut spans = Vec::new();
    let mut column = 0;
    for cell in cells {
        let span = |key| {
            cell.attr(key)
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 256)
        };
        let width = span("colspan");
        while occupied
            .get(column..)
            .unwrap_or_default()
            .iter()
            .take(width)
            .any(|&v| v > 0)
        {
            labels.push(String::new());
            spans.push(1);
            column += 1;
        }
        let end = column + width;
        // Wide tables already use the stacked layout. Bound the occupancy map
        // even if an input has thousands of cells with oversized spans.
        if end <= 256 {
            occupied.resize(occupied.len().max(end), 0);
            occupied[column..end].fill(span("rowspan"));
        }
        labels.push(clean(&cell.text().collect::<Vec<_>>().join(" ")));
        spans.push(width);
        column = end;
    }
    for value in occupied {
        *value = value.saturating_sub(1);
    }
    (labels, spans)
}

fn math_text(element: scraper::ElementRef<'_>, depth: usize) -> String {
    if depth >= 64 {
        return "…".into();
    }
    let parts: Vec<_> = element
        .child_elements()
        .filter(|child| !matches!(child.value().name(), "annotation" | "annotation-xml"))
        .map(|child| math_text(child, depth + 1))
        .collect();
    match (element.value().name(), parts.as_slice()) {
        ("mfrac", [a, b]) => format!("({a})/({b})"),
        ("msqrt", args) => format!("√({})", args.join("")),
        ("mroot", [a, b]) => format!("root({b}, {a})"),
        ("msup", [a, b]) => format!("{a}^({b})"),
        ("msub", [a, b]) => format!("{a}_({b})"),
        ("msubsup", [a, b, c]) => format!("{a}_({b})^({c})"),
        ("mfenced", args) => format!(
            "{}{}{}",
            element.attr("open").unwrap_or("("),
            args.join(", "),
            element.attr("close").unwrap_or(")")
        ),
        ("mtr", args) => args.join(", "),
        ("mtable", args) => format!("[{}]", args.join(" ; ")),
        (_, []) => clean(&element.text().collect::<String>()),
        (_, args) => args.join(""),
    }
}
