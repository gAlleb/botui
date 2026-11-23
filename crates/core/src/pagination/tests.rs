use crate::{Anchor, Block, Document, LayoutMode, Pager, PaginationMode, Reference, Target};

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
        assert_eq!(pager.complete_view().bounds, pages[0]);

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
        assert_eq!(pager.complete_view().bounds, spreads[0]);

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
        // Exercise progress from arbitrary first lines, including a word inside one.
        pager.set_pagination_mode(PaginationMode::KeepWord);
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
    pager.set_pagination_mode(PaginationMode::KeepWord);
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
fn jumps_and_direction_changes_always_use_the_same_chapter_pages() {
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
            for block in 0..doc.blocks.len() {
                let at = Anchor { block, byte: 0 };
                pager.seek_chapter(at);
                assert_eq!(pager.anchor(), at);
                check_direction_changes(&mut pager, &breaks);
            }
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
fn changing_modes_uses_the_containing_page_and_returns_to_its_visible_top_in_scroll() {
    let doc = short_document(true);
    let mut pager = Pager::new(&doc, 50, 6, LayoutMode::Single);
    pager.next();
    let at = pager.anchor();
    pager.switch_mode(&doc, 104, 6, LayoutMode::Spread, 4);
    let first = pager.lines(0)[0].anchor();
    assert_ne!(at, first);
    assert_eq!(pager.anchor(), at);
    assert!(pager.lines(1).iter().any(|l| l.anchor() == at));
    pager.switch_mode(&doc, 50, 6, LayoutMode::Scroll, 4);
    assert_eq!(pager.anchor(), first);
    assert_eq!(pager.lines(0)[0].anchor(), first);
}

#[test]
fn keep_word_switches_and_forward_turns_rejoin_chapter_pages_on_the_first_backward_turn() {
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
    pager.set_pagination_mode(PaginationMode::KeepWord);
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
    let mut regular = Pager::new(&doc, 86, 3, LayoutMode::Spread);
    for forwards in [0, 2] {
        if forwards > 0 {
            pager.seek_chapter(at);
            pager.switch_mode(&doc, 49, 3, LayoutMode::Scroll, 4);
            pager.scroll(11);
            pager.switch_mode(&doc, 86, 3, LayoutMode::Spread, 4);
        }
        for _ in 0..forwards {
            pager.next();
        }
        let current = pager.lines(0)[0].anchor();
        regular.seek_chapter(current);
        if regular.visible_position().anchor == current {
            regular.previous();
        }
        pager.previous();
        assert_eq!(pager.complete_view().bounds, regular.complete_view().bounds);
        assert!(pager.natural.is_none());
        assert!(!pager.exact_view);
        pager.next();
        regular.next();
        assert_eq!(pager.complete_view().bounds, regular.complete_view().bounds);
    }
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

#[test]
fn an_exact_reading_position_does_not_shorten_prose_at_earlier_illustrations() {
    let doc = Document {
        blocks: (0..280)
            .map(|i| Block {
                heading: false,
                text: format!("Passage {i}."),
            })
            .collect(),
        references: [17, 52, 101, 143, 191, 231]
            .into_iter()
            .map(|source| Reference {
                source,
                bytes: None,
                backlink: false,
                label: "Plate".into(),
                target: Target::Image("plate.png".into()),
            })
            .collect(),
        ..Default::default()
    };
    for mode in [LayoutMode::Single, LayoutMode::Spread] {
        let mut pager = Pager::new(&doc, 92, 16, mode);
        for block in 245..262 {
            pager.seek(Anchor { block, byte: 0 });
            let mut short_prose_columns = 0;
            while pager.first > 0 {
                pager.previous();
                let view = pager.complete_view();
                for col in 0..view.columns() {
                    let lines = view.lines(col);
                    let next = view.bounds[col + 1];
                    if pager.first != 0
                        && lines.len() < pager.viewport()
                        && lines.last().is_some_and(|line| line.figures.is_empty())
                        && pager.lines[next].figures.is_empty()
                    {
                        short_prose_columns += 1;
                    }
                }
            }
            assert!(
                short_prose_columns == 0,
                "{mode:?} at {block}: {short_prose_columns}"
            );
        }
    }
}

#[test]
fn a_search_leaves_the_temporary_opening_even_when_the_address_did_not_change() {
    let doc = Document {
        blocks: vec![Block {
            text: (0..170)
                .map(|i| format!("word{i:03}"))
                .collect::<Vec<_>>()
                .join(" "),
            heading: false,
        }],
        ..Default::default()
    };
    let at = Anchor {
        block: 0,
        byte: doc.blocks[0].text.find("word036").unwrap(),
    };
    for mode in [LayoutMode::Single, LayoutMode::Spread] {
        let mut normal = Pager::new(&doc, 86, 4, mode);
        normal.seek_chapter(at);
        let mut pager = Pager::for_display_position_with_pagination(
            &doc,
            (86, 4),
            mode,
            4,
            normal.reflow_options(),
            Some(crate::ViewPosition {
                anchor: at,
                illustration: None,
            }),
            (1, 2),
            PaginationMode::KeepWord,
        );
        assert_eq!(pager.visible_position().anchor, at);
        pager.seek_chapter(at);
        assert_eq!(pager.anchor(), at);
        assert_eq!(pager.complete_view().bounds, normal.complete_view().bounds);
        assert!(pager.natural.is_none());
        assert!(!pager.exact_view);
        pager.previous();
        normal.previous();
        assert_eq!(pager.complete_view().bounds, normal.complete_view().bounds);
    }
}

#[test]
fn chapter_resume_keeps_the_logical_word_when_the_view_begins_with_a_picture() {
    let mut doc = short_document(true);
    doc.references.push(Reference {
        source: 2,
        bytes: None,
        backlink: false,
        label: "Plate".into(),
        target: Target::Image("plate.png".into()),
    });
    let mut pager = Pager::new(&doc, 80, 6, LayoutMode::Single);
    let at = Anchor { block: 2, byte: 3 };
    pager.seek_chapter(at);
    assert!(pager.visible_position().illustration.is_some());
    let position = pager.position();
    assert_eq!(
        position,
        crate::ViewPosition {
            anchor: at,
            illustration: None
        }
    );
    for height in [7, 8, 6] {
        pager.relayout(&doc, 80, height, LayoutMode::Single);
        assert_eq!(pager.position(), position);
        let mut ordinary = Pager::new(&doc, 80, height, LayoutMode::Single);
        ordinary.seek_chapter(at);
        assert_eq!(
            pager.complete_view().bounds,
            ordinary.complete_view().bounds
        );
    }
    let at_opening = Anchor { block: 0, byte: 4 };
    pager.seek_chapter(at_opening);
    pager.previous();
    assert_eq!(pager.anchor(), at_opening);
}

#[test]
fn repeated_resizes_keep_the_same_origin_until_the_reader_navigates() {
    use std::sync::Arc;
    let doc = Arc::new(Document {
        blocks: vec![Block {
            text: (0..180)
                .map(|i| format!("w{i:03}"))
                .collect::<Vec<_>>()
                .join(" "),
            heading: false,
        }],
        ..Default::default()
    });
    let at = Anchor {
        block: 0,
        byte: doc.blocks[0].text.find("w083").unwrap(),
    };
    for policy in [PaginationMode::Chapter, PaginationMode::KeepWord] {
        let mut pager = Pager::new(&doc, 65, 7, LayoutMode::Single);
        pager.set_pagination_mode(policy);
        pager.seek_chapter(at);
        let target = match policy {
            PaginationMode::Chapter => at,
            PaginationMode::KeepWord => pager.visible_position().anchor,
        };
        for (w, h) in [(70, 3), (150, 7), (92, 8), (70, 3)] {
            assert!(pager.resize_for_display(&doc, w, h, LayoutMode::Spread, 4));
            assert!(!pager.resize_for_display(&doc, w, h, LayoutMode::Spread, 4));
            assert_eq!(pager.anchor(), target);
            if policy == PaginationMode::KeepWord {
                assert_eq!(pager.visible_position().anchor, target);
            }
        }
    }
}

#[test]
fn keep_word_backward_from_a_picture_at_a_regular_boundary_turns_the_page() {
    let mut doc = short_document(true);
    doc.references.push(Reference {
        source: 2,
        bytes: None,
        backlink: false,
        label: "Plate".into(),
        target: Target::Image("plate.png".into()),
    });
    let mut normal = Pager::new(&doc, 80, 6, LayoutMode::Single);
    let previous = normal.complete_view().bounds;
    normal.next();
    let picture = normal.position();
    assert!(picture.illustration.is_some());
    let mut pager = Pager::for_display_position_with_pagination(
        &doc,
        (80, 6),
        LayoutMode::Single,
        4,
        normal.reflow_options(),
        Some(picture),
        (1, 2),
        PaginationMode::KeepWord,
    );
    assert_eq!(pager.complete_view().bounds, normal.complete_view().bounds);
    pager.previous();
    assert_eq!(pager.complete_view().bounds, previous);
}

#[test]
fn history_restores_both_exact_openings_and_canonical_search_results_in_keep_word() {
    let text = (0..240)
        .map(|n| format!("word{n:03}"))
        .collect::<Vec<_>>()
        .join(" ");
    let want = Anchor {
        block: 0,
        byte: text.find("word036").unwrap(),
    };
    let away = Anchor {
        block: 0,
        byte: text.find("word102").unwrap(),
    };
    let doc = Document {
        blocks: vec![Block {
            text,
            heading: false,
        }],
        ..Default::default()
    };
    for mode in [LayoutMode::Single, LayoutMode::Spread] {
        let mut p = Pager::for_display_position_with_pagination(
            &doc,
            (86, 4),
            mode,
            4,
            Pager::new(&doc, 86, 4, mode).reflow_options(),
            Some(crate::ViewPosition {
                anchor: want,
                illustration: None,
            }),
            (1, 2),
            PaginationMode::KeepWord,
        );
        for source in [want, away] {
            let entry = p.history_position();
            assert_eq!(entry.logical.anchor, source);
            let frame = (p.complete_view().bounds, p.lines(0)[0].text.clone());
            p.seek_chapter(want);
            p.restore_history(&doc, entry);
            assert_eq!(p.history_position(), entry);
            assert_eq!(
                (p.complete_view().bounds, p.lines(0)[0].text.clone()),
                frame
            );
            p.seek_chapter(away);
        }
    }
}

#[test]
fn page_turn_after_selection_drag_resumes_regular_pages_in_both_directions() {
    let doc = short_document(true);
    for mode in [LayoutMode::Single, LayoutMode::Spread] {
        let mut pager = Pager::new(&doc, 86, 4, mode);
        let regular = forward_breaks(&mut pager);
        pager.seek_chapter(Anchor::default());
        pager.scroll(1);
        assert_eq!(pager.first, 1, "selection can advance by a single row");
        pager.next();
        assert_eq!(pager.complete_view().bounds, regular[1]);
        pager.previous();
        assert_eq!(pager.complete_view().bounds, regular[0]);
    }
}
