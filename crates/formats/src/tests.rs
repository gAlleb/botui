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
fn selected_epub_cells_keep_source_ranges_through_css_and_column_changes() {
    use botui_core::{LayoutMode, Pager};
    use unicode_width::UnicodeWidthStr;
    let doc = read_book(archive(&[
        ("META-INF/container.xml", "<container><rootfile full-path='book.opf'/></container>"),
        ("book.opf", "<package><manifest><item id='c' href='c.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='c'/></spine></package>"),
        ("c.xhtml", "<html><head><style>p { text-align:justify; text-indent:2em; margin:1em 0 } em { font-style:italic }</style></head><body><p>Точный <em>курсив &amp; цитата</em> после него обычные слова, пробелы и переносы. Далее 中文 и 🦉.</p><p>Второй абзац с цитатой.</p></body></html>"),
    ]), "selection").unwrap();
    for width in [32, 50, 90] {
        let pager = Pager::new(&doc, width, 80, LayoutMode::Single);
        let mut quoted = String::new();
        for line in pager.lines(0) {
            for cell in line.text_cells(&doc) {
                let source = doc.text_range(cell.source);
                let displayed: String = line
                    .text
                    .chars()
                    .scan(0, |col, c| {
                        let at = *col;
                        *col += c.to_string().width();
                        Some((at, c))
                    })
                    .filter(|(at, _)| cell.columns.contains(at))
                    .map(|(_, c)| c)
                    .collect();
                assert_eq!(source, displayed, "width {width}: {}", line.text);
                quoted.push_str(&source);
            }
        }
        for word in ["Точный", "курсив", "цитата", "Второй", "中文", "🦉"]
        {
            assert!(
                quoted.contains(word),
                "{word} missing at width {width}: {quoted}"
            );
        }
    }
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
fn a_spine_initial_printed_number_stays_with_its_named_chapter() {
    use botui_core::Anchor;
    let doc = read_book(archive(&[
        ("META-INF/container.xml", "<container><rootfile full-path='p'/></container>"),
        ("p", "<package><manifest><item id='a' href='a' media-type='application/xhtml+xml'/><item id='b' href='b' media-type='application/xhtml+xml'/><item id='c' href='c' media-type='application/xhtml+xml'/><item id='d' href='d' media-type='application/xhtml+xml'/><item id='nav' href='nav' media-type='application/xhtml+xml' properties='nav'/></manifest><spine><itemref idref='a'/><itemref idref='b'/><itemref idref='c'/><itemref idref='d'/></spine></package>"),
        ("a", "<html><body><p>2</p></body></html>"),
        ("b", "<html><body><h1 id='b'>BASIC PRINCIPLES</h1><p>End.</p></body></html>"),
        ("c", "<html><body><p>3</p><h1 id='c'>ANATOMY</h1><p>Body.</p></body></html>"),
        ("d", "<html><body><p>45</p><h1 id='d'>ROUTINE</h1></body></html>"),
        ("nav", "<html><body><nav role='doc-toc'><a href='b#b'>2. Basic Principles</a><a href='c#c'>3. Anatomy</a><a href='d#d'>4. Routine</a></nav></body></html>"),
    ]), "Numbered chapters").unwrap();
    let block = |text| doc.blocks.iter().position(|b| b.text == text).unwrap();
    assert!(doc.chapter_starts.contains(&block("3")));
    assert!(!doc.chapter_starts.contains(&block("ANATOMY")));
    assert!(doc.chapter_starts.contains(&block("BASIC PRINCIPLES")));
    assert!(
        !doc.chapter_starts.contains(&block("2")),
        "do not pull in the previous member"
    );
    assert!(doc.chapter_starts.contains(&block("ROUTINE")));
    assert!(
        !doc.chapter_starts.contains(&block("45")),
        "different leading number"
    );
    let target = Anchor {
        block: block("ANATOMY"),
        byte: 0,
    };
    assert_eq!(
        doc.contents
            .iter()
            .find(|e| e.title == "3. Anatomy")
            .unwrap()
            .anchor(),
        Some(target)
    );
    assert_eq!(doc.resolve_cfi(&doc.cfi(target).unwrap()), Some(target));
}

#[test]
fn epub_spine_ordinals_and_semantic_chapters_have_independent_boundaries() {
    use botui_core::{Anchor, LayoutMode, Pager};
    let manifest: String = [
        "cover", "intro", "empty", "chapters", "continue", "final", "notes",
    ]
    .iter()
    .map(|id| format!("<item id='{id}' href='{id}' media-type='application/xhtml+xml'/>"))
    .collect();
    let opf = format!(
        "<package><manifest>{manifest}</manifest><spine>
        <itemref idref='cover' linear='no'/><itemref idref='intro'/><itemref idref='empty'/>
        <itemref idref='chapters'/><itemref idref='continue'/><itemref idref='final'/>
        <itemref idref='notes' linear='no'/></spine></package>"
    );
    let doc = read_book(
        archive(&[
            (
                "META-INF/container.xml",
                "<container><rootfile full-path='book.opf'/></container>",
            ),
            ("book.opf", &opf),
            ("cover", "<body><p>Cover</p></body>"),
            ("notes", "<body><p>Non-linear notes</p></body>"),
            ("intro", "<body><p>1111</p><p>2222</p></body>"),
            ("empty", "<body></body>"),
            (
                "chapters",
                "<body><h1>One</h1><p>Text</p><h1>Two</h1><p>2222</p></body>",
            ),
            ("continue", "<body><p>3333</p></body>"),
            ("final", "<body><h1>Three</h1><p>End</p></body>"),
        ]),
        "Book",
    )
    .unwrap();
    let mut pager = Pager::new(&doc, 90, 6, LayoutMode::Single);
    for (block, ordinal, title, percent) in [
        (0, 2, None, 0),
        (1, 2, None, 50),
        (2, 4, Some("One"), 0),
        (4, 4, Some("Two"), 0),
        (6, 5, Some("Two"), 63),
        (7, 6, Some("Three"), 0),
    ] {
        pager.seek(Anchor { block, byte: 0 });
        let progress = pager.chapter_progress().unwrap();
        assert_eq!(
            (
                progress.ordinal,
                progress.total,
                progress.title,
                progress.percent
            ),
            (ordinal, 7, title, Some(percent)),
            "source block {block}"
        );
    }
}

#[test]
fn standalone_picture_spine_uses_its_own_ordinal_at_the_preceding_text_anchor() {
    use botui_core::{Anchor, IllustrationPosition, LayoutMode, Pager, ViewPosition};
    let doc = read_book(archive(&[
        ("META-INF/container.xml", "<container><rootfile full-path='p'/></container>"),
        ("p", "<package><manifest><item id='a' href='a' media-type='application/xhtml+xml'/><item id='plate' href='plate' media-type='application/xhtml+xml'/><item id='b' href='b' media-type='application/xhtml+xml'/><item id='img' href='i.jpg' media-type='image/jpeg'/></manifest><spine><itemref idref='a'/><itemref idref='plate'/><itemref idref='b'/></spine></package>"),
        ("a", "<body><p>Before.</p></body>"),
        ("plate", "<body><img src='i.jpg'/></body>"),
        ("b", "<body><p>After.</p></body>"),
    ]), "Album with text").unwrap();
    let mut pager = Pager::new(&doc, 70, 9, LayoutMode::Single);
    let body = Anchor { block: 0, byte: 7 };
    pager.restore_position(ViewPosition {
        anchor: body,
        illustration: Some(IllustrationPosition {
            reference: 0,
            row: 0,
        }),
    });
    let progress = pager.chapter_progress().unwrap();
    assert_eq!(
        (progress.ordinal, progress.total, progress.percent),
        (2, 3, None)
    );
    assert_eq!(
        pager.anchor(),
        body,
        "the saved reading address stays in the text"
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
        ("book.opf",r#"<package><manifest><item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/><item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/><item id="nav" properties="nav" href="toc.xhtml" media-type="application/xhtml+xml"/><item id="ill" href="i.jpg" media-type="image/jpeg"/></manifest><spine><itemref idref="cover" linear="no"/><itemref idref="a"/><itemref idref="b"/><itemref idref="notes" linear="no"/></spine></package>"#),
        ("cover.xhtml","<body><img src='i.jpg'/></body>"),
        ("notes.xhtml","<body><p>Non-linear notes</p></body>"),
        ("a.xhtml",r#"<html><body><img src="https://example.org/out.jpg"/><svg><image href="i.jpg"/></svg></body></html>"#),
        ("b.xhtml",r#"<html><body><img src="missing.jpg"/><img src="i.jpg"/><img alt="Last plate" src="i.jpg"/></body></html>"#),
        ("toc.xhtml",r#"<html><body><nav epub:type="toc"><ol><li><a href="a.xhtml">First</a></li><li><a href="b.xhtml">Last</a></li></ol></nav></body></html>"#),
    ]),"album").unwrap();
    assert_eq!(
        book.image_pages,
        ["botui-inline-svg:0:a.xhtml", "i.jpg", "i.jpg"]
    );
    assert_eq!(book.blocks.len(), 3);
    for (page, ordinal, range) in [(0, 2, 0..1), (1, 3, 1..3), (2, 3, 1..3)] {
        let item = book.epub_spine.position_at_block(page).unwrap();
        assert_eq!((item.ordinal, item.total, item.blocks), (ordinal, 4, range));
    }
    assert_eq!(book.blocks[2].text, "Last plate");
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
fn reads_styled_epub_with_math_and_spanning_table_in_ordinary_pager() {
    use botui_core::{LayoutMode, Pager};
    let doc = read_book(archive(&[
        ("META-INF/container.xml", r#"<container><rootfile full-path="O/book.opf"/></container>"#),
        ("O/book.opf", r#"<package><manifest><item id="a" href="Text/a.xhtml" media-type="application/xhtml+xml"/><item id="css" href="Styles/layout.css" media-type="text/css"/><item id="more" href="Styles/more.css" media-type="text/css"/></manifest><spine><itemref idref="a"/></spine></package>"#),
        ("O/Styles/layout.css", "@import url(more.css?v=1); .hidden { display:none } .pair { display:grid; grid-template-columns: 12ch 1fr }"),
        ("O/Styles/more.css", "@import url(layout.css); .elsewhere {display:none} @media (max-width: 300px) { .pair {margin-left:2ch} .wide {display:none} }"),
        ("O/Text/a.xhtml", r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Technical</title><link rel="stylesheet" href="missing.css"/><link rel="stylesheet" href="https://example.invalid/remote.css"/><link rel="stylesheet" href="../Styles/layout.css?edition=2#sheet"/></head><body><p class="hidden">INVISIBLE</p><p class="elsewhere">IMPORTED</p><section class="pair"><div>Left</div><div>Right</div></section><table><tr><th rowspan="2">Group</th><th colspan="2">Range</th></tr><tr><td>Low</td><td>High</td></tr><tr><td>Name</td><td colspan="2">Long cell with multiple words including κείμενο and 重 used when checking the exact continuation position inside this cell.</td></tr></table><math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><semantics><mfrac><mi>num</mi><mi>den</mi></mfrac><annotation encoding="application/x-tex">HIDDEN-TEX</annotation></semantics></math><pre>first
    second</pre><p class="wide">END-MARK</p></body></html>"#),
    ]), "technical").unwrap();
    let mut pager = Pager::new(&doc, 50, 150, LayoutMode::Single);
    let lines: Vec<_> = pager.lines(0).iter().map(|l| l.text.trim_end()).collect();
    let page = lines.join("\n");

    assert!(
        !page.contains("INVISIBLE") && !page.contains("IMPORTED") && !page.contains("HIDDEN-TEX"),
        "{page}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Left") && l.find("Right") == Some(12))
    );
    let cells = lines
        .iter()
        .find(|l| l.contains("Low") && l.contains("High"))
        .unwrap();
    assert!(cells.find("Low").unwrap() > 0);
    let num = lines.iter().position(|l| l.contains("num")).unwrap();
    let den = lines.iter().position(|l| l.contains("den")).unwrap();
    assert!(den > num + 1 && lines[num + 1..den].iter().any(|l| l.contains('─')));
    assert!(lines.iter().any(|l| l.starts_with("    second")));
    let index = pager.search_index(&doc);
    for hidden in ["INVISIBLE", "IMPORTED", "HIDDEN-TEX"] {
        assert!(index.matches(hidden).is_empty(), "{hidden}");
    }
    let hit = index.matches("exact continuation").pop().unwrap();
    let cfi = doc.cfi(hit.anchor).unwrap();
    assert_eq!(doc.resolve_cfi(&cfi), Some(hit.anchor));
    for width in [50, 29, 79] {
        pager.relayout(&doc, width, 7, LayoutMode::Single);
        assert_eq!(
            pager.search_index(&doc).matches("END-MARK").is_empty(),
            width == 29
        );
        pager.seek(doc.resolve_cfi(&cfi).unwrap());
        let line = &pager.lines(0)[0];
        assert!(line.text.contains("exact"), "{width}: {}", line.text);
        assert!(index.visible_range(&hit, line).is_some(), "{}", line.text);
    }
}

fn styled_fixture(body: &str, css: &str) -> botui_core::Document {
    let html = format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Read</title><style>{css}</style></head><body>{body}</body></html>"#
    );
    read_book(archive(&[
        ("META-INF/container.xml","<container><rootfile full-path='p'/></container>"),
        ("p", "<package><manifest><item id='text' href='chapter' media-type='application/xhtml+xml'/><item id='pic' href='fig.png' media-type='image/png'/></manifest><spine><itemref idref='text'/></spine></package>"),
        ("chapter", &html)
    ]),"Test book").unwrap()
}

#[test]
fn epub_table_dividers_follow_spanned_cells_and_preserve_source_addresses() {
    use botui_core::{Anchor, LayoutMode, Pager, Surface, TextRange};
    let doc = styled_fixture(
        "<table><tr><td><i>Name</i></td><td><i>Left</i></td><td><i>Right</i></td></tr>\
         <tr><td>&#160;</td></tr>\
         <tr><td rowspan='2'>Frame</td><td>Thin</td><td>Broad</td></tr>\
         <tr><td>Slight</td><td>Heavy</td></tr>\
         <tr><td>Weight</td><td colspan='2'>Combined</td><td>&#160;</td></tr></table>",
        "table {width:100%}",
    );
    let mut pager = Pager::new(&doc, 72, 48, LayoutMode::Single);
    let index = pager.search_index(&doc);
    let hit = index.matches("Heavy").pop().unwrap();
    let cfi = doc.cfi(hit.anchor).unwrap();
    assert!(index.matches("│").is_empty());
    let selection = TextRange::new(
        index.matches("Thin").pop().unwrap().anchor,
        Anchor {
            byte: hit.anchor.byte + "Heavy".len(),
            ..hit.anchor
        },
    );
    let copied = doc.text_range(selection);
    assert!(copied.contains("Thin") && copied.contains("Heavy") && !copied.contains('│'));
    for width in [72, 30, 50] {
        pager.relayout(&doc, width, 48, LayoutMode::Single);
        let lines = pager.lines(0);
        let blank = lines.iter().position(|l| l.text.contains("Name")).unwrap() + 1;
        let dividers = |line: &botui_core::PageLine| {
            line.surfaces
                .iter()
                .filter_map(|s| match s {
                    Surface::TableDivider { x } => Some(*x),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(lines[blank].text.trim().is_empty());
        assert_eq!(dividers(&lines[blank]), dividers(&lines[blank - 1]));
        for (word, count) in [("Name", 2), ("Thin", 2), ("Slight", 2), ("Combined", 1)] {
            let line = lines.iter().find(|l| l.text.contains(word)).unwrap();
            assert_eq!(dividers(line).len(), count + 2, "{}", line.text);
        }
        assert!(lines.iter().all(|l| !l.text.contains(['─', '│'])));
        let line = lines.iter().find(|l| l.text.contains("Heavy")).unwrap();
        assert_eq!(
            &line.text[index.visible_range(&hit, line).unwrap()],
            "Heavy"
        );
        assert_eq!(doc.resolve_cfi(&cfi), Some(hit.anchor));
    }
}

#[test]
fn epub_table_sides_enclose_wrapped_content_even_without_cell_spacing() {
    use botui_core::{LayoutMode, Pager, Surface};
    for css in ["", "border-spacing:0", "border-collapse:collapse;padding:0"] {
        let doc = styled_fixture(
            "<table><tr><th>A</th><th>B</th><th>C</th></tr>\
             <tr><td>coast</td><td colspan='2'>the coast remains visible</td></tr>\
             <tr><td>&#160;</td></tr><tr><td>port</td><td>dock</td><td>boats</td></tr></table>",
            &format!("table {{width:100%;{css}}} th,td {{text-align:left}}"),
        );
        let mut pager = Pager::new(&doc, 52, 30, LayoutMode::Single);
        for width in [52, 19, 33, 52] {
            pager.relayout(&doc, width, 30, LayoutMode::Single);
            pager.previous();
            let lines = pager.lines(0);
            let mut edges = lines.iter().enumerate().filter_map(|(i, l)| {
                l.surfaces.iter().find_map(|s| {
                    if let Surface::TableEdge { x, width, top } = s {
                        Some((i, *x, *x + *width - 1, *top))
                    } else {
                        None
                    }
                })
            });
            let (start, left, right, true) = edges.next().unwrap() else {
                panic!("top edge missing, {css} {width}")
            };
            assert!(right < width, "{css} width {width}: border at {right}");
            let (end, l, r, false) = edges
                .next()
                .expect("table must retain its bottom edge at the end of a book")
            else {
                panic!("bottom edge missing")
            };
            assert_eq!((left, right), (l, r));
            assert!(edges.next().is_none());
            for row in &lines[start + 1..end] {
                for x in [left, right] {
                    assert!(
                        row.surfaces
                            .iter()
                            .any(|s| matches!(s,Surface::TableDivider{x:at} if *at==x)),
                        "{css} width {width} side {x}: {}",
                        row.text
                    );
                    assert!(
                        row.text
                            .as_bytes()
                            .get(usize::from(x))
                            .is_none_or(|b| *b == b' '),
                        "side collides with text: {css} width {width} side {x}: {}",
                        row.text
                    );
                }
            }
        }
    }
}

#[test]
fn epub_table_edges_preserve_text_addresses_illustrations_and_page_turns() {
    use botui_core::{LayoutMode, Pager};
    let book = |role| {
        styled_fixture(
            &format!(
                "<p>Table caption</p><div><table role='{role}'><tr><th>Label</th><th>Value</th></tr>\
            <tr><td>First</td><td>Second</td></tr><tr><td>&#160;</td></tr>\
            <tr><td>Third</td><td>Fourth</td></tr></table></div>\
            <figure><img src='fig.png'/></figure><p>End of passage.</p>"
            ),
            "table, p, figure {margin:0} p:first-child {margin-bottom:64px} img {display:block;width:160px;height:96px}",
        )
    };
    let data = book("table");
    let plain = book("presentation");
    let contents = |doc: &botui_core::Document| {
        let mut pager = Pager::new(doc, 56, 5, LayoutMode::Single);
        let mut source = Vec::new();
        let mut edges = Vec::new();
        loop {
            let lines = pager.lines(0);
            for line in lines {
                for surface in &line.surfaces {
                    if let botui_core::Surface::TableEdge { top, .. } = surface {
                        edges.push(*top);
                    }
                }
                if !line.text.trim().is_empty() || !line.figures.is_empty() {
                    source.push((
                        line.text.clone(),
                        doc.cfi(line.anchor()),
                        line.figures.len(),
                    ));
                }
            }
            if pager.at_end() {
                break;
            }
            let before = pager.position();
            pager.next();
            let after = pager.position();
            assert_ne!(before, after);
            pager.previous();
            assert_eq!(pager.position(), before);
            pager.next();
            assert_eq!(pager.position(), after);
        }
        (source, edges)
    };
    let (actual, rules) = contents(&data);
    let (expected, no_rules) = contents(&plain);
    assert_eq!(actual, expected);
    assert!(actual.iter().any(|(_, _, figures)| *figures > 0));
    assert_eq!(rules, [true, false]);
    assert!(no_rules.is_empty());
    let pager = Pager::new(&data, 56, 60, LayoutMode::Single);
    let lines = pager.lines(0);
    let title = lines
        .iter()
        .position(|l| l.text.contains("Table caption"))
        .unwrap();
    assert!(lines[title + 1].text.is_empty() && lines[title + 1].surfaces.is_empty());
    assert!(
        lines[title + 2]
            .surfaces
            .iter()
            .any(|s| matches!(s, botui_core::Surface::TableEdge { top: true, .. }))
    );
    assert!(lines[title + 3].text.contains("Label"));
}

#[test]
fn full_width_epub_table_keeps_header_styles_and_source_positions() {
    use botui_core::{LayoutMode, Pager};
    let doc = styled_fixture(
        "<section><p>outside first</p><table><caption>First table</caption>         <thead><tr><th rowspan='2'>Name</th><th colspan='2'>Values</th></tr>         <tr><th>Left</th><th>Right</th></tr></thead>         <tbody><tr><td>漢字</td><td>eleven</td><td>twelve</td></tr></tbody></table>         <table><tr><th scope='row'>Row label</th><td>unrelated</td></tr></table></section>",
        "section table:first-of-type td {font-style:italic}",
    );
    let pager = Pager::new(&doc, 32, 9, LayoutMode::Spread);
    let chapter = doc.styled.values().next().unwrap();
    assert_eq!(chapter.tables.len(), 2);
    assert_eq!(chapter.tables[0].title, "First table");
    assert!(
        chapter.tables[1].heading.is_none(),
        "a row header is not a column heading"
    );
    let table = chapter
        .table_view(0, &doc.blocks, 32, 8, pager.reflow_options())
        .unwrap();
    assert!(table.width > 32);
    let body = table.rows.iter().find(|l| l.text.contains("漢字")).unwrap();
    let cell = body
        .spans
        .iter()
        .find(|s| body.text[s.bytes.clone()].contains("漢字"))
        .unwrap();
    assert!(cell.face.italic);
    let address = cell.source.unwrap();
    assert_eq!(doc.resolve_cfi(&doc.cfi(address).unwrap()), Some(address));
    let heading = table.rows[table.header]
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    for label in ["Name", "Values", "Left", "Right"] {
        assert!(heading.contains(label), "{heading}");
    }
    assert!(!heading.contains("漢字"));
    assert!(
        table
            .rows
            .iter()
            .all(|l| !l.text.contains("outside") && !l.text.contains("unrelated"))
    );
}

#[test]
fn epub_table_dividers_respect_presentation_tables_and_publisher_borders() {
    use botui_core::{LayoutMode, Pager};
    let doc = styled_fixture(
        "<table role='presentation'><tr><th>Layout</th><th>Header</th></tr>\
         <tr><td>A</td><td>B</td></tr></table>\
         <table data-botui-data-table='1'><tr><td>Side</td><td>By side</td></tr>\
         <tr><td>C</td><td>D</td></tr></table>\
         <table class='ruled'><tr><th>Ruled</th><th>Author</th></tr>\
         <tr><td>E</td><td>F</td></tr></table>",
        ".ruled {border-top:16px solid}",
    );
    let pager = Pager::new(&doc, 60, 48, LayoutMode::Single);
    let page = pager
        .lines(0)
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for word in ["Layout", "Side", "Ruled"] {
        assert!(page.contains(word));
    }
    assert!(page.contains('─'));
    assert!(pager.lines(0).iter().all(|l| l.surfaces.is_empty()));
}

#[test]
fn paging_back_from_a_citation_fills_prose_and_keeps_the_marker_whole() {
    use botui_core::{Anchor, LayoutMode, Pager, ReflowOptions};
    let paragraphs = (0..60)
        .map(|n| format!("<p>Passage {n}.</p>"))
        .collect::<String>();
    let doc = styled_fixture(
        &format!(
            "<p><img src='fig.png'/></p>{paragraphs}<p>A marker<a href='#notes'>[21]</a> inside the passage.</p>{paragraphs}<p id='notes'>Notes</p>"
        ),
        "p { margin: 0 }",
    );
    let marker = doc.references.iter().find(|r| r.label == "[21]").unwrap();
    let start = marker.bytes.as_ref().unwrap().start;
    for mode in [LayoutMode::Single, LayoutMode::Spread] {
        for resume in [
            None,
            Some(Anchor {
                block: marker.source,
                byte: start + 1,
            }),
        ] {
            let at = resume.unwrap_or(Anchor {
                block: marker.source,
                byte: start,
            });
            let mut pager = Pager::with_start(
                &doc,
                100,
                9,
                mode,
                ReflowOptions {
                    typography: Default::default(),
                    repair_printed: true,
                    inline_images: true,
                    paragraph_indent: 0,
                    compact_spacing: true,
                    paragraph_spacing: Some(0),
                },
                resume,
            );
            pager.seek(at);
            let hit = pager.lines(0)[0].text.clone();
            assert!(
                hit.contains("marker[21] inside"),
                "{mode:?} {resume:?}: {hit:?}"
            );
            pager.previous();
            for col in 0..pager.columns() {
                assert_eq!(
                    pager.lines(col).len(),
                    9,
                    "{mode:?} {resume:?}: column {col}"
                );
            }
            assert_eq!(
                pager.lines(pager.columns() - 1).last().unwrap().text,
                "Passage 59."
            );
            pager.next();
            assert_eq!(pager.lines(0)[0].text, hit);
        }
    }
}

#[test]
fn exact_epub_positions_after_chapter_titles_and_pictures_do_not_clip_the_previous_view() {
    use botui_core::{Anchor, LayoutMode, Pager, ReflowOptions};
    let passages = |label| {
        (0..32)
            .map(|n| format!("<p>{label} {n}.</p>"))
            .collect::<String>()
    };
    let body = format!(
        "<h1>First chapter</h1>{}<h1>Second chapter</h1>{}<img src='fig.png'/>{}",
        passages("Beginning"),
        passages("Chapter line"),
        passages("After picture"),
    );
    let doc = styled_fixture(
        &body,
        "p,h1{margin:0} img{display:block;width:80px;height:96px}",
    );
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 0,
        compact_spacing: true,
        paragraph_spacing: Some(0),
    };
    for mode in [LayoutMode::Single, LayoutMode::Spread] {
        for label in ["Chapter line", "After picture"] {
            for n in 1..8 {
                let query = format!("{label} {n}.");
                let block = doc.blocks.iter().position(|b| b.text == query).unwrap();
                let at = Anchor { block, byte: 0 };
                let cfi = doc.cfi(at).unwrap();
                let mut pager = Pager::with_start(&doc, 90, 12, mode, options, Some(at));
                assert_eq!(doc.cfi(pager.anchor()), Some(cfi));
                assert!(pager.lines(0)[0].text.contains(&query));
                pager.previous();
                let final_column = pager.lines(pager.columns() - 1);
                assert_eq!(final_column.len(), 12, "{mode:?} before {query}");
            }
        }
    }
}

#[test]
fn raised_footnotes_keep_wrapped_prose_on_consecutive_rows() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    for marker in [
        "<a role='doc-noteref' class='raised' href='#note'>[21]</a>",
        "<a epub:type='noteref' class='raised' href='#note'>[21]</a>",
        "<sup><a href='#note'>[21]</a></sup>",
        "<a href='#note'><sup>[21]</sup></a>",
    ] {
        let body = format!(
            "{}<p id='note'>Next paragraph.</p>",
            format!("<p>One two three four five{marker} six seven eight nine ten eleven twelve thirteen fourteen.</p>").repeat(8)
        );
        let doc = styled_fixture(
            &body,
            ".raised, sup { vertical-align: super; font-size: 0.8em }",
        );
        let pager = Pager::with_options(
            &doc,
            32,
            80,
            LayoutMode::Scroll,
            ReflowOptions {
                typography: Default::default(),
                paragraph_spacing: Some(1),
                repair_printed: true,
                inline_images: true,
                paragraph_indent: 0,
                compact_spacing: true,
            },
        );
        let rows = pager.lines(0);
        assert!(
            rows.iter()
                .any(|r| r.link_range(&doc, &doc.references[0]).is_some())
        );
        for (i, row) in rows.iter().enumerate().skip(1) {
            if row.text.trim().is_empty() {
                assert!(
                    rows[i - 1].text.ends_with("fourteen."),
                    "{marker}: unexpected gap after {:?}",
                    rows[i - 1].text
                );
            }
            if row.text.contains("Next paragraph") {
                assert!(rows[i - 1].text.trim().is_empty());
            }
        }
    }
}

#[test]
fn paragraph_spacing_in_epub_and_fb2_preserves_structured_content_and_anchors() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    let epub = styled_fixture(
        "<h1>Chapter</h1><p>First paragraph.</p><p>Second paragraph.</p>\
         <table><tr><td><p>Cell A</p><p>Cell B</p></td></tr></table>\
         <figure><figcaption><p>Caption A</p><p>Caption B</p></figcaption></figure>\
         <div role='doc-poem'><p>Verse A</p><p>Verse B</p></div>",
        "p {margin:0 !important} h1 {margin:0 0 16px} td p {margin:0 0 16px !important}",
    );
    let fb2 = super::fb2::parse(
        b"<FictionBook><body><section><title><p>Chapter</p></title>\
          <p>First paragraph.</p><p>Second paragraph.</p><empty-line/><p>After break.</p>\
          <poem><stanza><v>Verse A</v><v>Verse B</v></stanza></poem></section></body></FictionBook>",
        "Test book",
    )
    .unwrap();
    let epub_gaps = [
        ("Cell A", "Cell B", 1),
        ("Caption A", "Caption B", 0),
        ("Verse A", "Verse B", 0),
    ];
    let fb2_gaps = [
        ("Second paragraph.", "After break.", 1),
        ("Verse A", "Verse B", 1),
    ];
    for (doc, book_gap, structured_gaps) in [(&epub, 0, &epub_gaps[..]), (&fb2, 1, &fb2_gaps[..])] {
        let options = ReflowOptions {
            typography: Default::default(),
            repair_printed: true,
            inline_images: true,
            paragraph_indent: 3,
            compact_spacing: true,
            paragraph_spacing: None,
        };
        let mut pager = Pager::with_options(doc, 60, 48, LayoutMode::Single, options);
        for spacing in [None, Some(0), Some(1), Some(2), None] {
            pager.reconfigure(
                doc,
                60,
                48,
                LayoutMode::Single,
                4,
                ReflowOptions {
                    paragraph_spacing: spacing,
                    ..options
                },
            );
            let lines = pager.lines(0);
            let row = |s: &str| lines.iter().position(|line| line.text.trim() == s).unwrap();
            let gap = |left, right| row(right) - row(left) - 1;
            assert_eq!(
                gap("First paragraph.", "Second paragraph."),
                usize::from(spacing.unwrap_or(book_gap))
            );
            assert_eq!(gap("Chapter", "First paragraph."), 1);
            let at = lines[row("Second paragraph.")].anchor();
            assert_eq!(&doc.blocks[at.block].text[at.byte..], "Second paragraph.");
            assert_eq!(doc.resolve_cfi(&doc.cfi(at).unwrap()), Some(at));
            for &(left, right, expected) in structured_gaps {
                assert_eq!(gap(left, right), expected, "{left} to {right}, {spacing:?}");
            }
        }
    }
}

#[test]
fn paragraph_spacing_keeps_centered_addresses_and_authored_blank_rows() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    // The real Useful Addresses chapter uses centered p's for address lines,
    // but justified NBSP paragraphs for the one- and two-row group separators.
    let doc = styled_fixture(
        "<h1><span style='text-decoration:underline'>USEFUL ADDRESSES</span></h1>\
         <p>Ordinary prose A.</p><p>Ordinary prose B.</p>\
         <p class='address'>Institute A</p><p class='address'>Box A</p>\
         <p class='blank'>&#160;</p><p class='blank'>&#160;</p>\
         <p class='address'>Institute B</p><p class='address'>Box B</p>\
         <p class='blank'><br/></p>\
         <p class='address'>Institute C</p><p class='address'>Box C<br/>City C</p>",
        "p {margin:0; text-align:justify} .address {text-align:center} h1 {margin:0 0 16px}",
    );
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 0,
        compact_spacing: true,
        paragraph_spacing: None,
    };
    let mut pager = Pager::with_options(&doc, 48, 96, LayoutMode::Single, options);
    for width in [48, 80] {
        for spacing in [None, Some(0), Some(1), Some(2), None] {
            pager.reconfigure(
                &doc,
                width,
                96,
                LayoutMode::Single,
                4,
                ReflowOptions {
                    paragraph_spacing: spacing,
                    ..options
                },
            );
            let lines = pager.lines(0);
            let row = |text: &str| lines.iter().position(|l| l.text.trim() == text).unwrap();
            let gap = |a, b| row(b) - row(a) - 1;
            assert_eq!(
                gap("Ordinary prose A.", "Ordinary prose B."),
                usize::from(spacing.unwrap_or(0))
            );
            for (a, b, expected) in [
                ("Institute A", "Box A", 0),
                ("Box A", "Institute B", 2),
                ("Institute B", "Box B", 0),
                ("Box B", "Institute C", 1),
                ("Institute C", "Box C", 0),
                ("Box C", "City C", 0),
            ] {
                assert_eq!(
                    gap(a, b),
                    expected,
                    "{a} to {b}: width {width}, spacing {spacing:?}"
                );
            }
            for text in [
                "Institute A",
                "Box A",
                "Institute B",
                "Box B",
                "Institute C",
                "Box C",
                "City C",
            ] {
                let line = &lines[row(text)];
                let left = usize::from(line.indent) + line.text.find(text).unwrap();
                assert!(
                    left.abs_diff((usize::from(width) - text.len()) / 2) <= 1,
                    "address alignment: {line:?}"
                );
            }
            assert!(
                lines[row("USEFUL ADDRESSES")]
                    .spans
                    .iter()
                    .any(|s| s.face.underline)
            );
        }
    }
}

#[test]
fn centered_paragraphs_keep_the_book_indent_across_settings_and_resizes() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    let doc = styled_fixture(
        "<section><p>Centered</p><p class='offset'><b>Indented</b></p></section>\
         <p class='offset'>Ordinary</p>",
        "section {text-align:center} .offset {text-indent:48px} \
         @media(min-width:500px) {section {text-align:left}}",
    );
    for paragraph_spacing in [None, Some(1)] {
        for compact_spacing in [true, false] {
            for paragraph_indent in [0, 4] {
                let options = ReflowOptions {
                    typography: Default::default(),
                    repair_printed: true,
                    inline_images: true,
                    paragraph_spacing,
                    compact_spacing,
                    paragraph_indent,
                };
                let mut pager = Pager::with_options(&doc, 48, 30, LayoutMode::Single, options);
                // A second narrow width reuses the DOM after the wide layout.
                for width in [48, 80, 52] {
                    pager.relayout(&doc, width, 30, LayoutMode::Single);
                    let column = |word: &str| {
                        let line = pager
                            .lines(0)
                            .iter()
                            .find(|l| l.text.trim() == word)
                            .unwrap();
                        usize::from(line.indent) + line.text.find(word).unwrap()
                    };
                    let reader_indent = usize::from(paragraph_indent);
                    assert_eq!(column("Ordinary"), reader_indent);
                    if width == 80 {
                        assert_eq!(column("Centered"), reader_indent);
                        assert_eq!(column("Indented"), reader_indent);
                    } else {
                        // Both words occupy eight cells. A six-cell author indent
                        // moves the center of the indented line three cells right.
                        let centered = (usize::from(width) - 8) / 2;
                        assert_eq!(column("Centered"), centered);
                        assert_eq!(column("Indented"), centered + 3);
                    }
                }
            }
        }
    }
}

#[test]
fn compact_blocks_and_paragraph_spacing_are_independent() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    let doc = styled_fixture(
        "<p>Prose A.</p><p>Prose B.</p>\
         <div class='center'><p>Center A</p><p>Center B</p></div>\
         <address><p>Address A</p><p>Address B</p></address>\
         <table><tr><td><p>Cell A</p><p>Cell B</p></td></tr></table>",
        "p {margin:0} .center {text-align:center} table p {margin-bottom:32px}",
    );
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 0,
        paragraph_spacing: None,
        compact_spacing: true,
    };
    let mut pager = Pager::with_options(&doc, 60, 96, LayoutMode::Single, options);
    for spacing in [None, Some(0), Some(1), Some(2), None] {
        for compact in [true, false, true] {
            pager.reconfigure(
                &doc,
                60,
                96,
                LayoutMode::Single,
                4,
                ReflowOptions {
                    paragraph_spacing: spacing,
                    compact_spacing: compact,
                    ..options
                },
            );
            let lines = pager.lines(0);
            let row = |text: &str| lines.iter().position(|l| l.text.trim() == text).unwrap();
            let gap = |a, b| row(b) - row(a) - 1;
            assert_eq!(
                gap("Prose A.", "Prose B."),
                usize::from(spacing.unwrap_or(0))
            );
            let expected = if compact { 0 } else { spacing.unwrap_or(0) };
            assert_eq!(gap("Center A", "Center B"), usize::from(expected));
            assert_eq!(gap("Address A", "Address B"), usize::from(expected));
            assert_eq!(gap("Cell A", "Cell B"), 2);
        }
    }
}

