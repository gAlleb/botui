//! Bidirectional correspondence between normalized UTF-8 and a Foliate text node's UTF-16 offsets.
use super::{Anchor, Document};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug)]
pub struct Run {
    pub byte: usize,
    pub end: usize,
    pub utf16: usize,
    pub path: Arc<str>,
}
#[derive(Default, Clone, Debug)]
pub struct Locations(pub BTreeMap<usize, Vec<Run>>);
impl Document {
    /// A fraction of the logical text, independent of the terminal dimensions.
    pub fn text_percentage(&self, a: Anchor) -> f64 {
        let total: usize = self.blocks.iter().map(|b| b.text.len()).sum();
        let before: usize = self.blocks.iter().take(a.block).map(|b| b.text.len()).sum();
        let inside = self
            .blocks
            .get(a.block)
            .map_or(0, |b| a.byte.min(b.text.len()));
        100.0 * (before + inside) as f64 / total.max(1) as f64
    }
    pub fn cfi(&self, a: Anchor) -> Option<String> {
        let runs = self.locations.0.get(&a.block)?;
        let text = &self.blocks.get(a.block)?.text;
        let i = runs.partition_point(|r| r.end <= a.byte);
        let r = runs.get(i).or_else(|| runs.last())?;
        let pos = a.byte;
        if pos < r.byte || pos > r.end {
            return None;
        }
        let offset = r.utf16 + text.get(r.byte..pos)?.encode_utf16().count();
        Some(format!("epubcfi({}:{offset})", r.path))
    }
    pub fn resolve_cfi(&self, cfi: &str) -> Option<Anchor> {
        let value = cfi.strip_prefix("epubcfi(")?.strip_suffix(')')?;
        if value.len() > 8192 {
            return None;
        }
        let mut s = String::new();
        let mut assertion = false;
        let mut escaped = false;
        for c in value.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '^' if assertion => escaped = true,
                '[' if !assertion => assertion = true,
                ']' if assertion => assertion = false,
                _ if assertion => (),
                ',' | '~' | '@' => return None,
                _ => s.push(c),
            }
        }
        if assertion || escaped {
            return None;
        }
        let (path, offset) = s
            .split_once(';')
            .map_or(s.as_str(), |(p, _)| p)
            .rsplit_once(':')?;
        let offset: usize = offset.parse().ok()?;
        for (&block, runs) in &self.locations.0 {
            for r in runs.iter().filter(|r| &*r.path == path) {
                if offset < r.utf16 {
                    // Whitespace collapsed by the text reader, in this same DOM text node.
                    return Some(Anchor {
                        block,
                        byte: r.byte,
                    });
                }
                let mut units = offset - r.utf16;
                let text = &self.blocks[block].text[r.byte..r.end];
                for (i, c) in text.char_indices() {
                    if units == 0 {
                        return Some(Anchor {
                            block,
                            byte: r.byte + i,
                        });
                    }
                    if units < c.len_utf16() {
                        return None;
                    }
                    units -= c.len_utf16();
                }
                if units == 0 {
                    return Some(Anchor { block, byte: r.end });
                }
            }
        }
        None
    }
}

impl Document {
    /// A browser may place its caret in whitespace at a visual line break.
    /// Resume on the following printed character, including across a paragraph break.
    pub fn reading_anchor(&self, cfi: &str) -> Option<Anchor> {
        let at = self.bookmark_anchor(cfi)?;
        let tail = self.blocks.get(at.block)?.text.get(at.byte..)?;
        if let Some(i) = tail.find(|c: char| !c.is_whitespace()) {
            return Some(Anchor {
                block: at.block,
                byte: at.byte + i,
            });
        }
        Some(
            self.blocks
                .iter()
                .enumerate()
                .skip(at.block + 1)
                .find_map(|(block, b)| {
                    b.text
                        .find(|c: char| !c.is_whitespace())
                        .map(|byte| Anchor { block, byte })
                })
                .unwrap_or(at),
        )
    }

    /// Foliate also emits range CFIs for dog-ears. Navigate to the start of
    /// the range. Escaped commas inside an assertion do not delimit a range.
    pub fn bookmark_anchor(&self, cfi: &str) -> Option<Anchor> {
        if let Some(place) = self.resolve_cfi(cfi) {
            return Some(place);
        }
        let value = cfi.strip_prefix("epubcfi(")?.strip_suffix(')')?;
        if value.len() > 8192 {
            return None;
        }
        let mut clean = String::new();
        let mut escaped = false;
        let mut bracket = false;
        for c in value.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '^' if bracket => escaped = true,
                '[' if !bracket => bracket = true,
                ']' if bracket => bracket = false,
                _ if bracket => (),
                _ => clean.push(c),
            }
        }
        if bracket || escaped {
            return None;
        }
        let parts: Vec<_> = clean.split(',').collect();
        let path = match parts.as_slice() {
            [point] => point.to_string(),
            [common, first, _] => format!("{common}{first}"),
            _ => return None,
        };
        if path.contains(':') {
            return self.resolve_cfi(&format!("epubcfi({path})"));
        }
        // An element dog-ear targets the first mapped text inside that exact element.
        let last = path.rsplit('/').next()?.parse::<u32>().ok()?;
        if !last.is_multiple_of(2) {
            return self.resolve_cfi(&format!("epubcfi({path}:0)"));
        }
        let prefix = format!("{path}/");
        self.locations.0.iter().find_map(|(&block, runs)| {
            runs.iter()
                .find(|r| r.path.starts_with(&prefix))
                .map(|r| Anchor {
                    block,
                    byte: r.byte,
                })
        })
    }
}
