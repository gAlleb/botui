use crate::{Anchor, Block, Document, LayoutMode, Pager, Reference, Target};

// Record each column once by reading the book from its beginning.
fn forward_breaks(pager: &mut Pager) -> Vec<usize> {
    let mut breaks = vec![0];
    loop {
        let view = pager.complete_view();
        for &end in &view.bounds[1..=view.columns()] {
            if end > *breaks.last().unwrap() {
                breaks.push(end);
            }
        }
        if pager.at_end() {
            return breaks;
        }
        pager.next();
    }
}

fn check_direction_changes(pager: &mut Pager, breaks: &[usize]) {
    let columns = pager.columns();
    let end = breaks.len() - 1;
    let mut index = breaks.binary_search(&pager.first).unwrap();
    for direction in [-1, -1, 1, 1, 1, -1, -1, 1] {
        if direction < 0 {
            pager.previous();
            index = index.saturating_sub(columns);
        } else {
            pager.next();
            if index + columns < end {
                index += columns;
            }
        }
        let view = pager.complete_view();
        for (column, &edge) in view.bounds[..=columns].iter().enumerate() {
            assert_eq!(edge, breaks[(index + column).min(end)]);
        }
    }
}

#[test]
fn reversing_after_an_exact_jump_rejoins_the_forward_pages_without_skipping_rows() {
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
            let mut rejoined = false;
            for block in 23..doc.blocks.len() {
                pager.seek(Anchor { block, byte: 0 });
                while pager.first > breaks[pager.columns()] {
                    let original = pager.first;
                    pager.previous();
                    assert!(pager.first < original);
                    let end = pager.complete_view().end();
                    assert!(end >= original, "lost rows {end}..{original}");
                    if end > original {
                        // The regular page includes the row before the jump, as well
                        // as part of the exact view. All subsequent turns agree
                        // with the breaks recorded during uninterrupted reading.
                        check_direction_changes(&mut pager, &breaks);
                        rejoined = true;
                        break;
                    }
                    let previous = pager.first;
                    pager.next();
                    assert_eq!(pager.first, original);
                    pager.previous();
                    assert_eq!(pager.first, previous);
                }
            }
            assert!(
                rejoined,
                "no interior transition exercised at {mode:?}/{height}"
            );
        }
    }
}