#[test]
fn paragraph_spacing_uses_inherited_alignment_and_rechecks_it_after_resize() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    let doc = styled_fixture(
        "<section class='center'><div class='group'><p>Center A</p><p>Center B</p></div>\
         <div class='reset'><p>Reset prose A.</p><p>Reset prose B.</p></div></section>\
         <section class='right'><div><p>Right A</p><p>Right B</p></div></section>\
         <section class='responsive'><div><p>Responsive A.</p><p>Responsive B.</p></div></section>",
        "p {margin:0 !important} .center {text-align:center} \
         .center > .group > p {margin-bottom:32px !important} \
         .reset {text-align:justify} .right {text-align:right} \
         .right p {margin-bottom:16px !important} .responsive {text-align:center} \
         @media(min-width:500px) {.responsive {text-align:justify}}",
    );
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 0,
        compact_spacing: true,
        paragraph_spacing: None,
    };
    let mut pager = Pager::with_options(&doc, 48, 96, LayoutMode::Single, options);
    for spacing in [None, Some(0), Some(1), Some(2), None] {
        pager.reconfigure(
            &doc,
            48,
            96,
            LayoutMode::Single,
            4,
            ReflowOptions {
                paragraph_spacing: spacing,
                ..options
            },
        );
        // Keep this Pager's DOM pool across resizes. 49 is a new narrow width:
        // unlike cached 48, it must reclassify the DOM previously used at 80.
        for width in [48, 80, 49, 48] {
            pager.relayout(&doc, width, 96, LayoutMode::Single);
            let lines = pager.lines(0);
            let row = |text: &str| lines.iter().position(|l| l.text.trim() == text).unwrap();
            let gap = |a, b| row(b) - row(a) - 1;
            assert_eq!(gap("Center A", "Center B"), 2, "spacing {spacing:?}");
            assert_eq!(gap("Right A", "Right B"), 1, "spacing {spacing:?}");
            assert_eq!(
                gap("Reset prose A.", "Reset prose B."),
                usize::from(spacing.unwrap_or(0))
            );
            let responsive_gap = if width == 80 { spacing.unwrap_or(0) } else { 0 };
            assert_eq!(
                gap("Responsive A.", "Responsive B."),
                usize::from(responsive_gap),
                "width {width}, spacing {spacing:?}"
            );
            let line = &lines[row("Right A")];
            let left = usize::from(line.indent) + line.text.find("Right A").unwrap();
            assert!(
                left.abs_diff(usize::from(width) - "Right A".len()) <= 1,
                "right alignment: {line:?}"
            );
        }
    }
}

