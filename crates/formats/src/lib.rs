//! Read a local book as text, independent of its catalog and of the terminal.

mod cover;
mod metadata;
pub use metadata::{BookMetadata, book_metadata};
mod epub;
pub use cover::cover;
mod fb2;
mod locations;
mod resources;
pub use resources::{Supplement, illustration, supplement, supplementary_note};
mod publisher;
#[cfg(test)]
mod tests;
mod tex;
mod toc;
mod xhtml;

use botui_core::Document;
use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};
pub use epub::open_epub;
use std::{borrow::Cow, error::Error, io, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_TEXT_BYTES: u64 = 128 << 20;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Catalog visibility is independent of whether this build can render the format.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Epub,
    Fb2,
    Pdf,
    Djvu,
}

impl FileKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        Self::from_extension(path.extension()?.to_str()?)
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        [
            ("epub", Self::Epub),
            ("fb2", Self::Fb2),
            ("pdf", Self::Pdf),
            ("djvu", Self::Djvu),
            ("djv", Self::Djvu),
        ]
        .into_iter()
        .find_map(|(suffix, kind)| ext.eq_ignore_ascii_case(suffix).then_some(kind))
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Epub => "EPUB",
            Self::Fb2 => "FB2",
            Self::Pdf => "PDF",
            Self::Djvu => "DjVu",
        }
    }
    pub fn readable(self) -> bool {
        matches!(self, Self::Epub | Self::Fb2)
    }
}

pub fn supports_path(path: &Path) -> bool {
    FileKind::from_path(path).is_some_and(FileKind::readable)
}

pub fn open_document(path: &Path) -> Result<Document> {
    match FileKind::from_path(path) {
        Some(FileKind::Epub) => open_epub(path),
        Some(FileKind::Fb2) => fb2::open(path),
        Some(kind) => Err(invalid(format!("No built-in viewer for {}", kind.label())).into()),
        None => Err(invalid("Choose an EPUB or FB2 file").into()),
    }
}

// XML declares legacy encodings in an ASCII prolog. A BOM takes precedence.
fn xml_encoding(bytes: &[u8]) -> Result<(&'static Encoding, usize)> {
    let (encoding, skip) = if let Some(pair) = Encoding::for_bom(bytes) {
        pair
    } else if bytes.starts_with(&[0, b'<', 0, b'?']) {
        (UTF_16BE, 0)
    } else if bytes.starts_with(&[b'<', 0, b'?', 0]) {
        (UTF_16LE, 0)
    } else {
        let declared = bytes
            .strip_prefix(b"<?xml")
            .and_then(|p| {
                p.windows(2)
                    .position(|w| w == b"?>")
                    .and_then(|i| std::str::from_utf8(&p[..i]).ok())
            })
            .and_then(|p| p.split_once("encoding"))
            .and_then(|(_, rest)| rest.trim_start().strip_prefix('='))
            .and_then(|rest| {
                let rest = rest.trim_start();
                let quote = rest.chars().next().filter(|q| matches!(q, '\'' | '"'))?;
                rest[1..].split_once(quote).map(|(label, _)| label)
            });
        let encoding = match declared {
            Some(label) => Encoding::for_label(label.as_bytes())
                .ok_or_else(|| invalid(format!("Unknown XML encoding {label:?}")))?,
            None => UTF_8,
        };
        (encoding, 0)
    };
    Ok((encoding, skip))
}

fn decode_xml(bytes: &[u8]) -> Result<Cow<'_, str>> {
    let (encoding, skip) = xml_encoding(bytes)?;
    encoding
        .decode_without_bom_handling_and_without_replacement(&bytes[skip..])
        .ok_or_else(|| {
            invalid(format!(
                "Book text is damaged or does not match encoding {}",
                encoding.name()
            ))
            .into()
        })
}

/// Display untrusted catalog HTML as plain text; do not request linked resources.
pub fn synopsis(input: &str) -> String {
    if !input.contains('<') {
        return xhtml::clean(input);
    }
    let mut blocks = vec![];
    xhtml::append_blocks(
        &scraper::Html::parse_document(input),
        &mut blocks,
        &mut vec![],
        &mut Default::default(),
    );
    blocks
        .into_iter()
        .map(|b| b.text)
        .collect::<Vec<_>>()
        .join("\n")
}
