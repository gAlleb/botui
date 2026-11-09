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
    assert!(book.image_pages.is_empty());
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
fn empty_xhtml_title_keeps_chapter_text_and_its_navigation_target() {
    let book = read_book(archive(&[
        ("META-INF/container.xml", r#"<container><rootfile full-path="book.opf"/></container>"#),
        ("book.opf", r#"<package><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="chapter"/></spine></package>"#),
        ("chapter.xhtml", r#"<html><head><title /></head><body><p class="empty"/><p id="start">First breath.</p><script/><p>Second breath.</p></body></html>"#),
        ("nav.xhtml", r##"<html><head><title/></head><body><nav role="doc-toc"><a href="chapter.xhtml#start">Practice</a></nav></body></html>"##),
    ]), "test").unwrap();
    assert_eq!(
        book.blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>(),
        ["First breath.", "Second breath."]
    );
    let anchor = botui_core::Anchor { block: 0, byte: 0 };
    assert_eq!(book.contents.len(), 1);
    assert_eq!(book.contents[0].anchor(), Some(anchor));
    assert_eq!(book.contents[0].title, "Practice");
    assert_eq!(book.resolve_cfi(&book.cfi(anchor).unwrap()), Some(anchor));
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

#[test]
fn web_progress_resumes_on_printed_text_across_wraps_and_normalized_whitespace() {
    use botui_core::{Anchor, LayoutMode, Pager};
    let book = super::fb2::parse("<FictionBook><body><section><p>  Amber 🌄   at dawn.</p><p>Another day.</p></section></body></FictionBook>".as_bytes(), "").unwrap();
    let path = "/6/2!/4/2/2/1";
    let mut pager = Pager::new(&book, 5, 2, LayoutMode::Single);
    for (offset, block, byte, word) in [
        (0, 0, 0, "Amber"),
        (12, 0, "Amber 🌄 ".len(), "at"),
        (10, 0, "Amber 🌄 ".len(), "at"),
        (11, 0, "Amber 🌄 ".len(), "at"),
        (17, 0, "Amber 🌄 at d".len(), "dawn."),
        (21, 1, 0, "Anoth"),
    ] {
        let cfi = format!("epubcfi({path}:{offset})");
        let at = book.reading_anchor(&cfi).unwrap();
        assert_eq!(at, Anchor { block, byte }, "{cfi}");
        pager.seek(at);
        assert!(pager.lines(0)[0].text.contains(word), "{cfi}");
        assert_eq!(book.reading_anchor(&book.cfi(at).unwrap()), Some(at));
    }
    assert!(
        book.reading_anchor(&format!("epubcfi({path}:999)"))
            .is_none()
    );
}

#[test]
fn a_repeated_epub_passage_has_its_own_file_address() {
    let html = "<html><head/><body><p>Again.</p><p hidden='hidden'>Again.</p><p>Third line.</p><p>Again.</p></body></html>";
    let mut d = botui_core::Document::default();
    super::xhtml::append_blocks(
        &super::xhtml::parse_document(html),
        &mut d.blocks,
        &mut vec![],
        &mut d.preformatted,
        &mut d.tables,
    );
    super::locations::epub(html, "/6/2", 0, &d.blocks, &mut d.locations);
    for (block, step) in [(0, 2), (2, 8)] {
        let a = botui_core::Anchor { block, byte: 2 };
        let addr = format!("epubcfi(/6/2!/4/{step}/1:2)");
        assert_eq!(d.cfi(a), Some(addr.clone()));
        assert_eq!(d.bookmark_anchor(&addr), Some(a));
    }
}

#[test]
fn image_only_epub_keeps_two_spine_destinations_and_ignores_remote_art() {
    let book=read_book(archive(&[
        ("META-INF/container.xml",r#"<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>"#),
        ("book.opf",r#"<package><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/><item id="nav" properties="nav" href="toc.xhtml" media-type="application/xhtml+xml"/><item id="ill" href="i.jpg" media-type="image/jpeg"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#),
        ("a.xhtml",r#"<html><body><img src="https://example.org/out.jpg"/><svg><image href="i.jpg"/></svg></body></html>"#),
        ("b.xhtml",r#"<html><body><img alt="Last plate" src="i.jpg"/></body></html>"#),
        ("toc.xhtml",r#"<html><body><nav epub:type="toc"><ol><li><a href="a.xhtml">First</a></li><li><a href="b.xhtml">Last</a></li></ol></nav></body></html>"#),
    ]),"album").unwrap();
    assert_eq!(book.image_pages, ["i.jpg", "i.jpg"]);
    assert_eq!(book.blocks.len(), 2);
    assert_eq!(book.blocks[1].text, "Last plate");
    assert_eq!(
        book.contents
            .iter()
            .map(|e| e.anchor().unwrap().block)
            .collect::<Vec<_>>(),
        [0, 1]
    );
}

#[test]
fn navigation_document_can_link_to_its_own_fragment_in_the_spine() {
    let book = read_book(archive(&[
        ("META-INF/container.xml", r#"<container><rootfile full-path="OPS/p.opf"/></container>"#),
        ("OPS/p.opf", r#"<package><manifest><item id="n" href="chap.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="n"/></spine></package>"#),
        ("OPS/chap.xhtml",r##"<html><body><nav role="doc-toc"><ol><li><a href="#p%202">The coast</a></li></ol></nav><section id="p 2"><p>Far from the harbor.</p></section></body></html>"##),
    ]), "test").unwrap();
    assert_eq!(book.contents.len(), 1);
    let anchor = book.contents[0].anchor().unwrap();
    assert_eq!(book.contents[0].title, "The coast");
    assert_eq!(book.blocks[anchor.block].text, "Far from the harbor.");
}

#[test]
fn fb2_contents_group_section_titles_and_inline_fragment_keeps_its_paragraph() {
    let src = r##"<FictionBook><body><section><title><p>Book I</p><p>The Shore</p></title>
        <subtitle>A decorative subtitle</subtitle>
        <p>See <a href="#destination">the coast</a>.</p>
        <section><title><p>Part 1</p></title><p>The <emphasis id="destination">coast</emphasis> is rough.</p><p>Another day.</p></section>
        </section></body></FictionBook>"##;
    let book = super::fb2::parse(src.as_bytes(), "example").unwrap();
    let names: Vec<_> = book.contents.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(names, ["Book I The Shore", "Part 1"]);
    assert_eq!(book.contents[1].parent, Some(0));
    let botui_core::Target::Passage(place) = book.references[0].target else {
        panic!("missing internal link")
    };
    assert_eq!(book.blocks[place.block].text, "The coast is rough.");
}

#[test]
fn nested_contents_keep_duplicate_chapter_names_in_their_own_books() {
    let chapters = r###"<html><body><h1 id="a">A</h1><p id="first">Text A</p><h1 id="b">B</h1><p id="second">Text B</p></body></html>"###;
    let nav = r###"<nav role="doc-toc"><ol><li><a href="c#a">A</a><ol><li><a href="c#first">One</a></li></ol></li><li><span>B</span><ol><li><a href="c#second">One</a></li></ol></li><li><a href="notes#n">Notes</a></li></ol></nav>"###;
    let ncx = r###"<ncx><navMap><navPoint><navLabel><text>A</text></navLabel><content src="c#a"/><navPoint><navLabel><text>One</text></navLabel><content src="c#first"/></navPoint></navPoint><navPoint><navLabel><text>B</text></navLabel><content src="c#b"/><navPoint><navLabel><text>One</text></navLabel><content src="c#second"/></navPoint></navPoint><navPoint><navLabel><text>Notes</text></navLabel><content src="notes#n"/></navPoint></navMap></ncx>"###;
    for (index, mime, props) in [
        (nav, "application/xhtml+xml", "properties='nav'"),
        (ncx, "application/x-dtbncx+xml", ""),
    ] {
        let package = format!(
            "<package><manifest><item id='c' href='c' media-type='application/xhtml+xml'/><item id='toc' href='toc' media-type='{mime}' {props}/><item id='n' href='notes' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='c'/><itemref idref='n' linear='no'/></spine></package>"
        );
        let book = read_book(
            archive(&[
                (
                    "META-INF/container.xml",
                    "<container><rootfile full-path='p'/></container>",
                ),
                ("p", &package),
                ("c", chapters),
                ("toc", index),
                ("notes", "<p id='n'>Commentary</p>"),
            ]),
            "test",
        )
        .unwrap();
        assert_eq!(
            book.contents
                .iter()
                .map(|e| (e.title.as_str(), e.parent))
                .collect::<Vec<_>>(),
            [
                ("A", None),
                ("One", Some(0)),
                ("B", None),
                ("One", Some(2)),
                ("Notes", None)
            ]
        );
        assert_eq!(book.contents[1].anchor().unwrap().block, 1);
        assert_eq!(book.contents[3].anchor().unwrap().block, 3);
        assert!(
            matches!(&book.contents[4].target, botui_core::Target::Supplement { member, fragment, .. } if member == "notes" && fragment == "n")
        );
    }
}

#[test]
fn technical_epub_preserves_spans_equations_and_preformatted_lines() {
    let mut book = botui_core::Document::default();
    let dom = super::xhtml::parse_document(
        r#"<html><body>
        <table><tr><th rowspan="2">Group</th><th colspan="2">Range</th></tr>
        <tr><td>Low</td><td>High</td></tr></table>
        <p><math><semantics><mfrac><mi>a</mi><msup><mi>b</mi><mn>2</mn></msup></mfrac>
        <annotation encoding="application/x-tex">duplicate</annotation></semantics></math></p>
        <pre>first
    second</pre><p>After</p></body></html>"#,
    );
    super::xhtml::append_blocks(
        &dom,
        &mut book.blocks,
        &mut vec![],
        &mut book.preformatted,
        &mut book.tables,
    );
    let mut rows = book.tables.values();
    let header = rows.next().unwrap();
    assert_eq!((header.columns, &header.spans[..]), (3, &[1, 2][..]));
    let second = rows.next().unwrap();
    assert_eq!(second.cells, ["", "Low", "High"]);
    assert!(book.blocks.iter().any(|b| b.text == "(a)/(b^(2))"));
    let code = *book.preformatted.first().unwrap();
    assert_eq!(book.blocks[code].text, "first\n    second");
    assert_eq!(book.blocks[code + 1].text, "After");
}
