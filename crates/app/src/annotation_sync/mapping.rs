use botui_bookorbit::annotations::{Annotation, New, Pdf, Rect};
use botui_core::{Anchor, Document, RangeCfiError, TextRange};
use botui_store::{HighlightColor, Note, PdfRegion};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum LocalOnly {
    PdfPageNote,
    MultiplePdfPages,
    MultipleEpubDocuments,
    NoExactLocation,
    ChangedPassage,
}
impl LocalOnly {
    pub fn label(self) -> &'static str {
        match self {
            Self::PdfPageNote => "whole-page PDF notes",
            Self::MultiplePdfPages => "selections across PDF pages",
            Self::MultipleEpubDocuments => "selections across EPUB documents",
            Self::NoExactLocation => "marks without an exact location or passage",
            Self::ChangedPassage => "edits to an already synced passage's location or text",
        }
    }
}

pub enum Mapper {
    Text(Arc<Document>),
    Pdf {
        path: PathBuf,
        pages: BTreeMap<u32, botui_fixed::text::PageText>,
    },
}
fn color(c: Option<HighlightColor>) -> String {
    c.unwrap_or(HighlightColor::Yellow).hex()
}
fn same_text(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}
fn unique_pdf_quote(
    text: &botui_fixed::text::PageText,
    regions: &[PdfRegion],
    quote: &str,
) -> Option<std::ops::Range<usize>> {
    if quote.trim().is_empty() {
        return None;
    }
    // Without glyph geometry a server's partial-word rectangle can still be
    // located by its unique exact quote inside the intersecting source words.
    // Preserve the server rectangle: this does not estimate character boxes.
    let mut words = text.words.iter().filter(|w| {
        regions.iter().any(|r| {
            w.bounds[0] < r.bounds[2]
                && w.bounds[2] > r.bounds[0]
                && w.bounds[1] < r.bounds[3]
                && w.bounds[3] > r.bounds[1]
        })
    });
    let first = words.next()?;
    let last = words.next_back().unwrap_or(first);
    let candidate = &text.text[first.bytes.start..last.bytes.end];
    let mut matches = candidate
        .char_indices()
        .filter_map(|(i, _)| candidate[i..].starts_with(quote).then_some(i));
    let start = first.bytes.start + matches.next()?;
    matches
        .next()
        .is_none()
        .then_some(start..start + quote.len())
}
impl Mapper {
    pub fn local_color(remote: &Annotation) -> HighlightColor {
        HighlightColor::from_hex(&remote.color).unwrap_or(HighlightColor::Yellow)
    }

