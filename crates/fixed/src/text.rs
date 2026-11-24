//! The embedded PDF text, in reading order, with page-relative word boxes.
//! Extract only requested pages; neither rasterization nor OCR is involved.
use std::{io, ops::Range, path::Path, process::Command, sync::atomic::AtomicBool};
mod interaction;
pub use interaction::{Link, LinkTarget};

#[derive(Clone, Debug)]
pub struct Word {
    pub bytes: Range<usize>,
    /// Left, top, right, bottom in the unrotated page, normalized to 0..1.
    pub bounds: [f32; 4],
    pub line: usize,
    pub block: usize,
}

#[derive(Clone, Debug, Default)]
pub struct PageText {
    /// Unscaled page points in the same top-left coordinate space as word boxes.
    pub size: [f32; 2],
    pub text: String,
    pub words: Vec<Word>,
    /// Exact character boxes when Poppler GLib agrees with the canonical text.
    /// Empty means word-level interaction; never guess a glyph's width.
    pub characters: Vec<Word>,
    pub links: Vec<Link>,
    pub interactive: bool,
}

impl PageText {
    /// The visible gap belongs to the selection only if its space is selected.
    pub fn selected_spaces(&self, bytes: Range<usize>) -> impl Iterator<Item = [f32; 4]> + '_ {
        let first = self.words.partition_point(|w| w.bytes.end < bytes.start);
        self.words[first..]
            .windows(2)
            .take_while(move |pair| pair[0].bytes.end < bytes.end)
            .filter_map(|pair| {
                let (left, right) = (&pair[0], &pair[1]);
                (left.line == right.line
                    && left.block == right.block
                    && self.text.get(left.bytes.end..right.bytes.start) == Some(" "))
                .then(|| interaction::whitespace_box(left.bounds, right.bounds))
                .flatten()
            })
    }

    pub fn units(&self) -> &[Word] {
        if self.characters.is_empty() {
            &self.words
        } else {
            &self.characters
        }
    }
}

/// Load interaction geometry only for visited pages, not for a whole-book search.
pub fn interactive_page(path: &Path, page: u32, cancel: &AtomicBool) -> io::Result<PageText> {
    let mut result = page_text(path, page, cancel)?;
    interaction::enrich(path, page, cancel, &mut result);
    result.interactive = true;
    Ok(result)
}

pub fn page_text(path: &Path, page: u32, cancel: &AtomicBool) -> io::Result<PageText> {
    if page == 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut cmd = Command::new("pdftotext");
    let mut glib = false;
    cmd.args([
        "-f",
        &page.to_string(),
        "-l",
        &page.to_string(),
        "-bbox-layout",
        "-enc",
        "UTF-8",
    ])
    .arg(path.canonicalize()?)
    .arg("-");
    let output = match super::output(cmd, cancel, 8 << 20) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            glib = true;
            let mut fallback = Command::new("python3");
            fallback
                .args(["-I", "-c", include_str!("poppler.py")])
                .arg(path.canonicalize()?)
                .arg("text-boxes")
                .arg(page.to_string());
            super::output(fallback, cancel, 8 << 20)?
        }
        result => result?,
    };
    let xml = String::from_utf8_lossy(&output);
    if glib {
        parse(&xml)
    } else {
        parse_document_pages(&xml, path, page, cancel)?
            .into_iter()
            .next()
            .ok_or_else(|| io::Error::other("PDF text page is missing"))
    }
}

fn parse(xml: &str) -> io::Result<PageText> {
    parse_pages(xml)?
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::other("PDF text page is missing"))
}

pub(crate) fn parse_pages(xml: &str) -> io::Result<Vec<PageText>> {
    parse_rotated_pages(xml, None)
}

pub(crate) fn parse_document_pages(
    xml: &str,
    path: &Path,
    first: u32,
    cancel: &AtomicBool,
) -> io::Result<Vec<PageText>> {
    parse_rotated_pages(xml, Some((path, first, cancel)))
}

