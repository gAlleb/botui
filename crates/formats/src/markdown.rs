//! CommonMark reuses the ebook layout engine; both views address the original bytes.
use super::{Result, invalid};
use botui_core::{
    Anchor, Block, ContentsEntry, Document, Markdown, Reference, Target,
    publisher::{Chapter, SourceText, Stylesheets},
};
use pulldown_cmark::{BlockQuoteKind, CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};
use std::{fs::File, io::Read, ops::Range, path::Path, sync::Arc};

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub(super) fn open(path: &Path) -> Result<Document> {
    let mut raw = String::new();
    File::open(path)?
        .take((4 << 20) + 1)
        .read_to_string(&mut raw)?;
    if raw.len() > 4 << 20 {
        return Err(invalid("Markdown exceeds 4 MiB text limit").into());
    }
    Ok(parse(
        raw,
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Markdown"),
    ))
}

fn marked(
    text: &str,
    range: Range<usize>,
    raw: &str,
    sources: &mut Vec<Vec<SourceText>>,
    passages: &mut Vec<SourceText>,
    syntax: u8,
) -> String {
    let source = SourceText {
        start: Anchor {
            block: 0,
            byte: range.start,
        },
        end: range.end,
        child: 0,
        display: (raw[range.clone()] != *text).then(|| text.to_owned()),
    };
    let id = sources.len();
    sources.push(vec![source.clone()]);
    passages.push(source);
    format!(
        "<span data-botui-text='{id}' data-botui-syntax='{syntax}'>{}</span>",
        escape(text)
    )
}

// Inline code replaces line endings with spaces and may trim one padding space.
// Map those normalized characters individually so selections remain exact.
fn inline_code(
    text: &str,
    range: Range<usize>,
    raw: &str,
    sources: &mut Vec<Vec<SourceText>>,
    passages: &mut Vec<SourceText>,
) -> String {
    if let Some(at) = raw[range.clone()].find(text) {
        return marked(
            text,
            range.start + at..range.start + at + text.len(),
            raw,
            sources,
            passages,
            2,
        );
    }
    let ticks = raw[range.clone()]
        .bytes()
        .take_while(|b| *b == b'`')
        .count();
    let mut at = range.start + ticks;
    let end = range.end.saturating_sub(ticks);
    if raw
        .get(at..end)
        .is_some_and(|s| s.starts_with([' ', '\n', '\r']) && s.ends_with([' ', '\n', '\r']))
        && !text.chars().all(|c| c == ' ')
    {
        at += if raw[at..end].starts_with("\r\n") {
            2
        } else {
            1
        };
    }
    let mut out = String::new();
    for c in text.chars() {
        let len = if raw[at..end].starts_with("\r\n") {
            2
        } else {
            raw[at..end].chars().next().map_or(0, char::len_utf8)
        };
        if len == 0 {
            break;
        }
        out.push_str(&marked(
            &c.to_string(),
            at..at + len,
            raw,
            sources,
            passages,
            2,
        ));
        at += len;
    }
    out
}