    fn page(&mut self, n: u32) -> Option<&botui_fixed::text::PageText> {
        let Self::Pdf { path, pages } = self else {
            return None;
        };
        if !pages.contains_key(&n)
            && pages.len() >= 8
            && let Some(stale) = pages.keys().max_by_key(|p| p.abs_diff(n)).copied()
        {
            pages.remove(&stale);
        }
        if let std::collections::btree_map::Entry::Vacant(e) = pages.entry(n) {
            e.insert(botui_fixed::text::interactive_page(path, n, &AtomicBool::new(false)).ok()?);
        }
        pages.get(&n)
    }
    pub fn export(&mut self, note: &Note, file: u64) -> Result<New, LocalOnly> {
        let mut new = New {
            book_file_id: file,
            cfi: None,
            pdf: None,
            text: note.quote.clone(),
            color: color(note.color),
            style: "highlight".into(),
            note: (!note.body.trim().is_empty()).then(|| note.body.clone()),
        };
        match self {
            Self::Text(doc) => {
                new.cfi = Some(if let Some(range) = note.range() {
                    doc.try_range_cfi(range).map_err(|reason| match reason {
                        RangeCfiError::NoExactLocation => LocalOnly::NoExactLocation,
                        RangeCfiError::MultipleDocuments => LocalOnly::MultipleEpubDocuments,
                    })?
                } else {
                    new.text = doc
                        .blocks
                        .get(note.anchor().block)
                        .ok_or(LocalOnly::NoExactLocation)?
                        .text
                        .get(note.anchor().byte..)
                        .ok_or(LocalOnly::NoExactLocation)?
                        .chars()
                        .take(160)
                        .collect();
                    doc.cfi(note.anchor()).ok_or(LocalOnly::NoExactLocation)?
                });
            }
            Self::Pdf { .. } => {
                let first = note.pdf_regions.first().ok_or(if note.range().is_none() {
                    LocalOnly::PdfPageNote
                } else {
                    LocalOnly::NoExactLocation
                })?;
                if note.pdf_regions.iter().any(|r| r.page != first.page) {
                    return Err(LocalOnly::MultiplePdfPages);
                }
                let [w, h] = self
                    .page(first.page)
                    .ok_or(LocalOnly::NoExactLocation)?
                    .size;
                let rects: Vec<_> = note
                    .pdf_regions
                    .iter()
                    .map(|r| Rect {
                        x: r.bounds[0] * w,
                        y: r.bounds[1] * h,
                        width: (r.bounds[2] - r.bounds[0]) * w,
                        height: (r.bounds[3] - r.bounds[1]) * h,
                    })
                    .collect();
                let x = rects.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
                let y = rects.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
                let right = rects.iter().map(|r| r.x + r.width).fold(0., f32::max);
                let bottom = rects.iter().map(|r| r.y + r.height).fold(0., f32::max);
                new.pdf = Some(Pdf {
                    page: first.page - 1,
                    rect: Rect {
                        x,
                        y,
                        width: right - x,
                        height: bottom - y,
                    },
                    rects,
                });
            }
        }
        if new.text.trim().is_empty() {
            Err(LocalOnly::NoExactLocation)
        } else {
            Ok(new)
        }
    }
    pub fn import(&mut self, remote: &Annotation, id: String, file: u64) -> Option<Note> {
        if remote.jump_file_id != Some(file) {
            return None;
        }
        let (range, at, regions) = match self {
            Self::Text(doc) => {
                let cfi = remote.cfi.as_deref()?;
                if let Some(range) = doc.resolve_range_cfi(cfi) {
                    if !same_text(&doc.text_range(range), &remote.text) {
                        return None;
                    }
                    (Some(range), range.start, vec![])
                } else {
                    (None, doc.resolve_cfi(cfi)?, vec![])
                }
            }
            Self::Pdf { .. } => {
                let pdf = remote.pdf.as_ref()?;
                let page = pdf.page.checked_add(1)?;
                let text = self.page(page)?;
                let [w, h] = text.size;
                let rects = if pdf.rects.is_empty() {
                    std::slice::from_ref(&pdf.rect)
                } else {
                    &pdf.rects
                };
                let regions: Vec<_> = rects
                    .iter()
                    .map(|r| PdfRegion {
                        page,
                        bounds: [r.x / w, r.y / h, (r.x + r.width) / w, (r.y + r.height) / h],
                    })
                    .collect();
                if regions.iter().any(|r| {
                    r.bounds
                        .iter()
                        .any(|v| !v.is_finite() || !(0.0..=1.001).contains(v))
                        || r.bounds[0] >= r.bounds[2]
                        || r.bounds[1] >= r.bounds[3]
                }) {
                    return None;
                }
                let words: Vec<_> = text
                    .units()
                    .iter()
                    .filter(|word| {
                        regions.iter().any(|r| {
                            let x = (word.bounds[0] + word.bounds[2]) / 2.;
                            let y = (word.bounds[1] + word.bounds[3]) / 2.;
                            x >= r.bounds[0]
                                && x <= r.bounds[2]
                                && y >= r.bounds[1]
                                && y <= r.bounds[3]
                        })
                    })
                    .collect();
                let bytes = words
                    .first()
                    .zip(words.last())
                    .map(|(a, b)| a.bytes.start..b.bytes.end)
                    .filter(|r| same_text(&text.text[r.clone()], &remote.text))
                    .or_else(|| unique_pdf_quote(text, &regions, &remote.text))?;
                let at = Anchor {
                    block: pdf.page as usize,
                    byte: bytes.start,
                };
                let end = Anchor {
                    block: at.block,
                    byte: bytes.end,
                };
                (Some(TextRange::new(at, end)), at, regions)
            }
        };
        Some(Note::imported(
            id,
            range,
            at,
            remote.text.clone(),
            remote.note.clone().unwrap_or_default(),
            Some(Self::local_color(remote)),
            regions,
        ))
    }
    pub fn edit_color(note: &Note, base: &Note, remote: &Annotation) -> String {
        // An unrecognized browser color remains exact until the user changes it here.
        if note.color == base.color {
            remote.color.clone()
        } else {
            color(note.color)
        }
    }
}
