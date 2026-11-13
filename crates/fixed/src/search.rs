use super::{output, text::PageText};
use botui_formats::FileKind;
use std::{
    io,
    ops::Range,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct TextLayer {
    pages: Vec<PageText>,
}

#[derive(Clone, Debug)]
pub struct PageMatch {
    pub page: u32,
    pub excerpt: String,
    pub bytes: Range<usize>,
}

fn normalize(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for word in text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|s| !s.is_empty())
    {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    result
}

// Collapse whitespace and case while retaining original UTF-8 boundaries. PDF
// phrases may cross a printed line; Unicode lowercasing can change byte lengths.
type ByteMap = Vec<(Range<usize>, Range<usize>)>;
fn folded(text: &str) -> (String, ByteMap) {
    let mut value = String::new();
    let mut mapping = Vec::new();
    for (byte, c) in text.char_indices() {
        let space = c.is_whitespace() || c.is_control();
        if space && (value.is_empty() || value.ends_with(' ')) {
            continue;
        }
        let start = value.len();
        if space {
            value.push(' ');
        } else {
            value.extend(c.to_lowercase());
        }
        mapping.push((start..value.len(), byte..byte + c.len_utf8()));
    }
    (value, mapping)
}

impl TextLayer {
    pub fn page(&self, page: u32) -> Option<&PageText> {
        self.pages.get(page.checked_sub(1)? as usize)
    }
    pub fn find(&self, query: &str, cancel: &AtomicBool) -> Vec<PageMatch> {
        let query = normalize(query).to_lowercase();
        if query.is_empty() {
            return vec![];
        }
        let mut hits = Vec::new();
        for (i, page) in self.pages.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let (value, mapping) = folded(&page.text);
            for (byte, matched) in value.match_indices(&query) {
                let first = mapping.partition_point(|(r, _)| r.end <= byte);
                let last = mapping.partition_point(|(r, _)| r.start < byte + matched.len()) - 1;
                let bytes = mapping[first].1.start..mapping[last].1.end;
                let start = page.text[..bytes.start]
                    .char_indices()
                    .rev()
                    .nth(17)
                    .map_or(0, |(i, _)| i);
                let end = page.text[bytes.end..]
                    .char_indices()
                    .nth(42)
                    .map_or(page.text.len(), |(i, _)| bytes.end + i);
                hits.push(PageMatch {
                    page: i as u32 + 1,
                    excerpt: format!(
                        "{}{}{}",
                        if start > 0 { "…" } else { "" },
                        normalize(&page.text[start..end]),
                        if end < page.text.len() { "…" } else { "" }
                    ),
                    bytes,
                });
            }
        }
        hits
    }
}

/// Search the embedded text layer. Keep a physical page for empty scans as well.
pub fn text_layer(path: &Path, kind: FileKind, cancel: &AtomicBool) -> io::Result<TextLayer> {
    let path = path.canonicalize()?;
    let mut cmd = match kind {
        FileKind::Pdf => {
            let mut c = Command::new("pdftotext");
            c.args(["-bbox-layout", "-enc", "UTF-8"])
                .arg(&path)
                .arg("-");
            c
        }
        FileKind::Djvu => {
            let mut c = Command::new("djvutxt");
            c.arg("-detail=page").arg(&path);
            c
        }
        _ => return Err(io::ErrorKind::InvalidInput.into()),
    };
    let limit = 32 << 20;
    let bytes = match output(cmd, cancel, limit) {
        Err(e) if e.kind() == io::ErrorKind::NotFound && kind == FileKind::Pdf => {
            cmd = Command::new("python3");
            cmd.args(["-I", "-c", include_str!("poppler.py")])
                .arg(&path)
                .arg("text-boxes");
            output(cmd, cancel, limit)?
        }
        r => r?,
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut pages = Vec::new();
    if kind == FileKind::Pdf {
        pages = super::text::parse_pages(&text)?;
    } else {
        // miniexp prints one root expression per physical page. Quoted strings
        // can span lines and contain escaped quotes and parentheses.
        let mut value = String::new();
        let mut quoted = false;
        let mut escape = false;
        let mut depth = 0u32;
        for c in text.chars() {
            if quoted {
                if escape {
                    value.push(match c {
                        'n' | 'r' | 't' => ' ',
                        _ => c,
                    });
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c == '"' {
                    quoted = false;
                    value.push(' ');
                } else {
                    value.push(c);
                }
            } else {
                match c {
                    '"' => quoted = true,
                    '(' => depth += 1,
                    ')' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            pages.push(PageText {
                                text: normalize(&value),
                                ..Default::default()
                            });
                            value.clear();
                        }
                    }
                    _ => (),
                }
            }
        }
    }
    if pages.iter().all(|p| p.text.is_empty()) {
        return Err(io::Error::other("This scan has no searchable text layer."));
    }
    Ok(TextLayer { pages })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_occurrence_keeps_its_page_bytes_and_word_boxes() {
        let pages = super::super::text::parse_pages(
            r#"<doc>
          <page width="200" height="400"><flow><block>
            <line><word xMin="10" yMin="10" xMax="20" yMax="20">İ</word>
            <word xMin="30" yMin="10" xMax="70" yMax="20">Маяк</word></line>
            <line><word xMin="10" yMin="50" xMax="20" yMax="60">у</word>
            <word xMin="30" yMin="50" xMax="70" yMax="60">моря</word></line>
            <line><word xMin="10" yMin="90" xMax="50" yMax="100">МАЯК</word></line>
          </block></flow></page><page width="200" height="400"/>
          <page width="200" height="400"><flow><block><line>
            <word xMin="10" yMin="200" xMax="50" yMax="210">маяк</word>
          </line></block></flow></page></doc>"#,
        )
        .unwrap();
        let layer = TextLayer { pages };
        let hits = layer.find("маяк", &AtomicBool::new(false));
        assert_eq!(hits.iter().map(|h| h.page).collect::<Vec<_>>(), [1, 1, 3]);
        for hit in &hits {
            let page = layer.page(hit.page).unwrap();
            assert_eq!(page.text[hit.bytes.clone()].to_lowercase(), "маяк");
            assert_eq!(
                page.words.iter().filter(|w| w.bytes == hit.bytes).count(),
                1
            );
        }
        let phrase = layer.find("МАЯК   у\tморя", &AtomicBool::new(false));
        assert_eq!(phrase.len(), 1);
        assert_eq!(
            &layer.page(1).unwrap().text[phrase[0].bytes.clone()],
            "Маяк\nу моря"
        );
        assert!(layer.find("маяк", &AtomicBool::new(true)).is_empty());
    }

    #[test]
    fn unicode_excerpt_uses_original_text_after_a_length_changing_case_fold() {
        let prefix = "İ".repeat(64);
        let layer = TextLayer {
            pages: vec![
                PageText::default(),
                PageText {
                    text: format!("{prefix} Маяк\nу моря"),
                    ..Default::default()
                },
            ],
        };
        let hits = layer.find("маяк у", &AtomicBool::new(false));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].page, 2);
        assert!(hits[0].excerpt.starts_with('…'));
        assert!(hits[0].excerpt.ends_with("Маяк у моря"));
    }
}