// A small lexical highlighter: no compiled grammar bundle or runtime installation.
fn code(
    text: &str,
    range: Range<usize>,
    raw: &str,
    sources: &mut Vec<Vec<SourceText>>,
    passages: &mut Vec<SourceText>,
    language: &str,
) -> String {
    let range = raw[range.clone()].find(text).map_or(range.clone(), |at| {
        range.start + at..range.start + at + text.len()
    });
    if matches!(language, "md" | "markdown")
        && text.len() <= 65536
        && raw.get(range.clone()) == Some(text)
    {
        return markdown_code(text, range.start, raw, sources, passages);
    }
    let known = matches!(
        language,
        "rs" | "rust"
            | "py"
            | "python"
            | "js"
            | "javascript"
            | "ts"
            | "typescript"
            | "json"
            | "toml"
            | "sh"
            | "bash"
            | "c"
            | "cpp"
            | "go"
            | "java"
            | "yaml"
            | "yml"
            | "sql"
    );
    if !known || text.len() > 65536 || raw.get(range.clone()) != Some(text) {
        return marked(text, range, raw, sources, passages, 0);
    }
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let c = rest.chars().next().unwrap();
        let (len, tone) = if rest.starts_with("//")
            || (c == '#'
                && matches!(
                    language,
                    "py" | "python" | "sh" | "bash" | "yaml" | "yml" | "toml"
                ))
            || (rest.starts_with("--") && language == "sql")
        {
            (rest.find('\n').unwrap_or(rest.len()), 3)
        } else if c == '"' || c == '\'' || c == '`' {
            let mut escaped = false;
            let mut end = rest.len();
            for (i, ch) in rest.char_indices().skip(1) {
                if ch == c && !escaped {
                    end = i + ch.len_utf8();
                    break;
                }
                escaped = ch == '\\' && !escaped;
            }
            (end, 2)
        } else if c.is_alphanumeric() || c == '_' {
            let len = rest
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            let word = &rest[..len];
            let tone = if c.is_ascii_digit() {
                4
            } else if matches!(
                word,
                "fn" | "let"
                    | "mut"
                    | "pub"
                    | "use"
                    | "impl"
                    | "struct"
                    | "enum"
                    | "match"
                    | "self"
                    | "Self"
                    | "const"
                    | "static"
                    | "async"
                    | "await"
                    | "def"
                    | "class"
                    | "import"
                    | "from"
                    | "as"
                    | "if"
                    | "else"
                    | "elif"
                    | "for"
                    | "while"
                    | "in"
                    | "return"
                    | "yield"
                    | "true"
                    | "false"
                    | "True"
                    | "False"
                    | "None"
                    | "null"
                    | "function"
                    | "var"
                    | "new"
                    | "export"
                    | "default"
                    | "interface"
                    | "type"
                    | "switch"
                    | "case"
                    | "break"
                    | "continue"
                    | "try"
                    | "catch"
                    | "throw"
                    | "package"
                    | "func"
                    | "SELECT"
                    | "FROM"
                    | "WHERE"
            ) {
                1
            } else {
                0
            };
            (len, tone)
        } else if c.is_whitespace() {
            (
                rest.find(|c: char| !c.is_whitespace())
                    .unwrap_or(rest.len()),
                0,
            )
        } else {
            (c.len_utf8(), 0)
        };
        out.push_str(&marked(
            &text[at..at + len],
            range.start + at..range.start + at + len,
            raw,
            sources,
            passages,
            tone,
        ));
        at += len;
    }
    out
}
// Highlight Markdown examples and source without changing any source bytes.
fn markdown_code(
    text: &str,
    start: usize,
    raw: &str,
    sources: &mut Vec<Vec<SourceText>>,
    passages: &mut Vec<SourceText>,
) -> String {
    let mut out = String::new();
    let mut at = start;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let tone = if trimmed.starts_with('#')
            || (!trimmed.trim().is_empty() && trimmed.trim().chars().all(|c| c == '=' || c == '-'))
        {
            1
        } else if trimmed.starts_with('>')
            || trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
        {
            3
        } else {
            0
        };
        if tone != 0 {
            out.push_str(&marked(
                line,
                at..at + line.len(),
                raw,
                sources,
                passages,
                tone,
            ));
        } else {
            // Delimiters, inline code and URLs remain visible in a code sample.
            let mut offset = 0;
            while offset < line.len() {
                let rest = &line[offset..];
                let c = rest.chars().next().unwrap();
                let (len, tone) = if c == '`' {
                    (rest[1..].find('`').map_or(rest.len(), |i| i + 2), 2)
                } else if "*_~[]()!|".contains(c) {
                    (c.len_utf8(), 1)
                } else {
                    (
                        rest.char_indices()
                            .skip(1)
                            .find(|(_, c)| "`*_~[]()!|".contains(*c))
                            .map_or(rest.len(), |(i, _)| i),
                        0,
                    )
                };
                out.push_str(&marked(
                    &rest[..len],
                    at + offset..at + offset + len,
                    raw,
                    sources,
                    passages,
                    tone,
                ));
                offset += len;
            }
        }
        at += line.len();
    }
    out
}