#[test]
fn paragraph_spacing_recognizes_bare_epub_compact_markup_without_stylesheets() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    // Each wrapper is a separate EPUB: another styled element must not mask a
    // missing layout-gate case. The reader's default paragraph gap is one row.
    for (open, close) in [
        ("<address>", "</address>"),
        ("<div epub:type='appendix z3998:verse'>", "</div>"),
        ("<div role='note doc-verse'>", "</div>"),
        ("<center>", "</center>"),
        ("<div align='right'>", "</div>"),
    ] {
        let html = format!(
            "<html xmlns='http://www.w3.org/1999/xhtml' xmlns:epub='http://www.idpf.org/2007/ops'>\
             <head><title>Bare groups</title></head><body>\
             <p>Ordinary A.</p><p>Ordinary B.</p>\
             {open}<p>Compact A</p><p>Compact B</p>{close}</body></html>"
        );
        let doc = read_book(
            archive(&[
                ("META-INF/container.xml", "<container><rootfile full-path='p'/></container>"),
                ("p", "<package><manifest><item id='c' href='c.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='c'/></spine></package>"),
                ("c.xhtml", &html),
            ]),
            "Bare groups",
        ).unwrap();
        for spacing in [0, 2] {
            let pager = Pager::with_options(
                &doc,
                48,
                48,
                LayoutMode::Single,
                ReflowOptions {
                    typography: Default::default(),
                    repair_printed: true,
                    inline_images: true,
                    paragraph_indent: 0,
                    compact_spacing: true,
                    paragraph_spacing: Some(spacing),
                },
            );
            let lines = pager.lines(0);
            let row = |text: &str| lines.iter().position(|l| l.text.trim() == text).unwrap();
            assert_eq!(
                row("Ordinary B.") - row("Ordinary A.") - 1,
                usize::from(spacing)
            );
            assert_eq!(
                row("Compact B") - row("Compact A") - 1,
                1,
                "{open}, spacing {spacing}"
            );
        }
    }
}

