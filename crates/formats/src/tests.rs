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
    assert_eq!(book.image_pages, ["botui-inline-svg:0:a.xhtml", "i.jpg"]);
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
fn prose_preserves_words_indentation_footnotes_and_spread_page_turns() {
    use botui_core::{LayoutMode, Pager, ReflowOptions, Target};
    let mut body = String::from("<h1>Диалоги</h1>");
    for n in 0..36 {
        body.push_str(&format!("<p>— Реплика {n}. Кто видел <em>Кита 🐋 и Маяк</em> рядом с <a href='#n'>башней</a>?</p>"));
    }
    body.push_str("<p id='n'>Сноска к диалогу.</p>");
    let doc = styled_fixture(&body, "h1 {font-size:2.5em} p {text-indent:1em}");
    let options = ReflowOptions {
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 3,
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
fn justification_keeps_every_word_intact_at_terminal_widths() {
    use botui_core::{LayoutMode, Pager};
    let paragraph = "Миновав плац, я направился к дорожке, что вела в южный угол дворцового сада. Рядом стояла беседка. На деревянном столе лежала раскрытая книга.";
    let marked = paragraph.replace("южный", "ю<em>жн</em>ый");
    let doc = styled_fixture(&format!("<p>{marked}</p>"), "p { text-align:justify }");
    for width in 26..60 {
        let pager = Pager::new(&doc, width, 40, LayoutMode::Single);
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
            repair_printed: true,
            inline_images: false,
            paragraph_indent: 0,
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
        repair_printed: true,
        inline_images: true,
        paragraph_indent: 2,
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
            repair_printed: true,
            inline_images: true,
            paragraph_indent: 2,
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
