use super::epub::{member_path, read_book};
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::SimpleFileOptions};

fn archive(items: &[(&str, &str)]) -> Cursor<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in items {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap()
}

#[test]
fn imports_spine_in_order_without_front_matter_or_note_bodies() {
    let container = r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="Book/package.opf"/></rootfiles></container>"#;
    let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/">
        <metadata><dc:title>  Волны &amp; берег </dc:title></metadata>
        <manifest>
            <item id="last" href="../Text/two.xhtml" media-type="application/xhtml+xml"/>
            <item id="first" href="../Text/one%20chapter.xhtml" media-type="application/xhtml+xml"/>
            <item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/>
        </manifest>
        <spine><itemref idref="first"/><itemref idref="notes" linear="no"/><itemref idref="last"/></spine>
    </package>"#;
    let book = read_book(archive(&[
        ("META-INF/container.xml", container),
        ("Book/package.opf", opf),
        ("Text/two.xhtml", "<p>Последняя глава</p>"),
        ("Text/one chapter.xhtml", "<!DOCTYPE html><html><head><title>Окно</title></head><body><h1>Глава <b>1</b></h1><p>Нача<em>ло</em>&nbsp;книги. <span hidden>лишнее</span><script>alert(1)</script></p><p>Следующая строка.</p></body></html>"),
    ]), "fallback").unwrap();
    assert_eq!(book.title, "Волны & берег");
    let lines: Vec<_> = book
        .blocks
        .iter()
        .map(|b| (b.heading, b.text.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            (true, "Глава 1"),
            (false, "Начало книги."),
            (false, "Следующая строка."),
            (false, "Последняя глава"),
        ]
    );
}

#[test]
fn archive_references_stay_inside_the_document() {
    assert_eq!(
        member_path("OEBPS/Books/main.opf", "../chapter%201.xhtml#p2").unwrap(),
        "OEBPS/chapter 1.xhtml"
    );
    for href in [
        "../../../outside",
        "%2fetc/passwd",
        "https://example.org/ch",
        "x%5cy",
    ] {
        assert!(member_path("OEBPS/Books/main.opf", href).is_err(), "{href}");
    }
}

#[test]
fn fb2_decodes_cyrillic_and_only_displays_reading_bodies() {
    let xml = r#"<?xml version="1.0" encoding="windows-1251"?>
    <FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0">
    <description><title-info><book-title>Книга стихов</book-title></title-info></description>
    <body><section><title><p>Первая глава</p></title>
    <epigraph><p>Воздух и свет.</p></epigraph>
    <poem><stanza><v>По реке плывёт <emphasis>листва</emphasis>.</v></stanza></poem></section></body>
    <body name="notes"><section><p>Текст сноски</p></section></body>
    <binary id="cover">AAAA</binary></FictionBook>"#;
    let (bytes, _, bad) = encoding_rs::WINDOWS_1251.encode(xml);
    assert!(!bad);
    let book = super::fb2::parse(&bytes, "").unwrap();
    assert_eq!(book.title, "Книга стихов");
    let blocks: Vec<_> = book
        .blocks
        .iter()
        .map(|b| (b.heading, b.text.as_str()))
        .collect();
    assert_eq!(
        blocks,
        [
            (true, "Первая глава"),
            (false, "Воздух и свет."),
            (false, "По реке плывёт листва.")
        ]
    );
}

#[test]
fn bookmarks_resolve_element_or_range_starts_without_guessing_another_chapter() {
    let book = super::fb2::parse(br#"<FictionBook><body><section><title><p>Chapter</p></title><p>Amber at dawn.</p></section></body></FictionBook>"#, "").unwrap();
    let a = botui_core::Anchor { block: 1, byte: 0 };
    let cfi = book.cfi(a).expect("a mapped FB2 paragraph");
    let path = cfi
        .strip_prefix("epubcfi(")
        .unwrap()
        .strip_suffix(":0)")
        .unwrap();
    let (element, node) = path.rsplit_once('/').unwrap();
    assert_eq!(
        book.bookmark_anchor(&format!("epubcfi({element})")),
        Some(a)
    );
    assert_eq!(book.bookmark_anchor(&format!("epubcfi({path})")), Some(a));
    assert_eq!(
        book.bookmark_anchor(&format!("epubcfi({element}[id^,1],/{node}:2,/{node}:7)")),
        Some(botui_core::Anchor { byte: 2, ..a })
    );
    assert_eq!(book.bookmark_anchor("epubcfi(/9998/12)"), None);
}