#[test]
fn paragraph_spacing_preserves_semantic_groups_and_an_independent_unicode_cfi() {
    use botui_core::{Anchor, LayoutMode, Pager, ReflowOptions};
    let doc = styled_fixture(
        "<p>Alpha <em>Кит 🐋 Маяк</em></p><p>Ordinary prose.</p>\
         <address><p>Address A</p><p>Address B</p></address>\
         <ul><li><p>List A</p><p>List B</p></li></ul>\
         <div role='doc-poem'><p>Verse A</p><p>Verse B</p></div>",
        "p {margin:0; text-align:left} address {font-style:normal} \
         ul {margin:0; padding-left:16px; list-style:none} li p {margin-bottom:32px} \
         [role='doc-poem'] p {margin-bottom:16px}",
    );
    // First body paragraph, its em's text node, UTF-16 offset after "Кит 🐋 ".
    // This address is authored from the fixture DOM, not obtained from doc.cfi().
    let cfi = "epubcfi(/4/2!/4/2/2/1:7)";
    let at = doc
        .resolve_cfi(cfi)
        .expect("the independently authored CFI resolves");
    assert_eq!(at.block, 0);
    assert_eq!(&doc.blocks[at.block].text[at.byte..], "Маяк");
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 0,
        compact_spacing: true,
        paragraph_spacing: None,
    };
    let mut pager = Pager::with_options(&doc, 48, 96, LayoutMode::Single, options);
    for width in [48, 80] {
        for spacing in [None, Some(0), Some(1), Some(2), None] {
            pager.seek(Anchor::default());
            pager.reconfigure(
                &doc,
                width,
                96,
                LayoutMode::Single,
                4,
                ReflowOptions {
                    paragraph_spacing: spacing,
                    ..options
                },
            );
            let lines = pager.lines(0);
            let row = |text: &str| lines.iter().position(|l| l.text.trim() == text).unwrap();
            let gap = |a, b| row(b) - row(a) - 1;
            assert_eq!(
                gap("Alpha Кит 🐋 Маяк", "Ordinary prose."),
                usize::from(spacing.unwrap_or(0))
            );
            for (a, b, expected) in [
                ("Address A", "Address B", 0),
                ("List A", "List B", 2),
                ("Verse A", "Verse B", 1),
            ] {
                assert_eq!(gap(a, b), expected, "{a} to {b}: spacing {spacing:?}");
            }
            let line = &lines[row("Alpha Кит 🐋 Маяк")];
            assert!(
                line.text_cells(&doc)
                    .into_iter()
                    .any(|cell| cell.source.start == at)
            );
            assert_eq!(doc.resolve_cfi(cfi), Some(at));
            pager.seek(at);
            assert_eq!(pager.anchor(), at);
            assert!(pager.lines(0)[0].text.contains("Маяк"));
        }
    }
}

