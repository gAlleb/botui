use super::output;
use botui_formats::FileKind;
use std::{
    io,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct TextLayer {
    pages: Vec<String>,
}

pub struct PageMatch {
    pub page: u32,
    pub excerpt: String,
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

impl TextLayer {
    pub fn find(&self, query: &str, cancel: &AtomicBool) -> Vec<PageMatch> {
        let query = normalize(query).to_lowercase();
        if query.is_empty() {
            return vec![];
        }
        self.pages
            .iter()
            .enumerate()
            .take_while(|_| !cancel.load(Ordering::Relaxed))
            .filter_map(|(i, page)| {
                let byte = page.to_lowercase().find(&query)?;
                let mut folded_byte = 0;
                let index = page.char_indices().find_map(|(index, c)| {
                    folded_byte += c.to_lowercase().map(char::len_utf8).sum::<usize>();
                    (folded_byte > byte).then_some(index)
                })?;
                let start = page[..index]
                    .char_indices()
                    .rev()
                    .nth(17)
                    .map_or(0, |(i, _)| i);
                let end = page[index..]
                    .char_indices()
                    .nth(42)
                    .map_or(page.len(), |(i, _)| index + i);
                let excerpt = format!(
                    "{}{}{}",
                    if start > 0 { "…" } else { "" },
                    &page[start..end],
                    if end < page.len() { "…" } else { "" }
                );
                Some(PageMatch {
                    page: i as u32 + 1,
                    excerpt,
                })
            })
            .collect()
    }
}

/// Search the embedded text layer. Keep a physical page for empty scans as well.
pub fn text_layer(path: &Path, kind: FileKind, cancel: &AtomicBool) -> io::Result<TextLayer> {
    let path = path.canonicalize()?;
    let mut cmd = match kind {
        FileKind::Pdf => {
            let mut c = Command::new("pdftotext");
            c.args(["-enc", "UTF-8"]).arg(&path).arg("-");
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
                .arg("text");
            output(cmd, cancel, limit)?
        }
        r => r?,
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut pages = Vec::new();
    if kind == FileKind::Pdf {
        pages.extend(text.split_terminator('\x0c').map(normalize));
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
                            pages.push(normalize(&value));
                            value.clear();
                        }
                    }
                    _ => (),
                }
            }
        }
    }
    if pages.iter().all(String::is_empty) {
        return Err(io::Error::other("This scan has no searchable text layer."));
    }
    Ok(TextLayer { pages })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_excerpt_uses_original_text_after_a_length_changing_case_fold() {
        let prefix = "İ".repeat(64);
        let layer = TextLayer {
            pages: vec![String::new(), normalize(&format!("{prefix} Маяк\nу моря"))],
        };
        let hits = layer.find("маяк у", &AtomicBool::new(false));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].page, 2);
        assert!(hits[0].excerpt.starts_with('…'));
        assert!(hits[0].excerpt.ends_with("Маяк у моря"));
    }
}
