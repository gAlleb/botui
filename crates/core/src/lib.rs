//! Text pagination shared by every source and terminal frontend.

mod location;
mod markdown;
mod media;
pub use media::TableRow;
mod pagination;
mod printed;
mod prose;
pub mod publisher;
mod reference;
mod reflow_cache;
use pagination::BookLayout;
pub use pagination::{ChapterProgress, Viewport};
pub use reference::{Reference, Target};
use std::sync::Arc;
mod search;
mod spine;
pub use spine::{EpubSpine, SpinePosition};
mod selection;
pub use location::{Locations, RangeCfiError, Run};
pub use search::{SearchIndex, SearchMatch, SearchMode};
pub use selection::{TextCell, TextRange};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub epub_spine: EpubSpine,
    pub markdown: Option<Markdown>,
    /// FB2 verse and explicit breaks before a block keep their own spacing.
    pub fixed_breaks: std::collections::BTreeSet<usize>,
    /// EPUB heading rank keyed by the final text block of each heading.
    pub heading_levels: std::collections::BTreeMap<usize, u8>,
    /// Source blocks that open chapters or parts of the book.
    pub chapter_starts: std::collections::BTreeSet<usize>,
    /// Lightweight inline faces in otherwise plain text (e.g. FB2), in source bytes.
    pub inline: std::collections::BTreeMap<usize, Vec<publisher::TextSpan>>,
    /// Ordered archive members when the entire reading order consists of raster plates.
    pub image_pages: Vec<String>,
    /// Intrinsic dimensions from embedded image headers, keyed by archive member.
    pub image_sizes: std::collections::HashMap<String, (usize, usize)>,
    pub styled: std::collections::BTreeMap<usize, publisher::Chapter>,
    pub tables: std::collections::BTreeMap<usize, TableRow>,
    pub references: Vec<Reference>,
    /// Named destinations in the reading order, including links from lazily opened EPUB notes.
    pub epub_anchors: std::collections::HashMap<(String, String), Anchor>,
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
    /// A TOC label such as "3. Anatomy" can span two printed source blocks.
    pub fn matches_numbered_title(&self, number: &str, heading: &str) -> bool {
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let Some(rest) = self.title.trim().strip_prefix(number) else {
            return false;
        };
        let title = rest.trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, '.' | ')' | ':' | '-' | '–' | '—')
        });
        title.len() < rest.len() && title.to_lowercase() == heading.trim().to_lowercase()
    }

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

/// A local reading position. An illustration can share its text anchor with
/// the preceding line or with another illustration in the same document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewPosition {
    pub anchor: Anchor,
    pub illustration: Option<IllustrationPosition>,
}

/// Session-only jump history remembers the view as well as the address used
/// for progress sync. Only the logical position belongs in persistent storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryPosition {
    pub logical: ViewPosition,
    visible: ViewPosition,
    chapter_page: bool,
    split_word: Option<Anchor>,
    leading_rows: usize,
}

/// An occurrence in this document, including the first visible row relative
/// to its top. A negative row includes a leading layout spacer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IllustrationPosition {
    pub reference: usize,
    pub row: i32,
}

impl IllustrationPosition {
    fn at(rows: &[PageLine]) -> Option<Self> {
        let blank = rows.iter().take_while(|row| row.is_spacer()).count();
        let line = rows.get(blank)?;
        if line.chapter_start || !media::bare(line) {
            return None;
        }
        let part = line.figures.first()?;
        Some(Self {
            reference: part.reference,
            row: i32::from(part.row) - i32::try_from(blank).ok()?,
        })
    }

