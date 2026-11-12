//! CommonMark reuses the ebook layout engine; both views address the original bytes.
use super::{Result, invalid};
use botui_core::{
    Anchor, Block, ContentsEntry, Document, Markdown, Reference, Target,
    publisher::{Chapter, SourceText, Stylesheets},
};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};
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
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
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
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((level as usize, range.start, String::new()));
                events.push(Event::Start(Tag::Heading {
                    level,
                    id: None,
                    classes: vec![],
                    attrs: vec![],
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
    let style = "<style>body{margin:0}h1,h2,h3,h4,h5,h6{margin:1em 0;font-weight:bold}blockquote{border-left:2px solid;padding-left:1em;margin:1em 0}pre{white-space:pre-wrap;border:1px solid;padding:8px;margin:1em 0}code{font-style:normal}table{border-collapse:collapse;max-width:100%}th,td{border:1px solid;padding:4px 8px}img{max-width:100%;max-height:50vh}p{margin:1em 0}</style>";
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
    let mut source = Chapter::new(
        1,
        format!(
            "<!doctype html><html><head>{style}</head><body><pre style='border:0;padding:0' data-botui-text='0'>{}</pre></body></html>",
            escape(&raw)
        ),
        "source",
        sheets,
        vec![vec![SourceText {
            start: Anchor::default(),
            end: raw.len(),
            child: 0,
            display: None,
        }]],
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
