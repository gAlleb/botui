use botui_bookorbit::annotations::{Annotation, New, Pdf, Rect};
use botui_core::{Anchor, Document, TextRange};
use botui_store::{HighlightColor, Note, PdfRegion};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

pub enum Mapper {
    Text(Arc<Document>),
    Pdf {
        path: PathBuf,
        pages: BTreeMap<u32, botui_fixed::text::PageText>,
    },
}
fn color(c: Option<HighlightColor>) -> &'static str {
    match c.unwrap_or(HighlightColor::Yellow) {
        HighlightColor::Yellow => "#FACC15",
        HighlightColor::Green => "#4ADE80",
        HighlightColor::Blue => "#38BDF8",
        HighlightColor::Pink => "#F472B6",
        HighlightColor::Purple => "#C084FC",
    }
}
fn local_color(c: &str) -> HighlightColor {
    let rgb = u32::from_str_radix(c.trim_start_matches('#'), 16).ok();
    HighlightColor::ALL
        .into_iter()
        .min_by_key(|v| {
            let r = u32::from_str_radix(&color(Some(*v))[1..], 16).unwrap();
            [0, 8, 16]
                .into_iter()
                .map(|s| {
                    (((rgb.unwrap_or(0xFACC15) >> s) & 255) as i32 - ((r >> s) & 255) as i32).pow(2)
                })
                .sum::<i32>()
        })
        .unwrap()
}
fn same_text(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}
impl Mapper {
    fn page(&mut self, n: u32) -> Option<&botui_fixed::text::PageText> {
        let Self::Pdf { path, pages } = self else {
            return None;
        };
        if let std::collections::btree_map::Entry::Vacant(e) = pages.entry(n) {
            e.insert(botui_fixed::text::page_text(path, n, &AtomicBool::new(false)).ok()?);
        }
        pages.get(&n)
    }
    pub fn export(&mut self, note: &Note, file: u64) -> Option<New> {
        let mut new = New {
            book_file_id: file,
            cfi: None,
            pdf: None,
            text: note.quote.clone(),
            color: color(note.color).into(),
            style: "highlight".into(),
            note: (!note.body.trim().is_empty()).then(|| note.body.clone()),
        };
        match self {
            Self::Text(doc) => {
                new.cfi = Some(if let Some(range) = note.range() {
                    doc.range_cfi(range)?
                } else {
                    new.text = doc
                        .blocks
                        .get(note.anchor().block)?
                        .text
                        .get(note.anchor().byte..)?
                        .chars()
                        .take(160)
                        .collect();
                    doc.cfi(note.anchor())?
                });
            }
            Self::Pdf { .. } => {
                let first = note.pdf_regions.first()?;
                if note.pdf_regions.iter().any(|r| r.page != first.page) {
                    return None;
                }
                let [w, h] = self.page(first.page)?.size;
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
        (!new.text.trim().is_empty()).then_some(new)
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
                    .words
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
                let at = Anchor {
                    block: pdf.page as usize,
                    byte: words.first()?.bytes.start,
                };
                let end = Anchor {
                    block: at.block,
                    byte: words.last()?.bytes.end,
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
            Some(local_color(&remote.color)),
            regions,
        ))
    }
    pub fn edit_color(note: &Note, base: &Note, remote: &Annotation) -> String {
        // An unrecognized browser color remains exact until the user changes it here.
        if note.color == base.color {
            remote.color.clone()
        } else {
            color(note.color).into()
        }
    }
}
