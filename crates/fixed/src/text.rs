//! The embedded PDF text, in reading order, with page-relative word boxes.
//! Extract only requested pages; neither rasterization nor OCR is involved.
use std::{io, ops::Range, path::Path, process::Command, sync::atomic::AtomicBool};

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
}

pub fn page_text(path: &Path, page: u32, cancel: &AtomicBool) -> io::Result<PageText> {
    if page == 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut cmd = Command::new("pdftotext");
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
    let output = super::output(cmd, cancel, 8 << 20)?;
    parse(&String::from_utf8_lossy(&output))
}

fn parse(xml: &str) -> io::Result<PageText> {
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(io::Error::other)?;
    let page = doc
        .descendants()
        .find(|n| n.has_tag_name("page"))
        .ok_or_else(|| io::Error::other("PDF text page is missing"))?;
    let number = |node: roxmltree::Node<'_, '_>, name| -> io::Result<f32> {
        node.attribute(name)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .ok_or_else(|| io::Error::other("Invalid PDF text coordinates"))
    };
    let width = number(page, "width")?;
    let height = number(page, "height")?;
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
}