#[test]
fn prose_preserves_words_indentation_footnotes_and_spread_page_turns() {
    use botui_core::{LayoutMode, Pager, ReflowOptions, Target};
    let mut body = String::from("<h1>Диалоги</h1>");
    for n in 0..36 {
        body.push_str(&format!("<p>— Реплика {n}. Кто видел <em>Кита 🐋 и Маяк</em> рядом с <a href='#n'>башней</a>?</p>"));
    }
    body.push_str("<p id='n'>Сноска к диалогу.</p>");
    let doc = styled_fixture(
        &body,
        "h1 {font-size:2.5em} p {text-indent:1em; text-align:justify}",
    );
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 3,
        compact_spacing: true,
        paragraph_spacing: None,
    };
    let mut pager = Pager::with_options(&doc, 100, 8, LayoutMode::Spread, options);
    let text = |p: &Pager| {
        (0..p.columns())
            .flat_map(|c| p.lines(c))
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
    };
    let opening = text(&pager);
    assert!(opening.iter().any(|l| l.contains("Диалоги")), "{opening:?}");
    assert!(
        opening.iter().any(|l| l.starts_with("   — Реплика 0.")),
        "{opening:?}"
    );
    pager.next();
    let second = text(&pager);
    assert!(!second.iter().any(|l| l.contains("Реплика 0.")));
    pager.previous();
    assert_eq!(text(&pager), opening);
    // Independently authored CFI: after a supplementary-plane character in the em of the first paragraph.
    let cfi = "epubcfi(/4/2!/4/4/2/1:10)";
    let at = doc.resolve_cfi(cfi).unwrap();
    assert!(doc.blocks[at.block].text[at.byte..].starts_with("Маяк"));
    let reopened = Pager::with_start(&doc, 100, 8, LayoutMode::Spread, options, Some(at));
    assert!(
        reopened.lines(0)[0].text.trim_start().starts_with("Маяк"),
        "{:?}",
        text(&reopened)
    );
    assert_eq!(doc.cfi(reopened.anchor()).as_deref(), Some(cfi));
    let foot = &doc.references[0];
    assert!(matches!(foot.target, Target::Passage(_)));
    assert!(
        reopened
            .lines(0)
            .iter()
            .any(|l| l.link_range(&doc, foot).is_some())
    );
}

#[test]
fn latex_is_typeset_in_place_and_dollar_amounts_stay_literal() {
    use botui_core::{LayoutMode, Pager};
    let doc = styled_fixture(
        r#"<p>Tickets cost $20 per reader.</p><div class="math display">\[\frac{up}{dn}\]</div><script type="math/tex; mode=display">\sqrt{x}</script><p>Later</p>"#,
        "",
    );
    let p = Pager::new(&doc, 60, 60, LayoutMode::Single);
    let lines: Vec<_> = p.lines(0).iter().map(|l| l.text.trim()).collect();
    assert!(lines.contains(&"Tickets cost $20 per reader."), "{lines:?}");
    let bar = lines
        .iter()
        .position(|s| s.contains('─'))
        .expect("fraction");
    assert!(
        bar > 0 && lines[bar - 1] == "up" && lines[bar + 1] == "dn",
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l.contains('√')), "{lines:?}");
    let at = p.lines(0)[bar - 1].anchor();
    assert!(doc.blocks[at.block].text.contains("frac") && doc.cfi(at).is_some());
}

#[test]
fn technical_epub_keeps_row_groups_math_scripts_matrices_and_flex_columns() {
    use botui_core::{LayoutMode, Pager};
    let doc = styled_fixture(
        r#"<section class="pair"><div>First</div><div>Second</div></section><table><tbody><tr><td rowspan="0">Group</td><td>One</td></tr><tr><td>Two</td></tr><tr><td>Three</td></tr></tbody><tbody><tr><td>Next</td><td>Four</td></tr></tbody></table><math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><msubsup><mi>base</mi><mi>sub</mi><mi>sup</mi></msubsup></math><math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><mtable><mtr><mtd><mi>aa</mi></mtd><mtd><mi>bb</mi></mtd></mtr><mtr><mtd><mi>cc</mi></mtd><mtd><mi>dd</mi></mtd></mtr></mtable></math>"#,
        ".pair{display:flex;gap:3ch} td{vertical-align:top}",
    );
    let pager = Pager::new(&doc, 60, 80, LayoutMode::Single);
    let rows: Vec<_> = pager.lines(0).iter().map(|l| l.text.as_str()).collect();
    let pos = |word: &str| {
        rows.iter()
            .enumerate()
            .find_map(|(y, row)| row.find(word).map(|x| (y, x)))
            .unwrap_or_else(|| panic!("{word:?} missing from {rows:?}"))
    };
    assert_eq!(pos("Second"), (pos("First").0, pos("First").1 + 8));
    assert_eq!(pos("Two").1, pos("One").1);
    assert_eq!(pos("Three").1, pos("One").1);
    assert_eq!(pos("Next").1, pos("Group").1);
    assert_eq!(pos("Four").0, pos("Next").0);
    assert!(
        pos("sup").0 < pos("base").0 && pos("base").0 < pos("sub").0,
        "{rows:?}"
    );
    assert_eq!(pos("aa").0, pos("bb").0);
    assert_eq!(pos("cc").0, pos("dd").0);
    assert_eq!(pos("aa").1, pos("cc").1);
    assert!(pos("aa").0 < pos("cc").0 && pos("aa").1 < pos("bb").1);
}

#[test]
fn justified_prose_wraps_contiguously_and_separates_dialogue_paragraphs() {
    use botui_core::{LayoutMode, Pager};
    let paragraph = "The sailor read the tide and watched the coast. ".repeat(6);
    let doc = styled_fixture(
        &format!("<p>{paragraph}</p><p>— First line.</p><p>— Second line.</p>"),
        "p { text-align:justify }",
    );
    let pager = Pager::new(&doc, 40, 40, LayoutMode::Single);
    let rows: Vec<_> = pager.lines(0).iter().map(|l| l.text.trim()).collect();
    let first = rows.iter().position(|&s| s == "— First line.").unwrap();
    assert!(
        first > 4 && rows[..first - 1].iter().all(|s| !s.is_empty()),
        "{rows:?}"
    );
    assert_eq!(
        &rows[first..first + 3],
        ["— First line.", "", "— Second line."]
    );
}

#[test]
fn justified_prose_keeps_natural_word_spacing_at_terminal_widths() {
    use botui_core::{LayoutMode, Pager};
    let paragraph = "Миновав плац, я направился к дорожке, что вела в южный угол дворцового сада. Рядом стояла беседка. На деревянном столе лежала раскрытая книга.";
    let marked = paragraph.replace("южный", "ю<em>жн</em>ый");
    let doc = styled_fixture(&format!("<p>{marked}</p>"), "body { text-align:justify }");
    for width in 26..60 {
        let pager = Pager::new(&doc, width, 40, LayoutMode::Single);
        for row in pager.lines(0) {
            assert!(
                !row.text.trim().contains("  "),
                "artificial word spacing at width {width}: {:?}",
                row.text
            );
        }
        let words: Vec<_> = pager
            .lines(0)
            .iter()
            .flat_map(|l| l.text.split_whitespace())
            .collect();
        assert_eq!(
            words,
            paragraph.split_whitespace().collect::<Vec<_>>(),
            "column width {width}"
        );
    }
}

