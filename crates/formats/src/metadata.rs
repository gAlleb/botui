//! Catalog metadata without opening the spine or decoding embedded images.
use crate::{FileKind, Result, epub, invalid, xhtml, xml_encoding};
use roxmltree::{Document, Node};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

#[derive(Clone, Default, Debug)]
pub struct BookMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub series: Vec<String>,
}
pub fn book_metadata(path: &Path) -> Result<BookMetadata> {
    match FileKind::from_path(path) {
        Some(FileKind::Epub) => {
            let mut archive = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
            let mut remaining = 4 << 20;
            let container =
                epub::read_member(&mut archive, "META-INF/container.xml", &mut remaining)?;
            let xml = Document::parse(&container)?;
            let opf = xml
                .descendants()
                .find(|n| n.has_tag_name("rootfile"))
                .and_then(|n| n.attribute("full-path"))
                .ok_or_else(|| invalid("Missing EPUB rootfile"))?;
            let package =
                epub::read_member(&mut archive, &epub::member_path("", opf)?, &mut remaining)?;
            let xml = Document::parse(&package)?;
            let meta = xml
                .root_element()
                .children()
                .find(|n| n.has_tag_name("metadata"))
                .ok_or_else(|| invalid("Missing EPUB metadata"))?;
            let value = |n: Node<'_, '_>| {
                xhtml::clean(
                    &n.descendants()
                        .filter(|n| n.is_text())
                        .filter_map(|n| n.text())
                        .collect::<String>(),
                )
            };
            let mut m = BookMetadata::default();
            for n in meta.children().filter(|n| n.is_element()) {
                match n.tag_name().name() {
                    "title" if m.title.is_empty() => m.title = value(n),
                    "creator" => m.authors.push(value(n)),
                    "meta" if n.attribute("name") == Some("calibre:series") => m
                        .series
                        .push(n.attribute("content").unwrap_or_default().into()),
                    "meta" if n.attribute("property") == Some("belongs-to-collection") => {
                        m.series.push(value(n))
                    }
                    _ => (),
                }
            }
            Ok(m)
        }
        Some(FileKind::Fb2) => fb2(File::open(path)?),
        _ => Ok(BookMetadata::default()),
    }
}

fn fb2(input: impl Read) -> Result<BookMetadata> {
    use xml::reader::{ParserConfig, XmlEvent};
    let mut buf = BufReader::new(input.take(1 << 20));
    let (enc, _) = xml_encoding(buf.fill_buf()?)?;
    let stream = encoding_rs_io::DecodeReaderBytesBuilder::new()
        .encoding(Some(enc))
        .strip_bom(true)
        .build(buf);
    let parser = ParserConfig::new()
        .override_encoding(Some(xml::Encoding::Utf8))
        .ignore_invalid_encoding_declarations(true)
        .max_entity_expansion_length(2048)
        .create_reader(stream);
    let mut m = BookMetadata::default();
    let mut stack = Vec::<String>::new();
    let mut name = Vec::new();
    let mut nickname = String::new();
    let mut field = String::new();
    for e in parser {
        match e? {
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                stack.push(name.local_name);
                if stack.len() == 2 && stack[1] == "body" {
                    break;
                }
                if stack.len() >= 3 && stack[2] == "title-info" {
                    match stack.last().map(String::as_str) {
                        Some("sequence") => {
                            if let Some(a) = attributes.iter().find(|a| a.name.local_name == "name")
                            {
                                m.series.push(a.value.clone())
                            }
                        }
                        Some(
                            "book-title" | "first-name" | "middle-name" | "last-name" | "nickname",
                        ) => field.clear(),
                        _ => (),
                    }
                }
            }
            XmlEvent::Characters(s) | XmlEvent::CData(s) | XmlEvent::Whitespace(s)
                if stack.len() >= 4 && stack[2] == "title-info" && field.len() < 8192 =>
            {
                field.push_str(&s)
            }
            XmlEvent::EndElement { .. } => {
                if stack.len() >= 3
                    && stack[0] == "FictionBook"
                    && stack[1] == "description"
                    && stack[2] == "title-info"
                {
                    match stack.last().map(String::as_str) {
                        Some("book-title") => m.title = xhtml::clean(&field),
                        Some("first-name" | "middle-name" | "last-name")
                            if stack.get(3).is_some_and(|s| s == "author") =>
                        {
                            name.push(xhtml::clean(&field))
                        }
                        Some("nickname") if stack.get(3).is_some_and(|s| s == "author") => {
                            nickname = xhtml::clean(&field)
                        }
                        Some("author") => {
                            let full = xhtml::clean(&name.join(" "));
                            m.authors.push(if full.is_empty() {
                                std::mem::take(&mut nickname)
                            } else {
                                full
                            });
                            name.clear();
                            nickname.clear()
                        }
                        Some("title-info") => break,
                        _ => (),
                    }
                }
                stack.pop();
            }
            _ => (),
        }
    }
    Ok(m)
}
