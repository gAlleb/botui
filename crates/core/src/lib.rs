//! Text pagination shared by every source and terminal frontend.

mod location;
mod printed;
pub use location::{Locations, Run};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub contents: Vec<(Anchor, String)>,
    pub locations: Locations,
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub text: String,
    pub heading: bool,
}

/// A local position in the parsed text, independent of terminal dimensions.
/// `byte` is a UTF-8 byte offset in the indicated block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Anchor {
    pub block: usize,
    pub byte: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    Single,
    Spread,
}

#[derive(Debug)]
pub struct PageLine {
    pub text: String,
    pub heading: bool,
    start: Anchor,
}

const MIN_COLUMN: u16 = 32;
const SPREAD_GAP: u16 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    columns: usize,
    width: u16,
    height: usize,
    gutter: u16,
}

impl Layout {
    fn new(width: u16, height: u16, mode: LayoutMode) -> Self {
        let spread = mode == LayoutMode::Spread && width >= 2 * MIN_COLUMN + SPREAD_GAP;
        let (columns, width, gutter) = if spread {
            (2, (width - SPREAD_GAP) / 2, SPREAD_GAP)
        } else {
            (1, width, 0)
        };
        Self {
            columns,
            width,
            height: height.into(),
            gutter,
        }
    }
}

pub struct Pager {
    layout: Layout,
    lines: Vec<PageLine>,
    first: usize,
    focus: Anchor,
}

impl Pager {
    /// Width and height refer to the area available after the frontend's margins.
    pub fn new(doc: &Document, width: u16, height: u16, mode: LayoutMode) -> Self {
        let layout = Layout::new(width, height, mode);
        let lines = wrap_document(doc, layout.width);
        Self {
            layout,
            lines,
            first: 0,
            focus: Anchor::default(),
        }
    }

    /// Reflow the same document. Keep the line containing the saved word at the top.
    pub fn relayout(&mut self, doc: &Document, width: u16, height: u16, mode: LayoutMode) {
        let layout = Layout::new(width, height, mode);
        if self.layout == layout {
            return;
        }
        if self.layout.width != layout.width {
            self.lines = wrap_document(doc, layout.width);
        }
        self.layout = layout;
        self.seek(self.focus);
    }

    pub fn columns(&self) -> usize {
        self.layout.columns
    }
    pub fn column_width(&self) -> u16 {
        self.layout.width
    }
    pub fn gutter(&self) -> u16 {
        self.layout.gutter
    }
    pub fn anchor(&self) -> Anchor {
        self.focus
    }

    pub fn seek(&mut self, anchor: Anchor) {
        self.focus = anchor;
        // Anchors are in document order, including the paragraph separators.
        self.first = self
            .lines
            .partition_point(|line| line.start <= anchor)
            .saturating_sub(1);
    }

    pub fn lines(&self, column: usize) -> &[PageLine] {
        if column >= self.layout.columns {
            return &[];
        }
        let start = (self.first + column * self.layout.height).min(self.lines.len());
        let end = (start + self.layout.height).min(self.lines.len());
        &self.lines[start..end]
    }

    pub fn page_count(&self) -> usize {
        self.lines.len().div_ceil(self.layout.height.max(1)).max(1)
    }

    pub fn page_number(&self) -> usize {
        1 + self.first / self.layout.height.max(1)
    }

    pub fn next(&mut self) {
        let first = self.first + self.layout.columns * self.layout.height;
        if first != self.first && first < self.lines.len() {
            self.move_to(first);
        }
    }

    pub fn previous(&mut self) {
        let first = self
            .first
            .saturating_sub(self.layout.columns * self.layout.height);
        if first != self.first {
            self.move_to(first);
        }
    }

    fn move_to(&mut self, first: usize) {
        self.first = first;
        self.focus = self.lines[first].start;
    }
}

fn wrap_document(doc: &Document, width: u16) -> Vec<PageLine> {
    let mut lines = Vec::new();
    if width == 0 {
        return lines;
    }
    let printed = printed::printed_lines(&doc.blocks);
    let mut start = 0;
    while start < doc.blocks.len() {
        let mut end = start;
        while end + 1 < printed.len()
            && printed[end]
            && printed[end + 1]
            && printed::continues(&doc.blocks[end], &doc.blocks[end + 1])
        {
            end += 1;
        }
        let block = &doc.blocks[end];
        let before = lines.len();
        printed::wrap(
            &mut lines,
            &doc.blocks[start..=end],
            start,
            usize::from(width),
        );
        start = end + 1;
        if lines.len() > before {
            lines.push(PageLine {
                text: String::new(),
                heading: false,
                start: Anchor {
                    block: end,
                    byte: block.text.len(),
                },
            });
        }
    }
    lines.pop(); // Omit the trailing paragraph separator.
    lines
}