#[test]
fn justified_layout_matches_start_alignment_around_inline_boxes_and_on_resize() {
    use botui_core::{LayoutMode, Pager};
    let body = r#"<h1>Centered</h1><p>One two <em>three</em> four
        <img src="fig.png" width="40" height="32"/> five six seven eight nine.</p>
        <p>After the figure <math><mfrac><mi>x</mi><mi>y</mi></mfrac></math> the paragraph continues.</p>
        <p dir="rtl">אחד שני שלושה ארבע חמישה שישה שבעה שמונה תשעה עשרה.</p>
        <p class="last">Last line keeps its alignment across multiple lines.</p>
        <p class="right">Right aligned</p><pre>  code    listing</pre>
        <table><tr><td rowspan="2">Group</td><td>one</td></tr><tr><td>two</td></tr></table>"#;
    let make = |align| {
        styled_fixture(
            body,
            &format!(
                "body {{ --prose: {align}; text-align: var(--prose) }}
         p:first-of-type {{ text-align-last: {align} }}
         .last {{ text-align-last: center }} .right {{ text-align: right }}"
            ),
        )
    };
    let native = make("start");
    let book = make("justify");
    let mut expected = Pager::new(&native, 28, 80, LayoutMode::Single);
    let mut actual = Pager::new(&book, 28, 80, LayoutMode::Single);
    for width in [28, 35, 42, 28] {
        actual.relayout(&book, width, 80, LayoutMode::Single);
        expected.relayout(&native, width, 80, LayoutMode::Single);
        let rows = actual.lines(0);
        assert!(rows.iter().any(|r| !r.figures.is_empty()));
        assert!(rows.iter().any(|r| r.text.contains("code    listing")));
        assert_eq!(rows.len(), expected.lines(0).len(), "width {width}");
        for (i, (row, reference)) in rows.iter().zip(expected.lines(0)).enumerate() {
            assert_eq!(
                format!("{row:?}"),
                format!("{reference:?}"),
                "width {width}, row {i}"
            );
        }
    }
}

#[test]
fn illustrations_fit_the_viewport_and_hidden_figures_take_no_space() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    let doc = styled_fixture(
        r#"<p>Heading</p><img src="fig.png"/><img src="fig.png" class="omit"/><p>After the diagram</p>"#,
        ".omit{display:none}",
    );
    let mut pager = Pager::new(&doc, 90, 22, LayoutMode::Single);
    let height = |p: &Pager| {
        p.lines(0)
            .iter()
            .flat_map(|l| &l.figures)
            .inspect(|r| assert_eq!(r.reference, 0))
            .count()
    };
    let short = height(&pager);
    pager.relayout(&doc, 90, 42, LayoutMode::Single);
    assert!(
        short > 1 && height(&pager) > short && height(&pager) > 9,
        "{short} -> {}",
        height(&pager)
    );
    let hidden = Pager::with_options(
        &doc,
        90,
        42,
        LayoutMode::Single,
        ReflowOptions {
            typography: Default::default(),
            repair_printed: true,
            inline_images: false,
            paragraph_indent: 0,
            compact_spacing: true,
            paragraph_spacing: None,
        },
    );
    assert_eq!(height(&hidden), 0);
    assert!(
        hidden
            .lines(0)
            .iter()
            .any(|l| l.text.contains("After the diagram"))
    );
}

#[test]
fn chapter_boundaries_survive_resize_page_turns_and_scroll_mode() {
    use botui_core::{Anchor, LayoutMode, Pager};
    let doc = styled_fixture(
        "<section role='doc-chapter'><h2>First</h2><p>Beginning.</p><h3>Within</h3><p>Last line.</p></section>         <section role='doc-chapter'><h2>Second</h2><p>Next line.</p><p>Closing.</p></section>",
        "p,h2,h3{margin:0}",
    );
    assert_eq!(
        doc.chapter_starts.iter().copied().collect::<Vec<_>>(),
        [0, 4]
    );
    let mut pager = Pager::new(&doc, 90, 12, LayoutMode::Spread);
    let start = Anchor { block: 4, byte: 0 };
    let cfi = doc.cfi(start).unwrap();
    for (width, height) in [(90, 12), (74, 8), (58, 9), (90, 12)] {
        pager.relayout(&doc, width, height, LayoutMode::Spread);
        pager.seek(Anchor::default());
        assert!(pager.lines(0).iter().any(|l| l.text.contains("Last line.")));
        assert!(pager.lines(1).is_empty());
        pager.next();
        assert_eq!(pager.anchor(), start);
        assert!(pager.lines(0)[0].text.contains("Second"));
        assert!(pager.lines(0)[1].text.is_empty());
        assert_eq!(doc.cfi(pager.lines(0)[0].anchor()), Some(cfi.clone()));
        pager.seek(start);
        pager.previous();
        assert!(pager.lines(0)[0].text.contains("First"));
    }
    pager.relayout(&doc, 60, 30, LayoutMode::Scroll);
    pager.seek(Anchor::default());
    let rows = pager.lines(0);
    let heading = rows.iter().position(|l| l.text.contains("Second")).unwrap();
    assert!(rows[heading - 3].text.contains("Last line."));
    assert!(rows[heading - 2..heading].iter().all(|l| l.text.is_empty()));
    assert!(rows[heading + 1].text.is_empty());
    assert!(rows[heading + 2].text.contains("Next line."));
}

#[test]
fn reopening_before_the_last_picture_keeps_the_next_chapter_title_at_the_top() {
    use botui_core::{Anchor, LayoutMode, Pager, ReflowOptions};
    let doc = read_book(archive(&[
        ("META-INF/container.xml", "<container><rootfile full-path='p'/></container>"),
        ("p", "<package><manifest><item id='a' href='a' media-type='application/xhtml+xml'/><item id='b' href='b' media-type='application/xhtml+xml'/><item id='i' href='i.png' media-type='image/png'/></manifest><spine><itemref idref='a'/><itemref idref='b'/></spine></package>"),
        ("a", "<html><body><h2>Ten</h2><p>Ending of the chapter</p><figure><img src='i.png' style='width:100px;height:96px'/></figure></body></html>"),
        ("b", "<html><body><h2>Eleven</h2><p>First words of a new chapter.</p></body></html>"),
        ("i.png", "image"),
    ]), "Illustrated chapters").unwrap();
    let start = Anchor { block: 1, byte: 7 };
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 0,
        compact_spacing: true,
        paragraph_spacing: Some(1),
    };
    for height in [6, 12, 24] {
        let mut pager =
            Pager::with_start(&doc, 100, height, LayoutMode::Spread, options, Some(start));
        let mut pictures = 0;
        let mut titles = 0;
        loop {
            for column in 0..pager.columns() {
                for (row, line) in pager.lines(column).iter().enumerate() {
                    pictures += line.figures.iter().filter(|f| f.row == 0).count();
                    if line.text.trim() == "Eleven" {
                        assert_eq!((pictures, column, row), (1, 0, 0), "height {height}");
                        titles += 1;
                    }
                }
            }
            if pager.at_end() {
                break;
            }
            pager.next();
        }
        assert_eq!(titles, 1);
    }
}

