use crate::{Anchor, Block, Document, LayoutMode, Pager, Reference, Target};

// Record every visible spread in reading order, including an empty right page.
fn forward_breaks(pager: &mut Pager) -> Vec<[usize; 3]> {
    let mut spreads = Vec::new();
    loop {
        spreads.push(pager.complete_view().bounds);
        if pager.at_end() {
            return spreads;
        }
        pager.next();
    }
}

fn short_document(with_chapter: bool) -> Document {
    Document {
        blocks: (0..18)
            .map(|i| Block {
                text: format!("Passage {i}."),
                heading: false,
            })
            .collect(),
        chapter_starts: if with_chapter {
            [0].into()
        } else {
            Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn scrolling_into_a_single_page_does_not_add_a_page_after_returning_to_the_start() {
    for with_chapter in [false, true] {
        let doc = short_document(with_chapter);
        let mut regular = Pager::with_reflow(&doc, 50, 24, LayoutMode::Single, false);
        let pages = forward_breaks(&mut regular);
        assert_eq!(pages.len(), 2, "fixture must occupy two normal pages");

        let mut pager = Pager::with_reflow(&doc, 50, 24, LayoutMode::Scroll, false);
        pager.scroll(4);
        let anchor = pager.anchor();
        assert_ne!(anchor, Anchor::default());
        pager.relayout(&doc, 50, 24, LayoutMode::Single);
        assert_eq!(pager.anchor(), anchor);
        assert!(pager.first > pages[0][0] && pager.first < pages[1][0]);

        pager.previous();
        assert_eq!(pager.complete_view().bounds, pages[0]);
        pager.next();
        assert_eq!(pager.complete_view().bounds, pages[1]);
        assert!(pager.at_end());
        pager.previous();
        assert_eq!(pager.complete_view().bounds, pages[0]);
        pager.next();
        assert_eq!(pager.complete_view().bounds, pages[1]);
    }
}

#[test]
fn scrolling_into_one_spread_does_not_leave_an_extra_partial_spread() {
    for with_chapter in [false, true] {
        let doc = short_document(with_chapter);
        let mut regular = Pager::with_reflow(&doc, 104, 24, LayoutMode::Spread, false);
        let spreads = forward_breaks(&mut regular);
        assert_eq!(spreads.len(), 1, "fixture must fit in one full spread");

        let mut pager = Pager::with_reflow(&doc, 50, 24, LayoutMode::Scroll, false);
        pager.scroll(4);
        let anchor = pager.anchor();
        assert_ne!(anchor, Anchor::default());
        pager.relayout(&doc, 104, 24, LayoutMode::Spread);
        assert_eq!(pager.anchor(), anchor);
        assert!(pager.first > 0);

        pager.previous();
        assert_eq!(pager.complete_view().bounds, spreads[0]);
        assert!(pager.at_end());
        pager.next();
        assert_eq!(pager.complete_view().bounds, spreads[0]);
        pager.previous();
        assert_eq!(pager.complete_view().bounds, spreads[0]);
    }
}

#[test]
fn chapter_percentage_uses_the_first_visible_source_line_across_reflow() {
    let mut doc = Document {
        blocks: (0..9)
            .map(|i| Block {
                text: format!("Passage {i}."),
                heading: matches!(i, 1 | 5),
            })
            .collect(),
        chapter_starts: [1, 5].into(),
        ..Default::default()
    };
    let mut pager = Pager::with_reflow(&doc, 60, 5, LayoutMode::Single, false);
    for (mode, width, height) in [
        (LayoutMode::Single, 48, 6),
        (LayoutMode::Spread, 140, 12),
        (LayoutMode::Scroll, 52, 7),
    ] {
        pager.relayout(&doc, width, height, mode);
        pager.seek(Anchor::default());
        assert_eq!(
            pager.chapter_progress().and_then(|s| s.percent),
            None,
            "front matter"
        );
        for (block, expected) in [(1, 0), (3, 50), (4, 75), (5, 0), (7, 50), (8, 75)] {
            // Seeking a word within a line still reports the start of that visible line.
            let byte = if matches!(block, 3 | 7) { 6 } else { 0 };
            pager.seek(Anchor { block, byte });
            assert_eq!(
                pager.chapter_progress().and_then(|s| s.percent),
                Some(expected),
                "{mode:?} block {block}"
            );
        }
    }
    doc.chapter_starts.clear();
    doc.blocks.iter_mut().for_each(|b| b.heading = false);
    assert_eq!(
        Pager::new(&doc, 60, 5, LayoutMode::Single)
            .chapter_progress()
            .and_then(|s| s.percent),
        None
    );
}

#[test]
fn chapter_percentage_accepts_outline_destinations_within_one_source_block() {
    let text = "aaaa bbbb cccc dddd eeee ffff gggg hhhh";
    let doc = Document {
        blocks: vec![Block {
            text: text.to_owned(),
            heading: false,
        }],
        contents: [5, 25]
            .map(|byte| crate::ContentsEntry::passage(Anchor { block: 0, byte }, "Heading".into()))
            .into(),
        ..Default::default()
    };
    let mut pager = Pager::with_reflow(&doc, 5, 3, LayoutMode::Single, false);
    for (byte, expected) in [
        (0, None),
        (5, Some(0)),
        (15, Some(50)),
        (25, Some(0)),
        (35, Some(71)),
    ] {
        pager.seek(Anchor { block: 0, byte });
        assert_eq!(
            pager.chapter_progress().and_then(|s| s.percent),
            expected,
            "offset {byte}"
        );
    }
}

#[test]
fn chapter_progress_pairs_the_printed_title_with_its_outline_destination() {
    let blocks = [
        ("Знак Единорога", true),
        ("3", false),
        ("ANATOMY", true),
        ("First passage", false),
        ("Chapter IV", true),
        ("Second passage", false),
        ("Anonymous opening paragraph", false),
        ("Рука Оберона", true),
        ("Глава первая", true),
        ("45", false),
        ("Interlude", true),
    ];
    let doc = Document {
        blocks: blocks
            .map(|(s, heading)| Block {
                text: s.to_owned(),
                heading,
            })
            .into(),
        chapter_starts: [0, 1, 4, 6, 7, 8, 9].into(),
        contents: [
            (0, "Знак Единорога"),
            (2, "3. Anatomy"),
            (4, "Physiology"),
            (7, "Рука Оберона"),
            (10, "45. Other heading"),
        ]
        .map(|(block, title)| {
            crate::ContentsEntry::passage(Anchor { block, byte: 0 }, title.to_owned())
        })
        .into(),
        ..Default::default()
    };
    let mut pager = Pager::with_reflow(&doc, 80, 7, LayoutMode::Spread, false);
    for (block, title, toc_title) in [
        (0, Some("Знак Единорога"), Some("Знак Единорога")),
        (1, Some("ANATOMY"), Some("3. Anatomy")),
        (4, Some("Chapter IV"), Some("Physiology")),
        (6, None, None),
        (7, Some("Рука Оберона"), Some("Рука Оберона")),
        (8, Some("Глава первая"), None),
        (9, None, None),
    ] {
        pager.seek(Anchor { block, byte: 0 });
        let progress = pager.chapter_progress().unwrap();
        assert_eq!(
            (progress.title, progress.toc_title, progress.percent),
            (title, toc_title, Some(0))
        );
    }
}

fn check_direction_changes(pager: &mut Pager, spreads: &[[usize; 3]]) {
    let mut index = spreads
        .iter()
        .position(|view| view[0] == pager.first)
        .unwrap();
    for direction in [-1, -1, 1, 1, 1, -1, -1, 1] {
        if direction < 0 {
            pager.previous();
            index = index.saturating_sub(1);
        } else {
            pager.next();
            index = (index + 1).min(spreads.len() - 1);
        }
        assert_eq!(pager.complete_view().bounds, spreads[index]);
    }
}

#[test]
fn reversing_after_an_exact_jump_rejoins_regular_pages_without_skipping_rows() {
    let doc = Document {
        blocks: (0..72)
            .map(|i| Block {
                heading: matches!(i, 0 | 23 | 50),
                text: format!("Passage {i} in the book."),
            })
            .collect(),
        chapter_starts: [0, 23, 50].into(),
        references: [2, 26, 36, 53]
            .into_iter()
            .map(|source| Reference {
                source,
                bytes: None,
                backlink: false,
                label: "Illustration".into(),
                target: Target::Image("plate.png".into()),
            })
            .collect(),
        ..Default::default()
    };
    for mode in [LayoutMode::Single, LayoutMode::Spread] {
        for height in [6, 11, 17] {
            let mut pager = Pager::new(&doc, 96, height, mode);
            let breaks = forward_breaks(&mut pager);
            pager.seek(Anchor::default());
            check_direction_changes(&mut pager, &breaks);
            let mut crossed_overlap = false;
            for block in 0..doc.blocks.len() {
                pager.seek(Anchor { block, byte: 0 });
                while pager.first > 0 {
                    let original = pager.complete_view().bounds;
                    pager.previous();
                    let previous = pager.complete_view().bounds;
                    assert!(pager.first < original[0]);
                    let end = pager.complete_view().end();
                    assert!(
                        end >= original[0],
                        "lost rows at {mode:?}/{height}, block {block}: {previous:?} before {original:?}"
                    );
                    crossed_overlap |= end > original[0];
                    let chapter_opening =
                        pager.first == 0 || pager.lines[pager.first].chapter_start;
                    if end > original[0] {
                        assert!(
                            chapter_opening,
                            "interior overlap at {previous:?} before {original:?}"
                        );
                        assert!(pager.positioned_spreads.is_empty());
                        assert!(pager.regular_views().binary_search(&pager.first).is_ok());
                    }
                    let regular_opening =
                        breaks.iter().find(|view| view[0] == pager.first).copied();
                    pager.next();
                    if chapter_opening {
                        let opening = regular_opening.unwrap();
                        let expected = opening[pager.columns()];
                        if expected < pager.lines.len() {
                            assert_eq!(pager.first, expected);
                            pager.previous();
                            assert_eq!(pager.complete_view().bounds, opening);
                        }
                        continue;
                    }
                    if end == original[0] {
                        assert_eq!(pager.complete_view().bounds, original);
                    } else if end < pager.lines.len() {
                        assert_eq!(pager.first, end, "overlap must not create an extra page");
                    } else {
                        assert_eq!(pager.complete_view().bounds, previous);
                    }
                    if end < pager.lines.len() {
                        pager.previous();
                        assert_eq!(pager.complete_view().bounds, previous);
                    }
                }
            }
            assert!(
                crossed_overlap,
                "no interior transition exercised at {mode:?}/{height}"
            );
        }
    }
}

#[test]
fn plain_alignment_keeps_source_words_and_justifies_only_wrapped_lines() {
    use crate::TextAlignment::*;
    let doc = Document {
        blocks: vec![Block {
            text: "один  два три четыре".into(),
            heading: false,
        }],
        ..Default::default()
    };
    let base = Pager::new(&doc, 10, 12, LayoutMode::Single).reflow_options();
    for (alignment, indent, text) in [
        (Book, 0, "один два"),
        (Left, 0, "один два"),
        (Right, 2, "один два"),
        (Center, 1, "один два"),
        (Justify, 0, "один   два"),
    ] {
        let mut options = base;
        options.typography.alignment = alignment;
        let book = super::BookLayout::new(&doc, options, None);
        let rows = book.lines(
            &doc,
            crate::Layout::new(10, 12, LayoutMode::Single, 0),
            0..1,
        );
        assert_eq!((rows[0].indent, rows[0].text.as_str()), (indent, text));
        let last = rows[0].text_cells(&doc).pop().unwrap();
        assert_eq!(last.source.end.byte, "один  два".len());
        assert_eq!(rows.last().unwrap().text, "три четыре");
    }
}

#[test]
fn switching_from_single_page_keeps_the_same_passage_on_the_left() {
    for starts in [vec![], vec![0, 9]] {
        let doc = Document {
            blocks: (0..31)
                .map(|i| Block {
                    text: format!("Line {i}"),
                    heading: false,
                })
                .collect(),
            chapter_starts: starts.into_iter().collect(),
            ..Default::default()
        };
        let mut pager = Pager::new(&doc, 50, 6, LayoutMode::Single);
        for index in [1, 2, 5, 7] {
            pager.relayout(&doc, 50, 6, LayoutMode::Single);
            pager.seek(Anchor::default());
            for _ in 0..index {
                pager.next();
            }
            let anchor = pager.anchor();
            let text = pager
                .lines(0)
                .iter()
                .find(|row| !row.text.is_empty())
                .unwrap()
                .text
                .clone();
            for (mode, width) in [
                (LayoutMode::Spread, 104),
                (LayoutMode::Single, 50),
                (LayoutMode::Spread, 104),
                (LayoutMode::Scroll, 50),
                (LayoutMode::Spread, 104),
                (LayoutMode::Single, 50),
            ] {
                pager.relayout(&doc, width, 6, mode);
                assert_eq!(pager.anchor(), anchor, "{mode:?}, page {index}");
                assert_eq!(
                    pager
                        .lines(0)
                        .iter()
                        .find(|row| !row.text.is_empty())
                        .unwrap()
                        .text,
                    text,
                    "{mode:?}, page {index}"
                );
            }
        }
    }
}

#[test]
fn different_width_switches_keep_the_word_and_reverse_until_the_chapter_opening() {
    let body = (0..180)
        .map(|i| format!("w{i:03}"))
        .collect::<Vec<_>>()
        .join(" ");
    let doc = Document {
        blocks: vec![Block {
            text: body,
            heading: false,
        }],
        chapter_starts: [0].into(),
        ..Default::default()
    };
    let mut pager = Pager::new(&doc, 49, 3, LayoutMode::Scroll);
    pager.scroll(11);
    let at = pager.lines(0)[0].anchor();
    for (mode, width) in [
        (LayoutMode::Single, 32),
        (LayoutMode::Spread, 86),
        (LayoutMode::Scroll, 49),
        (LayoutMode::Spread, 86),
    ] {
        pager.switch_mode(&doc, width, 3, mode, 4);
        assert_eq!(pager.lines(0)[0].anchor(), at);
        assert_eq!(
            pager.lines(0)[0].text.split_whitespace().next(),
            doc.blocks[0].text[at.byte..].split_whitespace().next()
        );
    }
    let mut last = at;
    loop {
        pager.previous();
        let first = pager.lines(0)[0].anchor();
        if first.byte == 0 {
            break;
        }
        let tail = pager.lines(1).last().unwrap();
        assert_eq!(
            tail.text.split_whitespace().count(),
            8,
            "short seam before {last:?}"
        );
        let expected = doc.blocks[0].text[..last.byte]
            .split_whitespace()
            .next_back()
            .unwrap();
        assert_eq!(tail.text.split_whitespace().next_back(), Some(expected));
        pager.next();
        assert_eq!(pager.lines(0)[0].anchor(), last);
        pager.previous();
        last = first;
    }
    pager.next();
    let mut ordinary = Pager::with_start_and_gap(
        &doc,
        86,
        3,
        LayoutMode::Spread,
        4,
        pager.reflow_options(),
        None,
    );
    ordinary.next();
    assert_eq!(pager.lines(0)[0].anchor(), ordinary.lines(0)[0].anchor());
}

#[test]
fn optional_prose_hyphenation_preserves_source_text_and_clicks() {
    for text in [
        "One extraordinary opportunity",
        "Два замечательных предложения",
    ] {
        let doc = Document {
            blocks: vec![Block {
                text: text.into(),
                heading: false,
            }],
            ..Default::default()
        };
        let mut pager = Pager::new(&doc, 12, 12, LayoutMode::Single);
        let mut options = pager.reflow_options();
        options.typography.hyphenation = true;
        pager.reconfigure(&doc, 12, 12, LayoutMode::Single, 4, options);
        let rows = pager.lines(0);
        assert!(rows.iter().any(|l| l.text.ends_with('-')), "{rows:?}");
        let mut cells = rows.iter().flat_map(|l| l.text_cells(&doc));
        let range = crate::TextRange::new(
            cells.next().unwrap().source.start,
            cells.last().unwrap().source.end,
        );
        assert_eq!(doc.clipboard_text(range), text);
    }
}