fn breaks_word(c: char) -> bool {
    c.is_control() || (c.is_whitespace() && !matches!(c, '\u{00a0}' | '\u{202f}' | '\u{2007}'))
}

fn wrap_block(lines: &mut Vec<PageLine>, block: &Block, index: usize, width: usize) {
    let mut rest = block.text.as_str();
    let mut row = String::new();
    let mut row_start = 0;
    let mut used = 0;
    let emit = |lines: &mut Vec<PageLine>, row: &mut String, byte| {
        lines.push(PageLine {
            text: std::mem::take(row),
            heading: block.heading,
            start: Anchor { block: index, byte },
        });
    };

    loop {
        rest = rest.trim_start_matches(breaks_word);
        if rest.is_empty() {
            break;
        }
        let byte = block.text.len() - rest.len();
        let end = rest.find(breaks_word).unwrap_or(rest.len());
        let (word, remaining) = rest.split_at(end);
        rest = remaining;
        let cells = word.width();
        if !row.is_empty() && used + 1 + cells > width {
            emit(lines, &mut row, row_start);
            used = 0;
        }
        if row.is_empty() {
            row_start = byte;
        } else {
            row.push(' ');
            used += 1;
        }
        if cells <= width {
            row.push_str(word);
            used += cells;
            continue;
        }
        // Long words and scripts without spaces break only between graphemes.
        for (offset, grapheme) in word.grapheme_indices(true) {
            let glyph = if grapheme.width() > width {
                "�"
            } else {
                grapheme
            };
            let cells = glyph.width();
            if !row.is_empty() && used + cells > width {
                emit(lines, &mut row, row_start);
                row_start = byte + offset;
                used = 0;
            }
            row.push_str(glyph);
            used += cells;
        }
    }
    if !row.is_empty() {
        emit(lines, &mut row, row_start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(paragraphs: &[&str]) -> Document {
        Document {
            title: "Книга".into(),
            contents: vec![],
            locations: Locations::default(),
            blocks: paragraphs
                .iter()
                .map(|text| Block {
                    text: (*text).into(),
                    heading: false,
                })
                .collect(),
        }
    }

    #[test]
    fn spread_fills_entire_left_page_before_the_right_and_turns_both() {
        let doc = document(&["А", "Б", "В", "Г", "Д"]);
        let mut pager = Pager::new(&doc, 80, 2, LayoutMode::Spread);
        assert_eq!(pager.columns(), 2);
        assert_eq!(pager.lines(0)[0].text, "А");
        assert_eq!(pager.lines(1)[0].text, "Б");
        pager.next();
        assert_eq!(pager.lines(0)[0].text, "В");
        assert_eq!(pager.lines(1)[0].text, "Г");
        pager.next();
        assert_eq!(pager.lines(0)[0].text, "Д");
        assert!(pager.lines(1).is_empty());
        pager.next();
        assert_eq!((pager.page_number(), pager.page_count()), (5, 5));
        pager.previous();
        assert_eq!(pager.lines(0)[0].text, "В");
    }

    #[test]
    fn resize_and_layout_changes_keep_the_exact_word_visible_on_the_first_line() {
        let doc = document(&[
            "Первая глава",
            "Проехали через деревню, переправились через речку и увидели зелёный лес.",
        ]);
        let anchor = Anchor {
            block: 1,
            byte: doc.blocks[1].text.find("речку").unwrap(),
        };
        let mut pager = Pager::new(&doc, 100, 4, LayoutMode::Single);
        pager.seek(anchor);
        for (width, height, mode, columns) in [
            (90, 4, LayoutMode::Spread, 2),
            (18, 20, LayoutMode::Spread, 1),
            (1, 1, LayoutMode::Single, 1),
            (0, 0, LayoutMode::Single, 1),
            (100, 4, LayoutMode::Spread, 2),
        ] {
            pager.relayout(&doc, width, height, mode);
            assert_eq!(pager.anchor(), anchor);
            assert_eq!(pager.columns(), columns);
            if width > 1 {
                assert!(pager.lines(0)[0].text.contains("речку"));
            }
        }
    }

    #[test]
    fn long_words_wrap_by_terminal_cells_without_splitting_graphemes() {
        let text = "а\u{301}б👩‍💻в漢字я";
        let doc = document(&[text]);
        let pager = Pager::new(&doc, 3, 10, LayoutMode::Single);
        let lines = pager.lines(0);
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<String>(),
            text
        );
        assert!(lines.iter().all(|line| line.text.width() <= 3));
        assert!(lines.iter().any(|line| line.text.contains("а\u{301}")));
        assert!(lines.iter().any(|line| line.text.contains("👩‍💻")));
        for line in lines {
            assert!(text[line.start.byte..].starts_with(&line.text));
        }
    }
}