    fn resolve(self, lines: &[PageLine], starts: &[(usize, Figure)]) -> Option<usize> {
        let &(top, figure) = starts.iter().find(|(_, f)| f.reference == self.reference)?;
        if self.row < 0 {
            let blank = lines[..top]
                .iter()
                .rev()
                .take(self.row.unsigned_abs() as usize)
                .take_while(|row| row.is_spacer())
                .count();
            return Some(top - blank);
        }
        // Responsive CSS can shrink the image or reflow text alongside it.
        // Stay inside this occurrence, even if the old offset is out of range.
        let limit = (self.row as usize).min(usize::from(figure.height.saturating_sub(1)));
        let offset = lines[top + 1..]
            .iter()
            .take(limit)
            .enumerate()
            .take_while(|(i, line)| {
                !line.chapter_start
                    && line.figures.iter().any(|part| {
                        part.reference == self.reference && usize::from(part.row) == i + 1
                    })
            })
            .count();
        Some(top + offset)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    Scroll,
    Single,
    Spread,
}

/// Page boundaries used by the reflowable reader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaginationMode {
    #[default]
    Chapter,
    KeepWord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Figure {
    pub reference: usize,
    pub x: u16,
    pub width: u16,
    /// This row's offset in the complete illustration box.
    pub row: u16,
    pub height: u16,
}

/// Visual elements drawn over a row without changing its text or source positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    HeadingRule { double: bool },
    Background { x: u16, width: u16, role: u8 },
    TableDivider { x: u16 },
    TableHeaderRule { x: u16, width: u16 },
    TableEdge { x: u16, width: u16, top: bool },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageLine {
    /// Leading cells on the first visual line of a prose paragraph.
    pub indent: u16,
    pub figures: Vec<Figure>,
    pub surfaces: Vec<Surface>,
    pub table_header: bool,
    /// Row within a display equation and its total rendered height.
    equation: (u16, u16),
    chapter_start: bool,
    /// A terminal-cell hyphen inserted by the prose wrapper.
    discretionary_hyphen: bool,
    pub text: String,
    pub heading: bool,
    start: Anchor,
    pub spans: Vec<publisher::TextSpan>,
}

impl PageLine {
    fn has_visible_surface(&self) -> bool {
        !self.surfaces.is_empty()
    }

    fn is_spacer(&self) -> bool {
        self.figures.is_empty() && !self.chapter_start && media::bare(self)
    }