fn parse(raw: String, fallback: &str) -> Document {
    let mut doc = Document::default();
    let mut sources = Vec::new();
    let mut passages = Vec::new();
    let mut events = Vec::new();
    let mut images = Vec::new();
    let mut language = None;
    let mut heading: Option<(usize, usize, String)> = None;
    let mut parents: Vec<(usize, usize)> = Vec::new();
    let mut links: Vec<(usize, String)> = Vec::new();
    let mut paragraphs = Vec::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    for (event, range) in Parser::new_ext(&raw, options).into_offset_iter() {
        if matches!(
            &event,
            Event::Start(
                Tag::Paragraph
                    | Tag::Heading { .. }
                    | Tag::Item
                    | Tag::CodeBlock(_)
                    | Tag::TableCell
            )
        ) {
            paragraphs.push(range.clone());
        }
        match event {
            Event::Start(Tag::List(start)) => {
                lists.push(start);
                events.push(Event::Start(Tag::List(start)));
            }
            Event::End(TagEnd::List(ordered)) => {
                lists.pop();
                events.push(Event::End(TagEnd::List(ordered)));
            }
            Event::Start(Tag::Item) => {
                let marker = match lists.last_mut() {
                    Some(Some(number)) => {
                        let label = format!("{number}.");
                        *number = number.saturating_add(1);
                        label
                    }
                    _ => "•".into(),
                };
                // Task boxes replace the bullet; don't show two competing markers.
                let task = raw[range.clone()]
                    .lines()
                    .next()
                    .and_then(|line| line.trim_start().split_once(char::is_whitespace))
                    .is_some_and(|(_, text)| {
                        ["[ ]", "[x]", "[X]"].iter().any(|prefix| {
                            text.trim_start().strip_prefix(prefix).is_some_and(|rest| {
                                rest.is_empty() || rest.starts_with(char::is_whitespace)
                            })
                        })
                    });
                events.push(Event::InlineHtml(
                    format!(
                        "<li data-botui-marker='{}'>",
                        if task { "" } else { &marker }
                    )
                    .into(),
                ));
            }
            Event::Start(Tag::BlockQuote(Some(kind))) => {
                let (label, role) = match kind {
                    BlockQuoteKind::Note => ("Note", 12),
                    BlockQuoteKind::Tip => ("Tip", 13),
                    BlockQuoteKind::Important => ("Important", 14),
                    BlockQuoteKind::Warning => ("Warning", 15),
                    BlockQuoteKind::Caution => ("Caution", 16),
                };
                events.push(Event::InlineHtml(format!("<blockquote data-botui-role='{role}' data-botui-surface='{role}'><p class='alert-title'>{label}</p>").into()));
            }
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((level as usize, range.start, String::new()));
                events.push(Event::Start(Tag::Heading {
                    level,
                    id: None,
                    classes: vec![],
                    attrs: vec![
                        (
                            "data-botui-role".into(),
                            Some((level as usize).to_string().into()),
                        ),
                        (
                            "data-botui-surface".into(),
                            Some((level as usize).to_string().into()),
                        ),
                    ],
                }));
            }
            Event::End(TagEnd::Heading(level)) => {
                if let Some((depth, byte, title)) = heading.take() {
                    while parents.last().is_some_and(|p| p.0 >= depth) {
                        parents.pop();
                    }
                    let mut entry = ContentsEntry::passage(Anchor { block: 0, byte }, title);
                    entry.parent = parents.last().map(|p| p.1);
                    parents.push((depth, doc.contents.len()));
                    doc.contents.push(entry);
                }
                events.push(Event::End(TagEnd::Heading(level)));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                language = Some(match &kind {
                    CodeBlockKind::Fenced(s) => s
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase(),
                    _ => String::new(),
                });
                let label = language
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("text");
                events.push(Event::InlineHtml(format!("<div class='code-label' data-botui-role='10' data-botui-surface='10'>{}</div>", escape(label)).into()));
                events.push(Event::Start(Tag::CodeBlock(kind)));
            }
            Event::End(TagEnd::CodeBlock) => {
                language = None;
                events.push(Event::End(TagEnd::CodeBlock));
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                if let Some((_, _, title)) = &mut heading {
                    title.push_str(&text);
                }
                let fragment = if let Some(lang) = &language {
                    code(&text, range, &raw, &mut sources, &mut passages, lang)
                } else {
                    marked(&text, range, &raw, &mut sources, &mut passages, 0)
                };
                events.push(Event::InlineHtml(fragment.into()));
            }
            Event::Code(text) => {
                if let Some((_, _, title)) = &mut heading {
                    title.push_str(&text);
                }
                let span = inline_code(&text, range, &raw, &mut sources, &mut passages);
                events.push(Event::InlineHtml(format!("<code>{span}</code>").into()));
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let index = doc.references.len();
                doc.references.push(Reference {
                    source: 0,
                    bytes: Some(range.clone()),
                    backlink: false,
                    label: if title.is_empty() {
                        dest_url.to_string()
                    } else {
                        title.to_string()
                    },
                    target: Target::Image(dest_url.to_string()),
                });
                let marker = images.len();
                images.push(Some(index));
                events.push(Event::InlineHtml(
                    format!(
                        "<img data-botui-image='{marker}' alt='{}' />",
                        escape(&dest_url)
                    )
                    .into(),
                ));
            }
            Event::End(TagEnd::Image) => (),
            Event::Start(Tag::Link { dest_url, .. }) => {
                links.push((range.start, dest_url.to_string()));
                events.push(Event::InlineHtml("<u>".into()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((start, target)) = links.pop()
                    && super::resources::web(&target)
                {
                    doc.references.push(Reference {
                        source: 0,
                        bytes: Some(start..range.end),
                        backlink: false,
                        label: target.clone(),
                        target: Target::Web(target),
                    });
                }
                events.push(Event::InlineHtml("</u>".into()));
            }
            Event::TaskListMarker(done) => {
                events.push(Event::InlineHtml(if done { "☑ " } else { "☐ " }.into()))
            }
            other => events.push(other),
        }
    }
    let mut body = String::new();
    html::push_html(&mut body, events.into_iter());
    body = body
        .replace(
            "<blockquote>",
            "<blockquote data-botui-role='8' data-botui-surface='8'>",
        )
        .replace(
            "<pre>",
            "<pre data-botui-role='10' data-botui-surface='10'>",
        )
        .replace("<code>", "<code data-botui-role='10'>")
        .replace("<u>", "<u data-botui-role='7'>")
        .replace("<thead>", "<thead data-botui-role='9'>");
    body = body
        .replace("<th>", "<th data-botui-surface='9'>")
        .replace("<th style=", "<th data-botui-surface='9' style=");
    let style = concat!("<style>", include_str!("markdown.css"), "</style>");
    let sheets = Arc::new(Stylesheets::default());
    let mut rendered = Chapter::new(
        1,
        format!("<!doctype html><html><head>{style}</head><body>{body}</body></html>"),
        "markdown",
        sheets.clone(),
        sources,
    );
    rendered.images = images;
    rendered.preserve_structure = true;
    let mut source_spans = Vec::new();
    let source_body = code(
        &raw,
        0..raw.len(),
        &raw,
        &mut source_spans,
        &mut Vec::new(),
        "markdown",
    );
    let mut source = Chapter::new(
        1,
        format!(
            "<!doctype html><html><head>{style}</head><body><pre class='source'>{source_body}</pre></body></html>"
        ),
        "source",
        sheets,
        source_spans,
    );
    source.preserve_structure = true;
    source.images = rendered.images.clone();
    doc.title = doc
        .contents
        .first()
        .map_or_else(|| fallback.to_owned(), |e| e.title.clone());
    doc.blocks.push(Block {
        text: raw,
        heading: false,
    });
    doc.styled.insert(0, rendered.clone());
    doc.markdown = Some(Markdown {
        rendered,
        source,
        show_source: false,
        passages,
        paragraphs,
        text: String::new(),
        mapping: Vec::new(),
    });
    doc.markdown.as_mut().unwrap().index(&doc.blocks[0].text);
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use botui_core::{LayoutMode, Pager, TextRange};
    const SAMPLE: &str = "# Заголовок\n\nТекст **важное слово** и &amp; знак.\n\n> Цитата\n\n- первый\n- [x] готово\n\n| Имя | Число |\n| --- | --- |\n| тест | 42 |\n\n```rust\nlet answer = 42; // comment\n```\n\n![Схема](images/diagram.png)\n";
    #[test]
    fn markdown_hierarchy_panels_and_examples_survive_reflow_and_source_toggle() {
        let raw = "# Level one\n\n## Level two\n\n### Level three\n\n#### Level four\n\n##### Level five\n\n###### Level six\n\n> Quoted **words**\n\n| Name | Value |\n| --- | --- |\n| Answer | 42 |\n\n```markdown\n# Sample heading\n\n**Strong** and `inline`\n```\n\nAfter the panel.\n";
        let mut doc = parse(raw.into(), "sample");
        for width in [32, 80] {
            let pager = Pager::new(&doc, width, 500, LayoutMode::Single);
            let rows = pager.lines(0);
            for (level, word) in ["one", "two", "three", "four", "five", "six"]
                .iter()
                .enumerate()
            {
                let row = rows
                    .iter()
                    .find(|r| r.text.contains(&format!("Level {word}")))
                    .unwrap();
                assert!(
                    row.text
                        .trim_start()
                        .starts_with(['①', '②', '③', '④', '⑤', '⑥'])
                );
                assert!(
                    row.spans
                        .iter()
                        .any(|s| s.source.is_some() && s.face.role == (level + 1) as u8)
                );
                assert_eq!(row.surfaces.len(), 1);
                assert_eq!(row.surfaces[0].width, width);
            }
            for label in ["Quoted", "Name", "# Sample heading", "**Strong**"] {
                let row = rows.iter().find(|r| r.text.contains(label)).unwrap();
                assert!(
                    !row.surfaces.is_empty(),
                    "missing panel for {label}: {row:?}"
                );
            }
            let sample = rows
                .iter()
                .find(|r| r.text.contains("# Sample heading"))
                .unwrap();
            assert!(sample.spans.iter().any(|s| s.face.syntax == 1));
            let after = rows
                .iter()
                .find(|r| r.text.contains("After the panel."))
                .unwrap();
            assert!(after.surfaces.is_empty());
            assert!(after.spans.iter().all(|s| s.face.role == 0));
            let at = raw.find("Strong").unwrap();
            let range = TextRange::new(
                Anchor { block: 0, byte: at },
                Anchor {
                    block: 0,
                    byte: at + 6,
                },
            );
            assert_eq!(doc.text_range(range), "Strong");
            assert!(
                rows.iter()
                    .flat_map(|r| r.text_cells(&doc))
                    .any(|c| c.source.intersects(range))
            );
        }
        doc.toggle_markdown();
        let pager = Pager::new(&doc, 80, 500, LayoutMode::Single);
        let rows = pager.lines(0);
        assert!(rows.iter().any(|r| r.text.contains("###### Level six")));
        assert!(
            rows.iter()
                .any(|r| r.spans.iter().any(|s| s.face.syntax == 1))
        );
        assert!(rows.iter().all(|r| r.surfaces.is_empty()));
    }

    #[test]
    fn lists_rules_and_alerts_have_visible_structure_and_exact_text_addresses() {
        let raw = "3. First\n4. Second\n   - Nested bullet\n\n- [x] Done\n- [ ] Pending\n\n---\n\n> [!WARNING]\n> Keep **this text**.\n\nAfter.\n";
        let doc = parse(raw.into(), "blocks");
        for width in [24, 80] {
            let pager = Pager::new(&doc, width, 500, LayoutMode::Single);
            let rows = pager.lines(0);
            let text = rows
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            for expected in [
                "3. First",
                "4. Second",
                "• Nested bullet",
                "☑ Done",
                "☐ Pending",
                "Warning",
                "Keep this text.",
            ] {
                assert!(text.contains(expected), "missing {expected}: {text}");
            }
            assert!(!text.contains("[!WARNING]"));
            assert!(!text.contains("• ☑"));
            assert!(rows.iter().any(|r| r.text.chars().filter(|&c| c == '─').count() >= usize::from(width) - 2));
            let warning = rows
                .iter()
                .find(|r| r.text.contains("Keep this text."))
                .unwrap();
            assert!(warning.surfaces.iter().any(|s| s.role == 15));
            let byte = raw.find("this text").unwrap();
            let range = TextRange::new(
                Anchor { block: 0, byte },
                Anchor {
                    block: 0,
                    byte: byte + 9,
                },
            );
            let cells: Vec<_> = warning
                .text_cells(&doc)
                .into_iter()
                .filter(|c| c.source.intersects(range))
                .collect();
            assert_eq!(cells.first().unwrap().source.start, range.start);
            assert_eq!(cells.last().unwrap().source.end, range.end);
            assert_eq!(doc.text_range(range), "this text");
        }
    }

    #[test]
    fn normalized_inline_code_keeps_character_addresses() {
        let raw = "Before ` a\nb ` after";
        let doc = parse(raw.into(), "code");
        let b = Anchor {
            block: 0,
            byte: raw.find("b ").unwrap(),
        };
        assert_eq!(doc.text_range(doc.character_at(b).unwrap()), "b");
        let pager = Pager::new(&doc, 80, 30, LayoutMode::Single);
        assert!(
            pager
                .lines(0)
                .iter()
                .any(|l| l.text.contains("Before a b after"))
        );
    }
    #[test]
    fn visible_words_paragraphs_and_entities_skip_markdown_delimiters() {
        let text = "Alpha **bravo** charlie. Привет мир.\n\nNext paragraph with &amp; and `code`.";
        let doc = parse(text.into(), "test");
        let alpha = Anchor::default();
        let bravo = doc.move_text(alpha, true, true);
        assert_eq!(doc.text_range(doc.word_at(bravo).unwrap()), "bravo");
        assert_eq!(
            doc.move_text(Anchor { block: 0, byte: 5 }, true, false),
            bravo
        );
        assert_eq!(
            doc.text_range(doc.paragraph_at(bravo).unwrap()),
            "Alpha bravo charlie. Привет мир."
        );
        assert_eq!(
            doc.move_paragraph(bravo, true).byte,
            text.find("Next").unwrap()
        );
        let amp = Anchor {
            block: 0,
            byte: text.find("&amp;").unwrap(),
        };
        let range = doc.character_at(amp).unwrap();
        assert_eq!(doc.text_range(range), "&");
        assert_eq!(range.end.byte - range.start.byte, 5);
    }
    #[test]
    fn formatted_and_source_keep_identical_utf8_anchors() {
        let mut doc = parse(SAMPLE.into(), "fallback");
        assert_eq!(doc.title, "Заголовок");
        let start = SAMPLE.find("важное слово").unwrap();
        let range = TextRange::new(
            Anchor {
                block: 0,
                byte: start,
            },
            Anchor {
                block: 0,
                byte: start + "важное слово".len(),
            },
        );
        for source in [false, true, false] {
            if doc.markdown.as_ref().unwrap().show_source != source {
                doc.toggle_markdown();
            }
            for width in [32, 80] {
                let pager = Pager::new(&doc, width, 500, LayoutMode::Single);
                let rows = pager.lines(0);
                assert!(
                    rows.iter().any(|l| l.text.contains("важное")),
                    "rows: {rows:?}"
                );
                let cells: Vec<_> = rows
                    .iter()
                    .flat_map(|l| l.text_cells(&doc))
                    .filter(|c| c.source.intersects(range))
                    .collect();
                assert!(!cells.is_empty());
                assert_eq!(cells.first().unwrap().source.start, range.start);
                assert_eq!(cells.last().unwrap().source.end, range.end);
                if !source {
                    assert!(
                        rows.iter()
                            .any(|l| l.spans.iter().any(|s| s.face.syntax == 1)),
                        "no syntax: {rows:?}"
                    );
                    assert!(
                        rows.iter().any(|l| !l.figures.is_empty()),
                        "no image: {rows:?}"
                    );
                    assert!(rows.iter().any(|l| l.text.contains("42")));
                    let hits = pager.search_index(&doc).find("Текст важное слово и & знак");
                    assert_eq!(hits.len(), 1);
                    assert_eq!(hits[0].0.byte, SAMPLE.find("Текст").unwrap());
                }
            }
            assert_eq!(doc.text_range(range), "важное слово");
        }
        let full = doc.text_range(TextRange::new(
            Anchor::default(),
            Anchor {
                block: 0,
                byte: SAMPLE.len(),
            },
        ));
        assert!(full.contains("Текст важное слово и & знак."), "{full}");
        assert!(!full.contains("**"));
    }
}
