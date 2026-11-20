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
fn reversing_after_an_exact_jump_restores_the_same_spread_without_skipping_rows() {
    let doc = Document {
        blocks: (0..72)
            .map(|i| Block {
                heading: matches!(i, 0 | 23 | 50),
                text: format!("Passage {i} in the book."),
            })
            .collect(),
        chapter_starts: [0, 23, 50].into(),
        references: vec![Reference {
            source: 36,
            bytes: None,
            backlink: false,
            label: "Illustration".into(),
            target: Target::Image("plate.png".into()),
        }],
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
                    assert!(end >= original[0], "lost rows after {end}");
                    crossed_overlap |= end > original[0];
                    pager.next();
                    assert_eq!(pager.complete_view().bounds, original);
                    pager.previous();
                    assert_eq!(pager.complete_view().bounds, previous);
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
fn switching_from_single_page_opens_its_containing_spread() {
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
        let mut reference = Pager::new(&doc, 104, 6, LayoutMode::Spread);
        let spreads = forward_breaks(&mut reference);
        let mut pager = Pager::new(&doc, 50, 6, LayoutMode::Single);
        for index in [1, 2, 5, 7] {
            pager.seek(Anchor::default());
            for _ in 0..index {
                pager.next();
            }
            let row = pager.first;
            let expected = spreads
                .iter()
                .position(|b| (b[0]..b[2]).contains(&row))
                .unwrap();
            pager.relayout(&doc, 104, 6, LayoutMode::Spread);
            pager.pair_current_page();
            assert_eq!(pager.complete_view().bounds, spreads[expected]);
            pager.previous();
            assert_eq!(
                pager.complete_view().bounds,
                spreads[expected.saturating_sub(1)]
            );
            pager.next();
            assert_eq!(pager.complete_view().bounds, spreads[expected.max(1)]);
            pager.relayout(&doc, 50, 6, LayoutMode::Single);
        }
    }
}
