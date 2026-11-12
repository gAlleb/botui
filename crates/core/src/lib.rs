//! Text pagination shared by every source and terminal frontend.

mod location;
mod markdown;
mod media;
pub use media::TableRow;
mod pagination;
mod printed;
pub mod publisher;
mod reference;
mod reflow_cache;
use pagination::BookLayout;
pub use pagination::Viewport;
pub use reference::{Reference, Target};
use std::sync::Arc;
mod search;
mod selection;
pub use location::{Locations, Run};
pub use search::{SearchIndex, SearchMatch};
pub use selection::{TextCell, TextRange};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub markdown: Option<Markdown>,
    /// FB2 verse and explicit breaks before a block keep their own spacing.
    pub fixed_breaks: std::collections::BTreeSet<usize>,
    /// Lightweight inline faces in otherwise plain text (e.g. FB2), in source bytes.
    pub inline: std::collections::BTreeMap<usize, Vec<publisher::TextSpan>>,
    /// Ordered archive members when the entire reading order consists of raster plates.
    pub image_pages: Vec<String>,
    pub styled: std::collections::BTreeMap<usize, publisher::Chapter>,
    pub tables: std::collections::BTreeMap<usize, TableRow>,
    pub references: Vec<Reference>,
    pub contents: Vec<ContentsEntry>,
    pub locations: Locations,
    pub title: String,
    pub blocks: Vec<Block>,
}

/// Both views share the UTF-8 offsets of the original Markdown file.
#[derive(Clone, Debug)]
pub struct Markdown {
    pub rendered: publisher::Chapter,
    pub source: publisher::Chapter,
    pub show_source: bool,
    pub passages: Vec<publisher::SourceText>,
    pub paragraphs: Vec<std::ops::Range<usize>>,
    pub text: String,
    pub mapping: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
}
impl Document {
    pub fn toggle_markdown(&mut self) -> bool {
        let Some(md) = &mut self.markdown else {
            return false;
        };
        md.show_source = !md.show_source;
        self.styled.insert(
            0,
            if md.show_source {
                md.source.clone()
            } else {
                md.rendered.clone()
            },
        );
        true
    }
}

/// Preorder entry in the publisher's outline. Parent indexes refer to earlier entries.
#[derive(Clone, Debug)]
pub struct ContentsEntry {
    pub title: String,
    pub target: Target,
    pub parent: Option<usize>,
}
impl ContentsEntry {
    pub fn passage(anchor: Anchor, title: String) -> Self {
        Self {
            title,
            target: Target::Passage(anchor),
            parent: None,
        }
    }
    pub fn anchor(&self) -> Option<Anchor> {
        match self.target {
            Target::Passage(anchor) => Some(anchor),
            _ => None,
        }
    }
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
    Scroll,
    Single,
    Spread,
}

#[derive(Clone, Copy, Debug)]
pub struct Figure {
    pub reference: usize,
    pub x: u16,
    pub width: u16,
    /// This row's offset in the complete illustration box.
    pub row: u16,
    pub height: u16,
}

#[derive(Clone, Debug, Default)]
pub struct PageLine {
    /// Leading cells on the first visual line of a prose paragraph.
    pub indent: u16,
    pub figures: Vec<Figure>,
    pub table_header: bool,
    pub text: String,
    pub heading: bool,
    start: Anchor,
    pub spans: Vec<publisher::TextSpan>,
}

impl PageLine {
    pub fn anchor(&self) -> Anchor {
        self.start
    }
    pub(crate) fn source_range(
        &self,
        block: usize,
        bytes: &std::ops::Range<usize>,
    ) -> Option<std::ops::Range<usize>> {
        let mut visible: Option<std::ops::Range<usize>> = None;
        for span in &self.spans {
            let Some(at) = span.source else { continue };
            if at.block != block || at.byte >= bytes.end || span.source_end <= bytes.start {
                continue;
            }
            let range = if span.bytes.len() == span.source_end - at.byte {
                span.bytes.start + bytes.start.saturating_sub(at.byte)
                    ..span.bytes.end - span.source_end.saturating_sub(bytes.end)
            } else {
                span.bytes.clone()
            };
            match &mut visible {
                Some(r) if r.end == range.start => r.end = range.end,
                Some(_) => break,
                None => visible = Some(range),
            }
        }
        visible
    }
    pub fn link_range(&self, doc: &Document, link: &Reference) -> Option<std::ops::Range<usize>> {
        if !self.spans.is_empty() {
            return self.source_range(link.source, link.bytes.as_ref()?);
        }
        if link.source != self.start.block || self.text.is_empty() {
            return None;
        }
        let bytes = link.bytes.as_ref()?;
        let from = self.start.byte;
        let end = from + self.text.len();
        if doc.blocks.get(self.start.block)?.text.get(from..end)? != self.text {
            return None;
        }
        let a = bytes.start.max(from);
        let b = bytes.end.min(end);
        (a < b).then(|| a - from..b - from)
    }
}
const MIN_COLUMN: u16 = 32;
const SPREAD_GAP: u16 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    columns: usize,
    width: u16,
    height: usize,
    gutter: u16,
    scrolling: bool,
}