fn parse_rotated_pages(
    xml: &str,
    source: Option<(&Path, u32, &AtomicBool)>,
) -> io::Result<Vec<PageText>> {
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(io::Error::other)?;
    let pages: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("page"))
        .collect();
    let rotations = source
        .and_then(|(path, first, cancel)| {
            let mut cmd = Command::new("pdfinfo");
            cmd.args([
                "-f",
                &first.to_string(),
                "-l",
                &(first + pages.len().saturating_sub(1) as u32).to_string(),
            ])
            .arg(path);
            crate::output(cmd, cancel, 1 << 20).ok()
        })
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .lines()
                .filter_map(|line| {
                    let mut parts = line.split_whitespace();
                    if parts.next()? != "Page" {
                        return None;
                    }
                    let page = parts.next()?.parse::<u32>().ok()?;
                    if parts.next()? != "rot:" {
                        return None;
                    }
                    Some((page, parts.next()?.parse::<i32>().ok()?.rem_euclid(360)))
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    pages
        .into_iter()
        .enumerate()
        .map(|(i, page)| {
            parse_page(
                page,
                source
                    .and_then(|(_, first, _)| rotations.get(&(first + i as u32)).copied())
                    .unwrap_or(0),
            )
        })
        .collect()
}

fn parse_page(page: roxmltree::Node<'_, '_>, rotation: i32) -> io::Result<PageText> {
    let number = |node: roxmltree::Node<'_, '_>, name| -> io::Result<f32> {
        node.attribute(name)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .ok_or_else(|| io::Error::other("Invalid PDF text coordinates"))
    };
    let mut width = number(page, "width")?;
    let mut height = number(page, "height")?;
    // pdftotext reports MediaBox dimensions but has already rotated every bbox.
    // Match the rendered sheet before normalizing; clamping first loses words.
    if rotation == 90 || rotation == 270 {
        std::mem::swap(&mut width, &mut height);
    }
    if width <= 0. || height <= 0. {
        return Err(io::Error::other("Empty PDF page"));
    }
    let mut result = PageText {
        size: [width, height],
        ..Default::default()
    };
    let mut line_index = 0;
    for (block, node) in page
        .descendants()
        .filter(|n| n.has_tag_name("block"))
        .enumerate()
    {
        for line in node.children().filter(|n| n.has_tag_name("line")) {
            for word in line.children().filter(|n| n.has_tag_name("word")) {
                let text = word.text().unwrap_or("");
                if text.trim().is_empty() {
                    continue;
                }
                let bounds = [
                    number(word, "xMin")? / width,
                    number(word, "yMin")? / height,
                    number(word, "xMax")? / width,
                    number(word, "yMax")? / height,
                ]
                .map(|v| v.clamp(0., 1.));
                if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
                    continue;
                }
                if let Some(previous) = result.words.last() {
                    result.text.push(if previous.line == line_index {
                        ' '
                    } else {
                        '\n'
                    });
                    if previous.block != block {
                        result.text.push('\n');
                    }
                }
                let start = result.text.len();
                result.text.push_str(text);
                result.words.push(Word {
                    bytes: start..result.text.len(),
                    bounds,
                    line: line_index,
                    block,
                });
            }
            line_index += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_offsets_and_paragraphs_follow_pdf_reading_order() {
        let p = parse(r#"<doc><page width="200" height="400"><flow><block><line>
          <word xMin="20" yMin="40" xMax="60" yMax="60">Привет</word>
          <word xMin="70" yMin="40" xMax="90" yMax="60">世界</word></line>
          <line><word xMin="20" yMin="70" xMax="80" yMax="90">next</word></line></block>
          <block><line><word xMin="20" yMin="100" xMax="80" yMax="120">paragraph</word></line></block>
        </flow></page></doc>"#).unwrap();
        assert_eq!(p.text, "Привет 世界\nnext\n\nparagraph");
        assert_eq!(&p.text[p.words[1].bytes.clone()], "世界");
        assert_eq!(p.words[0].bounds, [0.1, 0.1, 0.3, 0.15]);
        assert_eq!(p.words[2].block, 0);
        assert_eq!(p.words[3].block, 1);
    }
    #[test]
    fn scans_are_empty_and_invalid_geometry_is_rejected() {
        assert!(
            parse(r#"<page width="200" height="300"/>"#)
                .unwrap()
                .words
                .is_empty()
        );
        assert!(parse(r#"<page width="NaN" height="300"/>"#).is_err());
    }
    #[test]
    fn intrinsic_quarter_turns_normalize_before_clipping() {
        for rotation in [0, 90, 180, 270] {
            let (x, y) = if rotation % 180 == 0 {
                (100, 400)
            } else {
                (440, 100)
            };
            let xml = format!(
                r#"<page width="360" height="500"><block><line><word xMin="{x}" yMin="{y}" xMax="{}" yMax="{}">Привет</word></line></block></page>"#,
                x + 15,
                y + 20
            );
            let doc = roxmltree::Document::parse(&xml).unwrap();
            let p = parse_page(doc.root_element(), rotation).unwrap();
            assert_eq!(p.text, "Привет");
            assert_eq!(p.words[0].bytes, 0..12);
            let size = if rotation % 180 == 0 {
                [360., 500.]
            } else {
                [500., 360.]
            };
            assert_eq!(p.size, size);
            assert_eq!(
                p.words[0].bounds,
                [
                    x as f32 / size[0],
                    y as f32 / size[1],
                    (x + 15) as f32 / size[0],
                    (y + 20) as f32 / size[1]
                ]
            );
        }
    }
}