    pub fn anchor(&self) -> Anchor {
        self.start
    }
    fn reading_anchor(&self) -> Option<Anchor> {
        let content = if self.spans.is_empty() {
            !self.text.trim().is_empty()
        } else {
            self.spans.iter().any(|s| s.source.is_some())
        };
        (content || !self.figures.is_empty()).then_some(self.start)
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
    image_cell: (u16, u16),
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
            image_cell: (1, 2),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignment {
    Book,
    #[default]
    Left,
    Center,
    Right,
    Justify,
}
impl TextAlignment {
    pub const ALL: [Self; 5] = [
        Self::Book,
        Self::Left,
        Self::Center,
        Self::Right,
        Self::Justify,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Book => "From book",
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
            Self::Justify => "Justified",
        }
    }
}
#[derive(Clone, Copy)]
pub struct Typography {
    pub alignment: TextAlignment,
    pub hyphenation: bool,
    pub heading_rules: bool,
}

// Low-level publisher layout follows the source until a reader profile is applied.
impl Default for Typography {
    fn default() -> Self {
        Self {
            alignment: TextAlignment::Book,
            hyphenation: false,
            heading_rules: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ReflowOptions {
    pub typography: Typography,
    pub repair_printed: bool,
    pub inline_images: bool,
    pub paragraph_indent: u8,
    /// Blank terminal rows between prose paragraphs. None follows the book.
    pub paragraph_spacing: Option<u8>,
    /// Preserve publisher spacing in addresses, aligned groups and explicit spacers.
    pub compact_spacing: bool,
}

struct NaturalRows {
    split_word: Anchor,
    replaced: std::ops::Range<usize>,
    rows: Vec<PageLine>,
    sources: Vec<(Anchor, usize)>,
}

pub struct Pager {
    image_cell: (u16, u16),
    pagination_mode: PaginationMode,
    /// An exact opening may rewrap a paragraph or start between regular pages.
    exact_view: bool,
    /// Keep the source address fixed throughout consecutive geometry changes.
    resize_origin: Option<ViewPosition>,
    focus_illustration: Option<IllustrationPosition>,
    book: Arc<BookLayout>,
    reflow_cache: reflow_cache::Cache,
    pending: Option<pagination::Pending>,
    recent_views: std::collections::VecDeque<(Layout, ViewPosition, Arc<[PageLine]>)>,
    natural: Option<NaturalRows>,
    layout: Layout,
    lines: Vec<PageLine>,
    source_lines: Vec<(Anchor, usize)>,
    /// Column starts, paginated forward from the start of the book.
    page_starts: Vec<usize>,
    /// Two-page views can end after the left page at a chapter boundary.
    spread_starts: Vec<usize>,
    figure_starts: Vec<(usize, Figure)>,
    first: usize,
    focus: Anchor,
}

impl Pager {
    pub fn reflow_options(&self) -> ReflowOptions {
        self.book.options
    }

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
                typography: Default::default(),
                repair_printed,
                inline_images,
                paragraph_indent: 0,
                compact_spacing: true,
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

    /// Open the chapter page containing the saved passage.
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
        Self::for_display(doc, (width, height), mode, gap, options, resume, (1, 2))
    }

    pub fn for_display(
        doc: &Document,
        size: (u16, u16),
        mode: LayoutMode,
        gap: u16,
        options: ReflowOptions,
        resume: Option<Anchor>,
        image_cell: (u16, u16),
    ) -> Self {
        Self::for_display_position(
            doc,
            size,
            mode,
            gap,
            options,
            resume.map(|anchor| ViewPosition {
                anchor,
                illustration: None,
            }),
            image_cell,
        )
    }

    pub fn for_display_position(
        doc: &Document,
        size: (u16, u16),
        mode: LayoutMode,
        gap: u16,
        options: ReflowOptions,
        resume: Option<ViewPosition>,
        image_cell: (u16, u16),
    ) -> Self {
        Self::for_display_position_with_pagination(
            doc,
            size,
            mode,
            gap,
            options,
            resume,
            image_cell,
            PaginationMode::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn for_display_position_with_pagination(
        doc: &Document,
        (width, height): (u16, u16),
        mode: LayoutMode,
        gap: u16,
        options: ReflowOptions,
        resume: Option<ViewPosition>,
        image_cell: (u16, u16),
        pagination_mode: PaginationMode,
    ) -> Self {
        let layout = Layout {
            image_cell,
            ..Layout::new(width, height, mode, gap)
        };
        let mut pager = Self::layout_document(doc, layout, options, pagination_mode);
        pager.index_pages();
        if let Some(position) = resume {
            pager.open_position(doc, position);
        } else {
            pager.move_to(0);
        }
        pager
    }

    fn layout_document(
        doc: &Document,
        layout: Layout,
        options: ReflowOptions,
        pagination_mode: PaginationMode,
    ) -> Self {
        let book = Arc::new(BookLayout::new(doc, options, None));
        let lines = book.lines(doc, layout, 0..doc.blocks.len());
        Self::from_lines(doc, layout, book, pagination_mode, lines)
    }

    fn from_lines(
        doc: &Document,
        layout: Layout,
        book: Arc<BookLayout>,
        pagination_mode: PaginationMode,
        lines: Vec<PageLine>,
    ) -> Self {
        Self {
            image_cell: layout.image_cell,
            book,
            reflow_cache: reflow_cache::Cache::default(),
            pending: None,
            recent_views: Default::default(),
            natural: None,
            pagination_mode,
            exact_view: false,
            resize_origin: None,
            focus_illustration: None,
            layout,
            source_lines: Self::source_index(!doc.styled.is_empty(), &lines),
            figure_starts: Self::index_figures(&lines),
            lines,
            page_starts: Vec::new(),
            spread_starts: Vec::new(),
            first: 0,
            focus: Anchor::default(),
        }
    }

    pub fn pagination_mode(&self) -> PaginationMode {
        self.pagination_mode
    }

    pub fn set_pagination_mode(&mut self, mode: PaginationMode) {
        if self.pagination_mode != mode {
            self.finish_resize();
            let position = self.position();
            self.pagination_mode = mode;
            self.recent_views.clear();
            self.restore_natural();
            self.resize_origin = None;
            self.locate(position, !self.layout.scrolling);
        }
    }

    pub fn relayout(&mut self, doc: &Document, width: u16, height: u16, mode: LayoutMode) {
        self.relayout_with_gap(doc, width, height, mode, SPREAD_GAP)
    }

    pub fn reconfigure(
        &mut self,
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        gap: u16,
        options: ReflowOptions,
    ) {
        let position = self.resize_position();
        *self = Self::for_display_position_with_pagination(
            doc,
            (width, height),
            mode,
            gap,
            options,
            Some(position),
            self.image_cell,
            self.pagination_mode,
        );
        self.resize_origin = Some(position);
    }

    pub fn set_image_cell(&mut self, width: u16, height: u16) {
        self.image_cell = (width.max(1), height.max(1));
    }

    fn resize_position(&self) -> ViewPosition {
        self.resize_origin
            .unwrap_or_else(|| match self.pagination_mode {
                PaginationMode::Chapter => self.position(),
                PaginationMode::KeepWord => self.visible_position(),
            })
    }

    pub fn relayout_with_gap(
        &mut self,
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        gap: u16,
    ) {
        let layout = Layout {
            image_cell: self.image_cell,
            ..Layout::new(width, height, mode, gap)
        };
        self.pending = None;
        if layout != self.layout {
            let position = self.resize_position();
            self.change_layout(doc, layout);
            self.open_position(doc, position);
            self.resize_origin = Some(position);
        }
    }

    fn change_layout(&mut self, doc: &Document, layout: Layout) {
        self.restore_natural();
        if !self.use_cached_layout(layout) {
            let lines = self.book.lines(doc, layout, 0..doc.blocks.len());
            self.install(
                reflow_cache::Snapshot {
                    size: self.book.size(layout),
                    sources: Self::source_index(!doc.styled.is_empty(), &lines),
                    lines,
                },
                layout,
            );
        }
    }

    /// Chapter pages share one reading position across layout switches. An
    /// exact-word switch always starts from what is currently visible.
    pub fn switch_mode(
        &mut self,
        doc: &Document,
        width: u16,
        height: u16,
        mode: LayoutMode,
        gap: u16,
    ) {
        self.finish_resize();
        let chapter = self.pagination_mode == PaginationMode::Chapter;
        let position = if chapter {
            self.resize_position()
        } else {
            self.visible_position()
        };
        let layout = Layout {
            image_cell: self.image_cell,
            ..Layout::new(width, height, mode, gap)
        };
        self.change_layout(doc, layout);
        self.open_position(doc, position);
        self.resize_origin = chapter.then_some(position);
    }

    fn open_position(&mut self, doc: &Document, position: ViewPosition) {
        let chapter = !self.layout.scrolling && self.pagination_mode == PaginationMode::Chapter;
        self.locate(position, chapter);
        if !chapter {
            self.exact_top(doc, position);
        }
    }

    fn exact_top(&mut self, doc: &Document, position: ViewPosition) {
        self.exact_top_in(doc, position, 0..doc.blocks.len());
    }

    fn exact_top_in(
        &mut self,
        doc: &Document,
        position: ViewPosition,
        blocks: std::ops::Range<usize>,
    ) {
        if self.layout.width == 0
            || position
                .illustration
                .is_some_and(|hint| hint.resolve(&self.lines, &self.figure_starts).is_some())
            || self
                .lines
                .get(self.first)
                .and_then(PageLine::reading_anchor)
                .is_none_or(|top| top == position.anchor)
        {
            return;
        }
        let anchor = position.anchor;
        let Some(block) = doc.blocks.get(anchor.block) else {
            return;
        };
        let prose_row = |l: &&PageLine| l.start.block == anchor.block && !l.text.trim().is_empty();
        let start = self.first
            - self.lines[..self.first]
                .iter()
                .rev()
                .take_while(prose_row)
                .count();
        let end = self.first
            + self.lines[self.first..]
                .iter()
                .take_while(prose_row)
                .count();
        let candidate = prose::layout(
            &self.lines[start..end],
            block,
            self.layout.width,
            self.book.options.typography,
            Some(anchor),
        );
        let (range, rows) = if let Some(rows) =
            candidate.filter(|rows| rows.iter().any(|row| row.reading_anchor() == Some(anchor)))
        {
            (start..end, rows)
        } else {
            // Joined print lines, nested markup and illustrations use their native renderer.
            let book = BookLayout::new(doc, self.book.options, Some(anchor));
            let (range, rows) = self
                .native_spine_edit(doc, &book, anchor, &blocks)
                .unwrap_or_else(|| (0..self.lines.len(), book.lines(doc, self.layout, blocks)));
            if !rows.iter().any(|row| row.reading_anchor() == Some(anchor)) {
                return;
            }
            (range, rows)
        };
        let replaced = range.start..range.start + rows.len();
        let original = self.lines.splice(range, rows).collect();
        self.natural = Some(NaturalRows {
            split_word: anchor,
            rows: original,
            replaced,
            sources: std::mem::take(&mut self.source_lines),
        });
        self.source_lines = Self::source_index(!doc.styled.is_empty(), &self.lines);
        self.index_figures_and_pages();
        self.locate(position, false);
    }

    /// An inserted break in one EPUB spine item cannot reflow another item's DOM.
    /// Apply just its changed rows when the surrounding source and layout agree
    /// with the complete book, including its inter-chapter spacing.
    fn native_spine_edit(
        &self,
        doc: &Document,
        anchored: &BookLayout,
        anchor: Anchor,
        blocks: &std::ops::Range<usize>,
    ) -> Option<(std::ops::Range<usize>, Vec<PageLine>)> {
        let (&start, chapter) = doc.styled.range(..=anchor.block).next_back()?;
        let spine = start..chapter.end;
        if !spine.contains(&anchor.block)
            || spine == *blocks
            || start < blocks.start
            || chapter.end > blocks.end
        {
            return None;
        }
        let normal = self.book.lines(doc, self.layout, spine.clone());
        let mut edited = anchored.lines(doc, self.layout, spine);
        let prefix = normal
            .iter()
            .zip(&edited)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = normal[prefix..]
            .iter()
            .rev()
            .zip(edited[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let changed = prefix..edited.len() - suffix;
        if prefix == 0
            || suffix == 0
            || !edited[changed.clone()]
                .iter()
                .any(|line| line.reading_anchor() == Some(anchor))
        {
            return None;
        }
        let row = Self::line_at(anchor, &normal, &Self::source_index(true, &normal));
        let offset = self.first.checked_sub(row)?;
        let end = normal.len() - suffix;
        let context = prefix - 1..end + 1;
        if self
            .lines
            .get(offset + context.start..offset + context.end)?
            != &normal[context]
        {
            return None;
        }
        Some((
            offset + prefix..offset + end,
            edited.drain(changed).collect(),
        ))
    }

    fn restore_natural(&mut self) {
        if let Some(natural) = self.natural.take() {
            self.lines.splice(natural.replaced, natural.rows);
            self.source_lines = natural.sources;
            self.index_figures_and_pages();
        }
    }
    fn index_figures_and_pages(&mut self) {
        self.figure_starts = Self::index_figures(&self.lines);
        self.index_pages();
    }

    fn use_cached_layout(&mut self, layout: Layout) -> bool {
        if self.layout == layout {
            return true;
        }
        let size = self.book.size(layout);
        if size == self.book.size(self.layout) {
            self.layout = layout;
            self.index_pages();
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
        self.figure_starts = Self::index_figures(&self.lines);
        self.reflow_cache.put(previous);
        self.layout = layout;
        self.index_pages();
    }

    /// Show the regular page or spread containing a rendered row.
    fn regular_first(&self, row: usize) -> usize {
        let starts = self.regular_views();
        if starts.is_empty() {
            let step = self.viewport() * self.layout.columns;
            row / step * step
        } else {
            starts[starts
                .partition_point(|&start| start <= row)
                .saturating_sub(1)]
        }
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

    pub fn position(&self) -> ViewPosition {
        ViewPosition {
            anchor: self.focus,
            illustration: self.focus_illustration,
        }
    }

    pub fn history_position(&mut self) -> HistoryPosition {
        self.finish_resize();
        let view = self.complete_view();
        let rows = view.lines(0);
        HistoryPosition {
            logical: self.position(),
            visible: self.visible_position(),
            chapter_page: !self.layout.scrolling && !self.exact_view,
            split_word: self.natural.as_ref().map(|n| n.split_word),
            leading_rows: rows
                .iter()
                .take_while(|l| l.reading_anchor().is_none())
                .count(),
        }
    }

    pub fn restore_history(&mut self, doc: &Document, saved: HistoryPosition) {
        self.finish_resize();
        self.restore_natural();
        self.resize_origin = None;
        if !self.layout.scrolling
            && (saved.chapter_page || self.pagination_mode == PaginationMode::Chapter)
        {
            self.locate(saved.logical, true);
            return;
        }

        if let Some(anchor) = saved.split_word {
            let split = ViewPosition {
                anchor,
                illustration: None,
            };
            self.locate(split, false);
            self.exact_top(doc, split);
        }
        self.locate(saved.visible, false);
        if saved.visible.illustration.is_none() {
            if self.visible_position().anchor != saved.visible.anchor {
                // The geometry changed since this entry. Establish the saved
                // first word in the new geometry using the original prose.
                self.restore_natural();
                self.locate(saved.visible, false);
                self.exact_top(doc, saved.visible);
            }
            let leading = self.lines[..self.first]
                .iter()
                .rev()
                .take(saved.leading_rows)
                .take_while(|l| l.reading_anchor().is_none())
                .count();
            self.first -= leading;
        }
        self.focus = saved.logical.anchor;
        self.focus_illustration = saved
            .logical
            .illustration
            .filter(|hint| hint.resolve(&self.lines, &self.figure_starts).is_some());
    }

    fn visible_position(&self) -> ViewPosition {
        let rows = &self.lines[self.first..];
        let anchor = rows
            .iter()
            .find_map(PageLine::reading_anchor)
            .unwrap_or(self.focus);
        let illustration = if self.figure_starts.is_empty() {
            None
        } else {
            IllustrationPosition::at(&rows[..rows.len().min(self.layout.height)])
        };
        ViewPosition {
            anchor,
            illustration,
        }
    }

    fn source_index(styled: bool, lines: &[PageLine]) -> Vec<(Anchor, usize)> {
        if !styled {
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
        self.restore_position(ViewPosition {
            anchor,
            illustration: None,
        });
    }

    pub fn restore_position(&mut self, position: ViewPosition) {
        self.finish_resize();
        self.restore_natural();
        self.resize_origin = None;
        self.locate(position, self.pagination_mode == PaginationMode::Chapter);
    }

    /// Search always uses the normal chapter pagination, including in KeepWord.
    pub fn seek_chapter(&mut self, anchor: Anchor) {
        self.finish_resize();
        self.restore_natural();
        self.resize_origin = None;
        self.locate(
            ViewPosition {
                anchor,
                illustration: None,
            },
            true,
        );
    }

    fn position_row(&self, position: ViewPosition) -> usize {
        position
            .illustration
            .and_then(|hint| hint.resolve(&self.lines, &self.figure_starts))
            .unwrap_or_else(|| Self::line_at(position.anchor, &self.lines, &self.source_lines))
    }

    fn locate(&mut self, position: ViewPosition, chapter: bool) {
        let mut row = self.position_row(position);
        self.exact_view = !self.layout.scrolling && !chapter;
        if !self.layout.scrolling {
            if chapter {
                row = self.regular_first(row);
            } else if let Some(line) = self.lines.get(row) {
                // Start at the top of a plate or a page-sized equation. Taller
                // equations can start on the row reached in Scroll.
                if let Some(offset) = line
                    .indivisible_blocks(self.layout.height)
                    .map(|(offset, _)| offset)
                    .max()
                {
                    row = row.saturating_sub(usize::from(offset));
                }
            }
        }
        self.first = row;
        self.focus = position.anchor;
        self.focus_illustration = if self.layout.scrolling || !chapter {
            self.visible_position().illustration
        } else {
            position
                .illustration
                .filter(|hint| hint.resolve(&self.lines, &self.figure_starts).is_some())
        };
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
        // Drag selection can scroll a paged view by a single row. A later
        // page turn still advances to the next regular chapter view.
        let start = if self.layout.scrolling || self.exact_view {
            self.first
        } else {
            self.regular_first(self.first)
        };
        let first = self.view_at(start).end();
        if first > self.first && first < self.lines.len() {
            self.move_to(first);
        }
    }

    /// When a Keep first word page overlaps the page we return to, report the
    /// old opening word so the reader can mark where the overlap begins.
    pub fn previous(&mut self) -> Option<Anchor> {
        self.finish_resize();
        if self.lines.is_empty() || self.layout.height == 0 {
            return None;
        }
        if self.exact_view {
            // The first backward turn leaves the temporary sequence. Locate
            // the word in the original rows before choosing the regular view.
            let position = self.visible_position();
            self.restore_natural();
            self.exact_view = false;
            let row = self.position_row(position);
            let start = self.regular_first(row);
            let rows = &self.lines[start..self.view_at(start).end()];
            let at_start = match position.illustration {
                Some(_) => row == start || rows[..row - start].iter().all(PageLine::is_spacer),
                None => rows.iter().find_map(PageLine::reading_anchor) == Some(position.anchor),
            };
            if at_start {
                self.move_to(self.regular_previous(start));
                return None;
            }
            self.move_to(start);
            return position.illustration.is_none().then_some(position.anchor);
        }
        let first = if self.layout.scrolling {
            self.first.saturating_sub(self.viewport())
        } else {
            self.regular_previous(self.first)
        };
        if first != self.first {
            self.move_to(first);
        }
        None
    }

    fn regular_previous(&self, first: usize) -> usize {
        self.regular_first(first.saturating_sub(1))
    }

    pub fn scroll(&mut self, lines: isize) {
        self.finish_resize();
        let max = self.lines.len().saturating_sub(self.layout.height.max(1));
        let first = self.first.saturating_add_signed(lines).min(max);
        if !self.lines.is_empty() && first != self.first {
            self.move_to(first);
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
        self.resize_origin = None;
        let position = self.visible_position();
        self.focus = position.anchor;
        self.focus_illustration = position.illustration;
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
            surfaces: Vec::new(),
            table_header: false,
            chapter_start: false,
            discretionary_hyphen: false,
            equation: (0, 0),
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
            epub_spine: Default::default(),
            markdown: None,
            fixed_breaks: Default::default(),
            heading_levels: Default::default(),
            chapter_starts: Default::default(),
            inline: Default::default(),
            image_pages: vec![],
            image_sizes: Default::default(),
            styled: Default::default(),
            tables: Default::default(),
            title: "Книга".into(),
            references: vec![],
            epub_anchors: Default::default(),
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
                typography: Default::default(),
                repair_printed: false,
                inline_images: true,
                paragraph_indent: 4,
                compact_spacing: true,
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
        assert!(pager.lines(0).iter().any(|l| l.text.starts_with("море")));
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
        assert!(pager.lines(1)[4].text.is_empty());
        assert_eq!(pager.lines(1)[5].text, "Caption");
        assert_eq!(pager.page_count(), 3);
        assert_eq!(pager.nearby_illustrations().count(), 0);
        pager.next();
        assert_eq!(pager.page_number(), 3);
        assert_eq!(
            pager
                .nearby_illustrations()
                .map(|f| f.reference)
                .collect::<Vec<_>>(),
            [0]
        );
        assert_eq!(pager.lines(0)[1].text, "Four");
        assert!(pager.at_end());
        pager.previous();
        assert_eq!(pager.page_number(), 1);
        assert_eq!(strip(pager.lines(1)), full);

        pager.relayout(&doc, 80, 6, LayoutMode::Single);
        let upcoming = pager
            .next_illustrations()
            .map(|f| (f.row, f.height))
            .collect::<Vec<_>>();
        assert_eq!(upcoming, full);
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
    fn adjacent_illustrations_with_one_text_anchor_restore_independently() {
        let mut doc = document(&["One", "Two", "Caption", "Four", "Five"]);
        for _ in 0..2 {
            doc.references.push(Reference {
                source: 2,
                bytes: None,
                backlink: false,
                label: "Picture".into(),
                target: Target::Image("same.png".into()),
            });
        }
        let mut pager = Pager::new(&doc, 80, 6, LayoutMode::Single);
        pager.next();
        let first = pager.position();
        pager.next();
        let second = pager.position();
        assert_eq!(first.anchor, second.anchor);
        let at = |reference, row| Some(IllustrationPosition { reference, row });
        assert_eq!(
            (first.illustration, second.illustration),
            (at(0, 0), at(1, 0))
        );

        for pos in [second, first] {
            pager.restore_position(pos);
            pager.relayout(&doc, 70, 8, LayoutMode::Single);
            assert_eq!(pager.position(), pos);
            assert_eq!(
                at(pager.lines(0)[0].figures[0].reference, 0),
                pos.illustration
            );
            pager.relayout(&doc, 80, 6, LayoutMode::Single);
            assert_eq!(pager.position(), pos);
        }
        let mut hidden = Pager::with_media(&doc, 80, 6, LayoutMode::Single, true, false);
        hidden.restore_position(second);
        assert_eq!(hidden.anchor(), second.anchor);
        assert_eq!(hidden.position().illustration, None);
    }

    #[test]
    fn illustration_offsets_stop_at_the_actual_extent_of_the_picture_and_its_margin() {
        let mut doc = document(&["One", "Two", "Caption", "Four", "Five"]);
        doc.references.push(Reference {
            source: 2,
            bytes: None,
            backlink: false,
            label: "Picture".into(),
            target: Target::Image("plate.png".into()),
        });
        let mut pager = Pager::new(&doc, 80, 6, LayoutMode::Scroll);
        for (saved_row, expected_row) in [(i32::MIN, -1), (-1, -1), (2, 2), (i32::MAX, 3)] {
            let illustration = Some(IllustrationPosition {
                reference: 0,
                row: saved_row,
            });
            pager.restore_position(ViewPosition {
                anchor: Anchor::default(),
                illustration,
            });
            assert_eq!(pager.position().illustration.unwrap().row, expected_row);
            let edge = pager.first;
            pager.restore_position(pager.position());
            assert_eq!(pager.first, edge);
        }
        pager.relayout(&doc, 80, 3, LayoutMode::Scroll);
        assert_eq!(pager.position().illustration.unwrap().row, 1);
    }

    #[test]
    fn preload_finds_two_figures_on_each_side_across_text_only_spreads() {
        let mut doc = document(&["Short paragraph."; 120]);
        for source in (10..120).step_by(15) {
            doc.references.push(Reference {
                source,
                bytes: None,
                backlink: false,
                label: "Picture".into(),
                target: Target::Image(format!("art-{source}")),
            });
        }
        let mut pager = Pager::new(&doc, 80, 5, LayoutMode::Spread);
        pager.seek(Anchor { block: 60, byte: 0 });
        assert!(pager.next_illustrations().next().is_none());
        let neighbours: Vec<_> = pager
            .nearby_illustrations()
            .map(|f| doc.references[f.reference].source)
            .collect();
        assert_eq!(neighbours, [70, 85, 55, 40]);
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
        let mut pager = Pager::for_display_position_with_pagination(
            &doc,
            (44, 2),
            LayoutMode::Single,
            SPREAD_GAP,
            ReflowOptions {
                typography: Default::default(),
                repair_printed: true,
                inline_images: false,
                paragraph_indent: 3,
                compact_spacing: true,
                paragraph_spacing: None,
            },
            Some(ViewPosition {
                anchor: at,
                illustration: None,
            }),
            (1, 2),
            PaginationMode::KeepWord,
        );
        for width in [44, 24, 100] {
            pager.relayout(&doc, width, 2, LayoutMode::Spread);
            assert_eq!(pager.lines(0)[0].anchor(), at);
            assert!(pager.lines(0)[0].text.starts_with("удалось,"));
            assert_eq!(pager.lines(0)[0].indent, 0);
        }
        pager.previous();
        let regular = Pager::with_options(&doc, 100, 2, LayoutMode::Spread, pager.reflow_options());
        assert_eq!(pager.lines(0)[0].text, regular.lines(0)[0].text);
    }

    #[test]
    fn resize_and_layout_changes_keep_the_position_on_its_chapter_page() {
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
                assert!(
                    (0..pager.columns())
                        .any(|c| pager.lines(c).iter().any(|r| r.text.contains("речку")))
                );
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