#[test]
fn reopening_an_illustrated_spread_distinguishes_the_picture_from_its_text_cfi() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    let ending: String = (0..13).map(|n| format!("<p>Line {n}</p>")).collect();
    let doc = styled_fixture(
        &format!(
            "{ending}<figure><img src='fig.png'/></figure><h1>Next chapter</h1><p>Opening words.</p>"
        ),
        "p,h1,figure{margin:0} img{display:block;width:160px;height:96px}",
    );
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: false,
        inline_images: true,
        paragraph_indent: 0,
        compact_spacing: true,
        paragraph_spacing: Some(0),
    };
    let open = |at| Pager::with_start(&doc, 100, 8, LayoutMode::Spread, options, at);
    let mut reading = open(None);
    reading.next();
    let picture = reading.position();
    assert!(picture.illustration.is_some());
    let cfi = doc.cfi(picture.anchor).unwrap();
    let rows = |pager: &Pager| {
        (0..pager.columns())
            .map(|col| {
                pager
                    .lines(col)
                    .iter()
                    .map(|line| {
                        (
                            line.text.clone(),
                            line.figures
                                .iter()
                                .map(|f| (f.reference, f.row, f.height))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let expected = rows(&reading);
    let text = open(doc.reading_anchor(&cfi));
    assert!(text.lines(0)[0].text.contains("Line 12"));
    let mut reopened = Pager::for_display_position(
        &doc,
        (100, 8),
        LayoutMode::Spread,
        4,
        options,
        Some(picture),
        (1, 2),
    );
    assert_eq!(rows(&reopened), expected);
    reopened.previous();
    reopened.next();
    assert_eq!(rows(&reopened), expected);
    assert_eq!(doc.cfi(reopened.anchor()), Some(cfi));
}

#[test]
fn standalone_illustrations_use_one_blank_row_at_the_actual_cell_shape() {
    use botui_core::{LayoutMode, Pager};
    let mut doc = styled_fixture(
        r#"<p>Before</p><figure><img src="fig.png"/></figure><p>After</p>"#,
        "figure{margin:3.25em 0 0} img{display:block;width:160px;height:160px}",
    );
    doc.image_sizes.insert("fig.png".into(), (400, 200));
    let mut pager = Pager::new(&doc, 64, 32, LayoutMode::Single);
    for (cell, height) in [((8, 16), 5), ((8, 26), 3), ((8, 16), 5)] {
        pager.set_image_cell(cell.0, cell.1);
        pager.relayout(&doc, 64, 32, LayoutMode::Single);
        let rows = pager.lines(0);
        let before = rows.iter().position(|l| l.text.contains("Before")).unwrap();
        let first = rows.iter().position(|l| !l.figures.is_empty()).unwrap();
        let after = rows.iter().position(|l| l.text.contains("After")).unwrap();
        assert_eq!(first, before + 2);
        assert_eq!(rows[first].figures[0].height, height);
        assert_eq!(after, first + usize::from(height) + 1);
    }
}

#[test]
fn publisher_figures_move_whole_across_page_breaks() {
    use botui_core::{LayoutMode, Pager};
    let doc = styled_fixture(
        r#"<p>Opening</p><p>Before the picture.</p><img src="fig.png"/>
        <p>First caption.</p><img src="fig.png"/><p>Last caption.</p>"#,
        "img{display:block;height:64px;width:80px}",
    );
    for (mode, width, height) in [
        (LayoutMode::Single, 32, 6),
        (LayoutMode::Spread, 90, 7),
        (LayoutMode::Spread, 90, 9),
    ] {
        let mut pager = Pager::new(&doc, width, height, mode);
        let mut figures = Vec::new();
        let mut text = String::new();
        loop {
            for col in 0..pager.columns() {
                let rows = pager.lines(col);
                text.extend(rows.iter().map(|row| row.text.as_str()));
                for id in 0..2 {
                    let mut parts = rows
                        .iter()
                        .flat_map(|line| &line.figures)
                        .filter(|f| f.reference == id)
                        .peekable();
                    if let Some(f) = parts.peek() {
                        let expected = 0..f.height;
                        assert!(parts.map(|part| part.row).eq(expected));
                        figures.push(id);
                    }
                }
            }
            if pager.at_end() {
                break;
            }
            pager.next();
        }
        assert_eq!(figures, [0, 1]);
        assert!(text.contains("First caption.") && text.contains("Last caption."));
    }
}

#[test]
fn resizing_and_cached_layouts_match_a_fresh_epub_layout() {
    use botui_core::{Anchor, LayoutMode, Pager, ReflowOptions};
    let doc = styled_fixture(
        r#"<p>Opening with <em>emphasis</em> and more words to wrap across the page.</p>
        <div class="wide">Wide screen</div><div class="tall">Tall screen</div>
        <div class="pair"><p>Left column</p><p>Right column</p></div>
        <table><tr><td rowspan="2">Both rows</td><td>Upper cell</td></tr><tr><td>Lower cell</td></tr></table>
        <math><mfrac><mi>numerator</mi><mi>denominator</mi></mfrac></math>
        <img src="fig.png"/><p>Last paragraph after the illustration.</p>"#,
        ".pair{display:flex;gap:2ch}.wide,.tall{display:none}@media(min-width:500px){.wide{display:block}}@media(min-height:500px){.tall{display:block}}",
    );
    let options = ReflowOptions {
        typography: Default::default(),
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 2,
        compact_spacing: true,
        paragraph_spacing: None,
    };
    let at = Anchor { block: 0, byte: 8 };
    let mut warm = Pager::with_start(&doc, 80, 20, LayoutMode::Single, options, Some(at));
    let snapshot = |p: &mut Pager| {
        p.seek(Anchor::default());
        let mut rows = Vec::new();
        loop {
            for col in 0..p.columns() {
                rows.extend(p.lines(col).iter().map(|l| format!("{l:?}")));
            }
            if p.at_end() {
                break;
            }
            p.next();
        }
        rows
    };
    // Cross media-query boundaries, resize images, and revisit cached geometries.
    for (width, height, mode) in [
        (48, 20, LayoutMode::Single),
        (48, 40, LayoutMode::Single),
        (80, 40, LayoutMode::Single),
        (48, 40, LayoutMode::Single),
        (100, 40, LayoutMode::Spread),
        (80, 20, LayoutMode::Scroll),
    ] {
        warm.relayout(&doc, width, height, mode);
        let mut cold = Pager::with_start(&doc, width, height, mode, options, Some(at));
        warm.seek(at);
        cold.seek(at);
        assert_eq!(warm.anchor(), cold.anchor());
        assert_eq!(
            snapshot(&mut warm),
            snapshot(&mut cold),
            "{width}x{height} {mode:?}"
        );
    }
    let changed = ReflowOptions {
        inline_images: false,
        paragraph_indent: 5,
        compact_spacing: true,
        ..options
    };
    warm.reconfigure(&doc, 48, 40, LayoutMode::Single, 4, changed);
    let mut cold = Pager::with_start(&doc, 48, 40, LayoutMode::Single, changed, Some(at));
    assert_eq!(snapshot(&mut warm), snapshot(&mut cold));
}

#[test]
fn visible_chapters_match_complete_pagination_across_resizes_and_page_turns() {
    use botui_core::{Anchor, LayoutMode, Pager};
    use std::sync::Arc;
    let doc = Arc::new(read_book(archive(&[
        ("META-INF/container.xml", "<container><rootfile full-path='p'/></container>"),
        ("p","<package><manifest><item id='a' href='a' media-type='application/xhtml+xml'/><item id='b' href='b' media-type='application/xhtml+xml'/><item id='c' href='c' media-type='application/xhtml+xml'/><item id='i' href='img.png' media-type='image/png'/></manifest><spine><itemref idref='a'/><itemref idref='b'/><itemref idref='c'/></spine></package>"),
        ("a","<html><body><p>Opening chapter with some words to wrap and an ending.</p><p>Last words of the opening chapter</p></body></html>"),
        ("b","<html><head><style>.hidden{display:none}@media(min-width:400px){.wide{display:none}}</style></head><body><p class='hidden'>Hidden</p><p>Middle chapter with <em>emphasis</em>.</p><p class='wide'>Only narrow</p><table><tr><td rowspan='2'>Spanning</td><td>A</td></tr><tr><td>B</td></tr></table><p>End of middle</p></body></html>"),
        ("c","<html><body><math><mfrac><mi>up</mi><mi>down</mi></mfrac></math><img src='img.png'/><p>End of book.</p></body></html>"),
    ]),"Chapters").unwrap());
    let location = |text: &str| Anchor {
        block: doc
            .blocks
            .iter()
            .position(|b| b.text.contains(text))
            .unwrap(),
        byte: 0,
    };
    let mut reader = Pager::new(&doc, 80, 7, LayoutMode::Single);
    let snapshot = |p: &Pager| {
        let view = p.viewport_rows();
        (0..view.columns())
            .flat_map(|c| view.lines(c))
            .map(|l| format!("{l:?}"))
            .collect::<Vec<_>>()
    };
    for (label, width, height, mode) in [
        ("Last words", 84, 6, LayoutMode::Spread),
        ("Middle chapter", 51, 9, LayoutMode::Single),
        ("End of middle", 105, 8, LayoutMode::Spread),
        ("End of book", 33, 5, LayoutMode::Scroll),
        ("Hidden", 64, 9, LayoutMode::Single),
    ] {
        let at = location(label);
        reader.seek(at);
        // Replace an unfinished resize, like two successive mouse drags.
        reader.resize_for_display(&doc, 39, 6, LayoutMode::Single, 4);
        reader.start_pagination();
        reader.resize_for_display(&doc, width, height, mode, 4);
        let mut full = Pager::new(&doc, width, height, mode);
        full.seek(at);
        if label == "Middle chapter" {
            assert!(reader.is_paginating());
        }
        assert_eq!(snapshot(&reader), snapshot(&full), "first frame at {label}");
        assert_eq!(reader.anchor(), at);
        if mode == LayoutMode::Spread {
            reader.start_pagination();
        }
        reader.finish_resize();
        assert_eq!(
            snapshot(&reader),
            snapshot(&full),
            "complete frame at {label}"
        );
        assert_eq!(reader.page_number(), full.page_number());
        assert_eq!(reader.page_count(), full.page_count());
        assert_eq!(
            reader.search_index(&doc).matches("Only narrow").len(),
            full.search_index(&doc).matches("Only narrow").len()
        );
        reader.previous();
        full.previous();
        assert_eq!(reader.anchor(), full.anchor());
        assert_eq!(snapshot(&reader), snapshot(&full));
        reader.next();
        full.next();
        assert_eq!(reader.anchor(), full.anchor());
        assert_eq!(reader.at_end(), full.at_end());
    }
}

#[test]
fn fb2_faces_survive_wrapping_and_resume_without_changing_source_offsets() {
    use botui_core::{LayoutMode, Pager, ReflowOptions};
    let doc = super::fb2::parse("<FictionBook><body><section><p>  Начало <emphasis>курсив <strong>жирный &amp; курсив</strong></emphasis> обычный. </p></section></body></FictionBook>".as_bytes(), "").unwrap();
    let mut plain = doc.clone();
    plain.inline.clear();
    for width in [9, 20, 80] {
        let pager = Pager::new(&doc, width, 100, LayoutMode::Single);
        let baseline = Pager::new(&plain, width, 100, LayoutMode::Single);
        assert_eq!(
            pager
                .lines(0)
                .iter()
                .map(|l| (&l.text, l.anchor()))
                .collect::<Vec<_>>(),
            baseline
                .lines(0)
                .iter()
                .map(|l| (&l.text, l.anchor()))
                .collect::<Vec<_>>()
        );
        let mut styled_letters = String::new();
        for line in pager.lines(0) {
            for span in &line.spans {
                let at = span.source.unwrap();
                assert_eq!(
                    &line.text[span.bytes.clone()],
                    &doc.blocks[at.block].text[at.byte..span.source_end]
                );
                assert_eq!(doc.cfi(at), plain.cfi(at));
                if let Some(cfi) = doc.cfi(at) {
                    assert_eq!(doc.resolve_cfi(&cfi), Some(at));
                }
                if span.face.italic {
                    styled_letters.push_str(&line.text[span.bytes.clone()]);
                }
                if span.face.bold {
                    assert!(span.face.italic);
                }
            }
        }
        assert_eq!(
            styled_letters.split_whitespace().collect::<String>(),
            "курсивжирный&курсив"
        );
    }
    let at = botui_core::Anchor {
        block: 0,
        byte: doc.blocks[0].text.find("жирный").unwrap(),
    };
    let reopened = Pager::with_start(
        &doc,
        20,
        30,
        LayoutMode::Single,
        ReflowOptions {
            typography: Default::default(),
            repair_printed: true,
            inline_images: true,
            paragraph_indent: 2,
            compact_spacing: true,
            paragraph_spacing: None,
        },
        Some(at),
    );
    assert!(reopened.lines(0)[0].text.starts_with("жирный"));
    assert!(reopened.lines(0)[0].spans[0].face.bold);
}

#[test]
fn svg_figures_are_images_not_prose_and_keep_source_positions() {
    use botui_core::{Anchor, LayoutMode, Pager, Target};
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 120"><defs><path id="line" d="M10 10L100 100"/></defs><use href="#line" stroke="blue"/><text x="15" y="25">VectorLabel</text></svg>"##;
    let html = format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Before the diagram.</p>{svg}<p id="after">After the diagram.</p><img src="fig.svg"/><p>Second caption.</p><object type="image/svg+xml" data="fig.svg"/><p>End.</p></body></html>"#
    );
    let input = archive(&[
        (
            "META-INF/container.xml",
            r#"<container><rootfile full-path="book.opf"/></container>"#,
        ),
        (
            "book.opf",
            r#"<package><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/><item id="i" href="fig.svg" media-type="image/svg+xml"/></manifest><spine><itemref idref="c"/></spine></package>"#,
        ),
        ("c.xhtml", &html),
        ("fig.svg", svg),
    ]);
    let doc = read_book(input, "SVG book").unwrap();
    assert_eq!(
        doc.blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>(),
        [
            "Before the diagram.",
            "After the diagram.",
            "Second caption.",
            "End."
        ]
    );
    assert_eq!(doc.references.len(), 3);
    assert!(
        matches!(&doc.references[0].target,Target::Image(s) if s=="botui-inline-svg:0:c.xhtml")
    );
    let at = Anchor { block: 1, byte: 6 };
    assert_eq!(doc.cfi(at).as_deref(), Some("epubcfi(/4/2!/2/6/1:6)"));
    assert_eq!(doc.resolve_cfi(&doc.cfi(at).unwrap()), Some(at));
    for mode in [LayoutMode::Single, LayoutMode::Spread, LayoutMode::Scroll] {
        let mut p = Pager::new(&doc, 80, 24, mode);
        let mut found = std::collections::BTreeSet::new();
        loop {
            for col in 0..p.columns() {
                for line in p.lines(col) {
                    assert!(!line.text.contains("VectorLabel"));
                    for f in &line.figures {
                        found.insert(f.reference);
                    }
                }
            }
            if p.at_end() {
                break;
            }
            p.next();
        }
        assert_eq!(found, [0, 1, 2].into_iter().collect(), "{mode:?}");
    }
}

#[test]
fn svg_archive_resources_are_self_contained_and_bounded() {
    use super::svg;
    let inline = r##"<html xmlns="http://www.w3.org/1999/xhtml"><body><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="240" height="120"><title>Chart</title><g transform="translate(10 5)"><image xlink:href="../img/pixel.png" width="40" height="40"/><path d="M0 0L10 10"/></g><image href="file:///etc/passwd"/><image href="https://example.org/a.png"/></svg></body></html>"##;
    let mut input = archive(&[
        ("text/c.xhtml", inline),
        ("img/pixel.png", "PNG-placeholder"),
    ]);
    let mut zip = zip::ZipArchive::new(&mut input).unwrap();
    let out = svg::read(&mut zip, "botui-inline-svg:0:text/c.xhtml", &mut (1 << 20)).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("data:text/plain;base64,UE5HLXBsYWNlaG9sZGVy"));
    assert!(xml.contains("translate(10 5)") && xml.contains("<path"));
    assert!(!xml.contains("file://") && !xml.contains("example.org"));
    roxmltree::Document::parse(&xml).unwrap();
    assert!(svg::read(&mut zip, "botui-inline-svg:0:text/c.xhtml", &mut 10).is_err());
    assert_eq!(svg::header_size(xml.as_bytes()), Some((240, 120)));
    assert_eq!(
        svg::header_size(br#"<svg viewBox="0 0 400 200" width="2in">"#),
        Some((192, 96))
    );
}

#[test]
fn epub_inline_math_and_code_share_the_prose_baseline() {
    use botui_core::{LayoutMode, Pager};
    let doc = styled_fixture(
        "<h1>Unit</h1><p>Before <math><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></math> after <code>run</code>.</p><h2>Details</h2><pre><code>$x$</code></pre>",
        "",
    );
    assert_eq!(doc.chapter_starts.iter().copied().collect::<Vec<_>>(), [0]);
    for mode in [LayoutMode::Single, LayoutMode::Spread, LayoutMode::Scroll] {
        let pager = Pager::new(&doc, 100, 24, mode);
        let rows = pager.lines(0);
        let line = rows
            .iter()
            .find(|l| l.text.contains("Before E = mc² after run."))
            .unwrap();
        let code = line
            .spans
            .iter()
            .filter(|s| s.face.role == 10)
            .map(|s| &line.text[s.bytes.clone()])
            .collect::<String>();
        assert_eq!(code, "run");
        let cells = line.text_cells(&doc);
        let full = botui_core::TextRange::new(
            cells.first().unwrap().source.start,
            cells.last().unwrap().source.end,
        );
        assert!(doc.clipboard_text(full).contains("mc²"));
        assert!(rows.iter().any(|l| l.text.contains("Details")));
        let raw = rows.iter().find(|l| l.text.contains("$x$")).unwrap();
        assert!(raw.spans.iter().all(|s| s.face.role != 10));
    }
}

#[test]
fn scrollable_epub_table_keeps_rows_and_respects_hidden_media() {
    use botui_core::{LayoutMode, Pager};
    let doc = styled_fixture(
        "<blockquote><p>Quoted</p></blockquote><table><thead><tr><th>Name</th><th>Formula</th></tr></thead><tbody><tr><td>Energy</td><td>First</td></tr><tr><td>Euler</td><td>Second</td></tr></tbody></table><p>End.</p>",
        "table{display:block;overflow-x:auto;width:100%}@media(max-width:300px){table{display:none}}",
    );
    let mut pager = Pager::new(&doc, 64, 40, LayoutMode::Single);
    for width in [64, 29, 48] {
        pager.relayout(&doc, width, 40, LayoutMode::Single);
        let rows = pager.lines(0);
        assert!(
            rows.iter()
                .any(|r| r.text.contains("Quoted") && r.text.contains('│'))
        );
        if width == 29 {
            assert!(rows.iter().all(|r| !r.text.contains("Euler")));
        } else {
            let row = |a: &str, b: &str| {
                rows.iter()
                    .position(|r| r.text.contains(a) && r.text.contains(b))
                    .unwrap()
            };
            let header = row("Name", "Formula");
            assert!(rows[header].spans.iter().any(|s| s.face.role == 18));
            assert!(
                rows[header + 1]
                    .surfaces
                    .iter()
                    .any(|s| matches!(s, botui_core::Surface::TableHeaderRule { .. }))
            );
            assert!(rows[header + 2].text.trim().is_empty());
            assert_eq!(row("Energy", "First"), header + 3);
            assert!(rows[..header - 1].iter().all(|r| {
                !r.surfaces
                    .iter()
                    .any(|s| matches!(s, botui_core::Surface::TableDivider { .. }))
            }));
            assert!(row("Energy", "First") < row("Euler", "Second"));
        }
    }
}

#[test]
fn epub_display_exponent_sits_above_the_integrand() {
    use botui_core::{LayoutMode, Pager};
    let doc = styled_fixture(
        r#"<p>Integral</p><math display="block"><msubsup><mo>∫</mo><mn>0</mn><mi>∞</mi></msubsup><msup><mi>e</mi><mrow><mo>−</mo><msup><mi>x</mi><mn>2</mn></msup></mrow></msup><mi>d</mi><mi>x</mi><mo>=</mo><mfrac><mi>pi</mi><mn>2</mn></mfrac></math>"#,
        "",
    );
    let pager = Pager::new(&doc, 60, 24, LayoutMode::Single);
    let rows = pager.lines(0);
    let base = rows.iter().position(|r| r.text.contains('∫')).unwrap();
    assert!(rows[base].text.contains('e') && rows[base].text.contains("dx ="));
    assert!(rows[base - 1].text.contains("−x²") && rows[base - 1].text.contains('∞'));
}

#[test]
fn epub_heading_rules_use_the_markup_level_in_both_layout_engines() {
    use botui_core::{LayoutMode, Pager, Surface};
    let body = "<h2>Opening</h2><h3>Detail</h3><h1>Part</h1><h4>Aside</h4><p>End</p>";
    let plain = read_book(archive(&[
        ("META-INF/container.xml","<container><rootfile full-path='p'/></container>"),
        ("p","<package><manifest><item id='c' href='c' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='c'/></spine></package>"),
        ("c",&format!("<body>{body}</body>"))
    ]),"Plain").unwrap();
    for doc in [plain, styled_fixture(body, "")] {
        let base = Pager::new(&doc, 60, 60, LayoutMode::Scroll).reflow_options();
        for enabled in [false, true] {
            let mut options = base;
            options.typography.heading_rules = enabled;
            let pager = Pager::with_options(&doc, 60, 60, LayoutMode::Scroll, options);
            let rows = pager.lines(0);
            let main = rows.iter().position(|r| r.text.contains("Part")).unwrap();
            let next = rows.iter().position(|r| r.text.contains("Aside")).unwrap();
            assert_eq!(next - main, 2 + usize::from(enabled));
            assert!(rows[next - 1].text.trim().is_empty() && rows[next - 1].surfaces.is_empty());
            let rules: Vec<_> = pager
                .lines(0)
                .iter()
                .filter_map(|row| {
                    if row.text.contains("══") {
                        return Some(true);
                    }
                    if row.text.contains("──") {
                        return Some(false);
                    }
                    row.surfaces.iter().find_map(|s| match s {
                        Surface::HeadingRule { double } => Some(*double),
                        _ => None,
                    })
                })
                .collect();
            assert_eq!(rules, if enabled { vec![false, true] } else { vec![] });
        }
    }
}

#[test]
fn nested_epub_fraction_keeps_all_operands_across_narrow_pages() {
    use botui_core::{LayoutMode, Pager};
    let terms = "<mi>a</mi><mo>+</mo><mi>b</mi><mo>+</mo><mi>c</mi><mo>+</mo><mi>d</mi><mo>+</mo><mi>f</mi><mo>+</mo><mi>g</mi>";
    let doc = styled_fixture(
        &format!(
            "<p>Lead</p><math display='block'><mrow><mfrac><mfrac><mi>Q</mi><mi>W</mi></mfrac><mi>Z</mi></mfrac><mo>=</mo><mrow>{terms}</mrow></mrow></math><p>Tail</p>"
        ),
        "",
    );
    for width in [12, 28, 64] {
        let render = |height| {
            let mut p = Pager::new(&doc, width, height, LayoutMode::Single);
            let mut lines = Vec::new();
            for _ in 0..p.page_count() {
                lines.extend(p.lines(0).iter().map(|l| l.text.clone()));
                p.next();
            }
            lines
        };
        let tall = render(64);
        assert_eq!(
            render(3),
            tall,
            "different content at page boundary, width {width}"
        );
        let equation = tall.join("").replace("Lead", "").replace("Tail", "");
        for c in "QWZabcdfg=".chars() {
            assert_eq!(equation.matches(c).count(), 1, "width {width}: {tall:?}");
        }
    }
}

#[test]
fn copying_epub_code_restores_whitespace_across_inline_spans_and_wraps() {
    use botui_core::{Anchor, LayoutMode, Pager, TextRange};
    let raw = "first line\n\n    indented long line with words\n\tlast line";
    let doc = styled_fixture(
        "<pre><code>first line\n\n    <b>indented</b> long line with words\n\tlast line</code></pre>",
        "",
    );
    let canonical = &doc.blocks[0].text;
    for width in [12, 32, 64] {
        let p = Pager::new(&doc, width, 30, LayoutMode::Single);
        let cells: Vec<_> = p.lines(0).iter().flat_map(|l| l.text_cells(&doc)).collect();
        let range = TextRange::new(
            cells.first().unwrap().source.start,
            cells.last().unwrap().source.end,
        );
        assert_eq!(doc.clipboard_text(range), raw);
        assert_eq!(doc.text_range(range), canonical.as_str());
        let start = canonical.find("words").unwrap();
        let tail = TextRange::new(
            Anchor {
                block: 0,
                byte: start,
            },
            range.end,
        );
        assert_eq!(doc.clipboard_text(tail), "words\n\tlast line");
    }
}