impl Layout {
    fn new(width: u16, height: u16, mode: LayoutMode, gap: u16) -> Self {
        let spread = mode == LayoutMode::Spread && width.saturating_sub(gap) >= 2 * MIN_COLUMN;
        let (columns, width, gutter) = if spread {
            (2, (width - gap) / 2, gap)
        } else {
            (1, width, 0)
        };
        Self {
            columns,
            width,
            height: height.into(),
            gutter,
            scrolling: mode == LayoutMode::Scroll,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ReflowOptions {
    pub repair_printed: bool,
    pub inline_images: bool,
    pub paragraph_indent: u8,
    /// Blank terminal rows between prose paragraphs. None follows the book.
    pub paragraph_spacing: Option<u8>,
}

pub struct Pager {
    book: Arc<BookLayout>,
    reflow_cache: reflow_cache::Cache,
    pending: Option<pagination::Pending>,
    layout: Layout,
    lines: Vec<PageLine>,
    source_lines: Vec<(Anchor, usize)>,
    page_starts: Vec<usize>,
    first: usize,
    focus: Anchor,
}

impl Pager {
    /// Width and height refer to the area available after the frontend's margins.
    pub fn new(doc: &Document, width: u16, height: u16, mode: LayoutMode) -> Self {
        Self::with_reflow(doc, width, height, mode, true)
    }
    pub fn with_reflow(
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        repair_printed: bool,
    ) -> Self {
        Self::with_media(doc, width, height, mode, repair_printed, true)
    }
    pub fn with_media(
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        repair_printed: bool,
        inline_images: bool,
    ) -> Self {
        Self::with_options(
            doc,
            width,
            height,
            mode,
            ReflowOptions {
                repair_printed,
                inline_images,
                paragraph_indent: 0,
                paragraph_spacing: None,
            },
        )
    }
    pub fn with_options(
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        options: ReflowOptions,
    ) -> Self {
        Self::with_start(doc, width, height, mode, options, None)
    }

    /// Reflow the opening passage from the saved word. Earlier text remains
    /// reachable by paging back, including the prefix of a split paragraph.
    pub fn with_start(
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        options: ReflowOptions,
        resume: Option<Anchor>,
    ) -> Self {
        Self::with_start_and_gap(doc, width, height, mode, SPREAD_GAP, options, resume)
    }

    pub fn with_start_and_gap(
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        gap: u16,
        options: ReflowOptions,
        resume: Option<Anchor>,
    ) -> Self {
        let layout = Layout::new(width, height, mode, gap);
        let book = Arc::new(BookLayout::new(doc, options, resume));
        let lines = book.lines(doc, layout, 0..doc.blocks.len());
        let mut pager = Self {
            book,
            reflow_cache: reflow_cache::Cache::default(),
            pending: None,
            layout,
            source_lines: Self::source_index(doc, &lines),
            lines,
            page_starts: Vec::new(),
            first: 0,
            focus: Anchor::default(),
        };
        if let Some(at) = resume {
            pager.seek(at);
        } else {
            pager.index_pages();
        }
        pager
    }

    /// Reflow the same document. Keep the line containing the saved word at the top.
    pub fn relayout(&mut self, doc: &Document, width: u16, height: u16, mode: LayoutMode) {
        self.relayout_with_gap(doc, width, height, mode, SPREAD_GAP)
    }
    /// Changing typography keeps the current passage. Only the original
    /// resume address can introduce a break in the middle of a paragraph.
    pub fn reconfigure(
        &mut self,
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        gap: u16,
        options: ReflowOptions,
    ) {
        let focus = self.focus;
        *self = Self::with_start_and_gap(doc, width, height, mode, gap, options, self.book.resume);
        self.seek(focus);
    }

    pub fn relayout_with_gap(
        &mut self,
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        gap: u16,
    ) {
        let layout = Layout::new(width, height, mode, gap);
        self.pending = None;
        if !self.use_cached_layout(layout) {
            let lines = self.book.lines(doc, layout, 0..doc.blocks.len());
            self.install(
                reflow_cache::Snapshot {
                    size: self.book.size(layout),
                    sources: Self::source_index(doc, &lines),
                    lines,
                },
                layout,
            );
        }
    }

    fn use_cached_layout(&mut self, layout: Layout) -> bool {
        if self.layout == layout {
            return true;
        }
        let size = self.book.size(layout);
        if size == self.book.size(self.layout) {
            self.layout = layout;
            self.seek(self.focus);
            return true;
        }
        let Some(snapshot) = self.reflow_cache.take(size) else {
            return false;
        };
        self.install(snapshot, layout);
        true
    }

    fn install(&mut self, snapshot: reflow_cache::Snapshot, layout: Layout) {
        let previous = reflow_cache::Snapshot {
            size: self.book.size(self.layout),
            lines: std::mem::replace(&mut self.lines, snapshot.lines),
            sources: std::mem::replace(&mut self.source_lines, snapshot.sources),
        };
        self.reflow_cache.put(previous);
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

    fn source_index(doc: &Document, lines: &[PageLine]) -> Vec<(Anchor, usize)> {
        if doc.styled.is_empty() {
            return Vec::new();
        }
        let mut index = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if line.spans.is_empty() && (!line.text.trim().is_empty() || !line.figures.is_empty()) {
                index.push((line.start, i));
            }
            index.extend(line.spans.iter().filter_map(|s| s.source.map(|a| (a, i))));
        }
        index.sort_unstable();
        index.dedup_by_key(|v| v.0);
        index
    }

    pub fn seek(&mut self, anchor: Anchor) {
        self.finish_resize();
        self.focus = anchor;
        self.first = Self::line_at(anchor, &self.lines, &self.source_lines);
        self.index_pages();
    }

    fn line_at(anchor: Anchor, lines: &[PageLine], sources: &[(Anchor, usize)]) -> usize {
        if !sources.is_empty() {
            let i = sources.partition_point(|(at, _)| *at <= anchor);
            return sources[i.saturating_sub(1)].1;
        }
        let first = lines.partition_point(|line| line.start < anchor);
        if lines.get(first).is_some_and(|line| line.start == anchor) {
            first
        } else {
            first.saturating_sub(1)
        }
    }

    pub fn lines(&self, column: usize) -> &[PageLine] {
        self.complete_view().lines(column)
    }

    pub fn page_count(&self) -> usize {
        if self.page_starts.is_empty() {
            self.lines.len().div_ceil(self.layout.height.max(1)).max(1)
        } else {
            self.page_starts.len()
        }
    }

    pub fn page_number(&self) -> usize {
        if self.page_starts.is_empty() {
            1 + self.first / self.layout.height.max(1)
        } else {
            self.page_starts
                .partition_point(|&start| start <= self.first)
        }
    }

    pub fn next(&mut self) {
        self.finish_resize();
        let first = self.complete_view().end();
        if first != self.first && first < self.lines.len() {
            self.move_to(first);
        }
    }

    pub fn previous(&mut self) {
        self.finish_resize();
        let first = if self.page_starts.is_empty() {
            self.first
                .saturating_sub(self.layout.columns * self.layout.height)
        } else {
            self.page_starts[(self.page_number() - 1).saturating_sub(self.layout.columns)]
        };
        if first != self.first {
            self.move_to(first);
        }
    }

    pub fn scroll(&mut self, lines: isize) {
        self.finish_resize();
        let max = self.lines.len().saturating_sub(self.layout.height.max(1));
        let first = self.first.saturating_add_signed(lines).min(max);
        if !self.lines.is_empty() && first != self.first {
            self.move_to(first);
            if !self.page_starts.is_empty() {
                self.index_pages();
            }
        }
    }
    pub fn viewport(&self) -> usize {
        self.layout.height.max(1)
    }
    pub fn at_end(&self) -> bool {
        self.complete_view().end() >= self.lines.len()
    }
    fn move_to(&mut self, first: usize) {
        self.first = first;
        // The blank row between paragraphs belongs to the layout. Its source
        // address is the end of the preceding paragraph, often on another page.
        let line = &self.lines[first];
        self.focus = self.lines[first..]
            .iter()
            .find(|row| {
                !row.figures.is_empty()
                    || if row.spans.is_empty() {
                        !row.text.trim().is_empty()
                    } else {
                        row.spans.iter().any(|s| s.source.is_some())
                    }
            })
            .unwrap_or(line)
            .start;
    }
}

fn breaks_word(c: char) -> bool {
    c.is_control() || (c.is_whitespace() && !matches!(c, '\u{00a0}' | '\u{202f}' | '\u{2007}'))
}

fn wrap_block(lines: &mut Vec<PageLine>, block: &Block, index: usize, width: usize, indent: u8) {
    let mut rest = block.text.as_str();
    let mut row = String::new();
    let mut row_start = 0;
    // Leave at least two cells for a wide grapheme in a narrow viewport.
    let mut indent = usize::from(indent.min(8)).min(width.saturating_sub(2));
    let mut used = indent;
    let emit = |lines: &mut Vec<PageLine>, row: &mut String, byte, indent: &mut usize| {
        lines.push(PageLine {
            spans: Vec::new(),
            indent: std::mem::take(indent) as u16,
            figures: Vec::new(),
            table_header: false,
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
            emit(lines, &mut row, row_start, &mut indent);
            used = 0;
        }
        if row.is_empty() {
            row_start = byte;
        } else {
            row.push(' ');
            used += 1;
        }
        if used + cells <= width {
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
                emit(lines, &mut row, row_start, &mut indent);
                row_start = byte + offset;
                used = 0;
            }
            row.push_str(glyph);
            used += cells;
        }
    }
    if !row.is_empty() {
        emit(lines, &mut row, row_start, &mut indent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(paragraphs: &[&str]) -> Document {
        Document {
            markdown: None,
            fixed_breaks: Default::default(),
            inline: Default::default(),
            image_pages: vec![],
            styled: Default::default(),
            tables: Default::default(),
            title: "Книга".into(),
            references: vec![],
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
    fn first_line_indent_reserves_cells_without_changing_source_addresses() {
        let mut doc = document(&["A heading", "зима море берег ветер", "новый абзац"]);
        doc.blocks[0].heading = true;
        let mut pager = Pager::with_options(
            &doc,
            10,
            14,
            LayoutMode::Single,
            ReflowOptions {
                repair_printed: false,
                inline_images: true,
                paragraph_indent: 4,
                paragraph_spacing: None,
            },
        );
        let lines = pager.lines(0);
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.indent > 0)
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>(),
            ["зима", "новый"]
        );
        let anchor = Anchor {
            block: 1,
            byte: "зима ".len(),
        };
        pager.seek(anchor);
        assert!(pager.lines(0)[0].text.starts_with("море"));
        for (width, mode) in [(1, LayoutMode::Scroll), (90, LayoutMode::Spread)] {
            pager.relayout(&doc, width, 14, mode);
            assert_eq!(pager.anchor(), anchor);
            assert!(
                pager
                    .lines(0)
                    .iter()
                    .all(|line| line.text.width() + usize::from(line.indent)
                        <= usize::from(pager.column_width()))
            );
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
    fn page_turns_keep_a_figure_together_and_scrolling_exposes_its_source_rows() {
        let mut doc = document(&["One", "Two", "Caption", "Four", "Five"]);
        doc.references.push(Reference {
            source: 2,
            bytes: None,
            backlink: false,
            label: "Picture".into(),
            target: Target::Image("picture.png".into()),
        });
        let strip = |rows: &[PageLine]| {
            rows.iter()
                .flat_map(|line| &line.figures)
                .map(|f| (f.row, f.height))
                .collect::<Vec<_>>()
        };
        let full = [(0, 4), (1, 4), (2, 4), (3, 4)];
        let mut pager = Pager::new(&doc, 80, 6, LayoutMode::Spread);
        assert_eq!(pager.lines(0).len(), 4);
        assert!(strip(pager.lines(0)).is_empty());
        assert_eq!(strip(pager.lines(1)), full);
        assert_eq!(pager.lines(1)[4].text, "Caption");
        assert_eq!(pager.page_count(), 3);
        pager.next();
        assert_eq!(pager.page_number(), 3);
        assert_eq!(pager.lines(0)[0].text, "Four");
        assert!(pager.at_end());
        pager.previous();
        assert_eq!(pager.page_number(), 1);
        assert_eq!(strip(pager.lines(1)), full);

        pager.relayout(&doc, 80, 6, LayoutMode::Single);
        pager.next();
        assert_eq!(strip(pager.lines(0)), full);
        pager.previous();
        assert_eq!(pager.lines(0)[0].text, "One");
        pager.relayout(&doc, 80, 6, LayoutMode::Scroll);
        assert_eq!(strip(pager.lines(0)), &full[..2]);
        pager.scroll(5);
        assert_eq!(strip(pager.lines(0)), &full[1..]);
        pager.scroll(-1);
        assert_eq!(strip(pager.lines(0)), full);
    }

    #[test]
    fn page_turn_on_paragraph_separator_saves_first_visible_passage() {
        let doc = document(&["one two three", "next paragraph", "ending"]);
        let mut pager = Pager::new(&doc, 5, 3, LayoutMode::Single);
        pager.next();
        assert!(pager.lines(0)[0].text.is_empty());
        assert_eq!(pager.lines(0)[1].text, "next");
        assert_eq!(pager.anchor(), Anchor { block: 1, byte: 0 });
        pager.previous();
        assert_eq!(pager.anchor(), Anchor::default());
    }

    #[test]
    fn web_resume_starts_at_saved_word_and_keeps_preceding_text_reachable() {
        let doc = document(&["друг другу. Частично это мне удалось, и когда мне показалось"]);
        let at = Anchor {
            block: 0,
            byte: doc.blocks[0].text.find("удалось").unwrap(),
        };
        let mut pager = Pager::with_start(
            &doc,
            44,
            2,
            LayoutMode::Single,
            ReflowOptions {
                repair_printed: true,
                inline_images: false,
                paragraph_indent: 3,
                paragraph_spacing: None,
            },
            Some(at),
        );
        for width in [44, 24, 100] {
            pager.relayout(&doc, width, 2, LayoutMode::Spread);
            assert_eq!(pager.lines(0)[0].anchor(), at);
            assert!(pager.lines(0)[0].text.starts_with("удалось,"));
            assert_eq!(pager.lines(0)[0].indent, 0);
        }
        pager.previous();
        assert_eq!(pager.lines(0)[0].text, "друг другу. Частично это мне");
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
    fn cached_chapters_keep_source_order_and_do_not_leak_between_books() {
        use publisher::{Chapter, SourceText, Stylesheets};
        use std::sync::Arc;
        let book = |name: &str| {
            let mut doc = Document::default();
            let sheets = Arc::new(Stylesheets::default());
            for block in 0..12 {
                let text = format!("{name} chapter {block}");
                let html = format!(
                    "<!doctype html><html><body><p data-botui-text='0'>{text}</p></body></html>"
                );
                let sources = vec![vec![SourceText {
                    display: None,
                    start: Anchor { block, byte: 0 },
                    end: text.len(),
                    child: 0,
                }]];
                doc.blocks.push(Block {
                    text,
                    heading: false,
                });
                doc.styled.insert(
                    block,
                    Chapter::new(
                        block + 1,
                        html,
                        &format!("chapter{block}"),
                        sheets.clone(),
                        sources,
                    ),
                );
            }
            doc
        };
        let first = book("First");
        let second = book("Second");
        let mut a = Pager::new(&first, 60, 100, LayoutMode::Single);
        let mut b = Pager::new(&second, 60, 100, LayoutMode::Single);
        for width in [48, 62, 60, 48, 70] {
            for (pager, doc) in [(&mut a, &first), (&mut b, &second)] {
                pager.relayout(doc, width, 100, LayoutMode::Single);
                let rows: Vec<_> = pager
                    .lines(0)
                    .iter()
                    .filter(|l| !l.text.is_empty())
                    .collect();
                assert_eq!(rows.len(), doc.blocks.len());
                for (i, line) in rows.iter().enumerate() {
                    assert_eq!(line.text, doc.blocks[i].text);
                    assert_eq!(line.anchor(), Anchor { block: i, byte: 0 });
                }
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
