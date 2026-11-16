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
fn chapter_remainder_matches_actual_page_turns_after_jumps_and_reflow() {
    let doc = Document {
        blocks: (0..72)
            .map(|i| Block {
                heading: matches!(i, 5 | 23 | 50),
                text: format!("Passage {i} in the book."),
            })
            .collect(),
        chapter_starts: [5, 23, 50].into(),
        ..Default::default()
    };
    let mut pager = Pager::new(&doc, 90, 7, LayoutMode::Spread);
    for mode in [LayoutMode::Single, LayoutMode::Spread, LayoutMode::Scroll] {
        for height in [6, 13] {
            pager.relayout(&doc, 90, height, mode);
            pager.seek(Anchor::default());
            assert_eq!(pager.screens_left_in_chapter(), None);
            for block in [5, 9, 21, 23, 49, 50, 67] {
                pager.seek(Anchor { block, byte: 0 });
                let expected = pager.screens_left_in_chapter().unwrap();
                let end = (pager.first + 1..pager.lines.len())
                    .find(|&row| pager.lines[row].chapter_start)
                    .unwrap_or(pager.lines.len());
                let mut turns = 0;
                while pager.complete_view().end() < end {
                    let before = pager.first;
                    pager.next();
                    assert!(pager.first > before);
                    turns += 1;
                }
                assert_eq!(turns, expected, "{mode:?} at {block}, height {height}");
            }
        }
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
