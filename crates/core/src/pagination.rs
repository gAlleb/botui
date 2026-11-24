//! Render the visible EPUB chapters first, then finish the exact book-wide page index.
mod chapters;
pub use chapters::ChapterProgress;
#[cfg(test)]
mod tests;
use super::{
    Anchor, Document, Figure, Layout, LayoutMode, PageLine, Pager, PaginationMode, ReflowOptions,
    Target, media, printed, publisher, reflow_cache::Snapshot,
};
use std::{
    collections::BTreeMap,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

pub(super) struct BookLayout {
    pub(super) options: ReflowOptions,
    pub(super) resume: Option<Anchor>,
    pub(super) printed: Vec<bool>,
    images: BTreeMap<usize, Vec<usize>>,
    reflow: publisher::Reflow,
    uses_height: bool,
    has_figures: bool,
    has_chapters: bool,
    chapter_progress: chapters::Progress,
}
impl BookLayout {
    pub(super) fn new(doc: &Document, mut options: ReflowOptions, resume: Option<Anchor>) -> Self {
        options.paragraph_spacing = options.paragraph_spacing.map(|n| n.min(2));
        let mut printed = if options.repair_printed {
            printed::printed_lines(&doc.blocks)
        } else {
            vec![false; doc.blocks.len()]
        };
        for (&start, row) in &doc.tables {
            for v in printed
                .iter_mut()
                .skip(start)
                .take(row.end.saturating_sub(start))
            {
                *v = false;
            }
        }
        for r in &doc.references {
            if let Some(p) = printed.get_mut(r.source) {
                *p = false;
            }
        }
        for (&start, chapter) in &doc.styled {
            if chapter.preserve_structure {
                printed[start..chapter.end].fill(false);
            }
            if start > 0 {
                printed[start - 1] = false;
            }
        }
        let positioned: std::collections::HashSet<_> = doc
            .styled
            .values()
            .flat_map(|c| c.images.iter().flatten().copied())
            .collect();
        let mut images = std::collections::BTreeMap::<usize, Vec<usize>>::new();
        for (i, r) in doc.references.iter().enumerate() {
            if options.inline_images
                && matches!(r.target, Target::Image(_))
                && !positioned.contains(&i)
            {
                images
                    .entry(r.source.min(doc.blocks.len()))
                    .or_default()
                    .push(i);
            }
        }

        Self {
            options,
            resume,
            printed,
            uses_height: !doc.styled.is_empty() || !images.is_empty(),
            has_chapters: !doc.chapter_starts.is_empty(),
            chapter_progress: chapters::Progress::new(doc),
            has_figures: !images.is_empty() || (options.inline_images && !positioned.is_empty()),
            images,
            reflow: publisher::Reflow::new(doc),
        }
    }
    pub(super) fn size(&self, layout: Layout) -> super::reflow_cache::Geometry {
        (
            layout.width,
            if self.uses_height { layout.height } else { 0 },
            if self.has_figures {
                layout.image_cell
            } else {
                (1, 2)
            },
            self.has_chapters && layout.scrolling,
        )
    }

    pub(super) fn lines(
        &self,
        doc: &Document,
        layout: Layout,
        range: Range<usize>,
    ) -> Vec<PageLine> {
        let mut styled = self
            .reflow
            .layout(doc, layout, self, |start| range.contains(&start));
        let mut lines = Vec::new();
        self.append(&mut lines, doc, layout, range, &mut styled);
        self.format_rows(lines, doc, layout)
    }

    fn format_rows(&self, lines: Vec<PageLine>, doc: &Document, layout: Layout) -> Vec<PageLine> {
        let lines = crate::prose::rewrap(
            lines,
            doc,
            layout.width,
            self.options.typography,
            self.resume,
        );
        let lines = media::space_figures(lines, doc, layout.image_cell);
        if self.has_chapters {
            chapters::format(lines, doc, layout.scrolling)
        } else {
            lines
        }
    }

    // Ranges start and end at chapter boundaries, so printed paragraphs and tables stay intact.
    fn append(
        &self,
        lines: &mut Vec<PageLine>,
        doc: &Document,
        layout: Layout,
        range: Range<usize>,
        styled: &mut BTreeMap<usize, Vec<PageLine>>,
    ) {
        let width = layout.width;
        if width == 0 {
            return;
        }
        let images = &self.images;
        let printed = &self.printed;
        let resume = self.resume;
        use super::TextAlignment;
        use unicode_width::UnicodeWidthStr;
        let typography = self.options.typography;
        let paragraph_indent = match typography.alignment {
            TextAlignment::Right | TextAlignment::Center => 0,
            _ => self.options.paragraph_indent,
        };
        let draw_images = |lines: &mut Vec<PageLine>, boundary: usize| {
            if let Some(ids) = images.get(&boundary) {
                let anchor = if boundary == 0 {
                    Anchor::default()
                } else {
                    Anchor {
                        block: boundary - 1,
                        byte: doc.blocks[boundary - 1].text.len(),
                    }
                };
                for &id in ids {
                    let height = (layout.height * 2 / 3)
                        .max(1)
                        .min(usize::from(width) / 2 + 1) as u16;
                    for row in 0..height {
                        lines.push(PageLine {
                            start: anchor,
                            figures: vec![Figure {
                                reference: id,
                                x: 0,
                                width,
                                row,
                                height,
                            }],
                            ..Default::default()
                        });
                    }
                }
            }
        };
        let mut start = range.start;
        while start < range.end {
            if let Some(chapter) = doc.styled.get(&start)
                && (chapter.preserve_structure || !printed[start..chapter.end].contains(&true))
            {
                draw_images(lines, start);
                lines.extend(styled.remove(&start).unwrap_or_default());
                start = chapter.end;
                continue;
            }
            draw_images(lines, start);
            if let Some(row) = doc.tables.get(&start) {
                media::table(lines, start, row, width.into());
                let end = row.end.clamp(start + 1, doc.blocks.len());
                for i in start + 1..end {
                    draw_images(lines, i);
                }
                start = end;
                continue;
            }
            let end = printed::end(&doc.blocks, printed, start);
            let end = (start + 1..=end)
                .find(|i| {
                    images.contains_key(i)
                        || doc.fixed_breaks.contains(i)
                        || doc.chapter_starts.contains(i)
                        || doc.tables.contains_key(i)
                        || doc.styled.contains_key(i)
                })
                .map_or(end, |i| i - 1);
            let block = &doc.blocks[end];
            let before = lines.len();
            printed::wrap(
                lines,
                &doc.blocks[start..=end],
                start,
                usize::from(width),
                paragraph_indent,
                resume.filter(|a| (start..=end).contains(&a.block)),
            );
            printed::faces(&mut lines[before..], doc, start, end);
            if block.heading || typography.alignment != TextAlignment::Book {
                let count = lines.len() - before;
                for (i, line) in lines[before..].iter_mut().enumerate() {
                    let spare = usize::from(width)
                        .saturating_sub(line.text.width() + usize::from(line.indent));
                    match typography.alignment {
                        TextAlignment::Book | TextAlignment::Justify if block.heading => {
                            line.indent = (spare / 2) as u16
                        }
                        TextAlignment::Right => line.indent = spare as u16,
                        TextAlignment::Center => line.indent = (spare / 2) as u16,
                        TextAlignment::Justify if i + 1 < count => printed::justify(line, spare),
                        _ => (),
                    }
                }
            }
            if block.heading
                && typography.heading_rules
                && lines.len() > before
                && let Some(double) = if doc.epub_spine.is_empty() {
                    Some(doc.chapter_starts.contains(&start))
                } else {
                    doc.heading_levels
                        .get(&end)
                        .filter(|&&level| level <= 2)
                        .map(|&level| level == 1)
                }
            {
                lines.push(PageLine {
                    start: Anchor {
                        block: end,
                        byte: block.text.len(),
                    },
                    surfaces: vec![super::Surface::HeadingRule { double }],
                    ..Default::default()
                });
            }
            start = end + 1;
            if lines.len() > before {
                let gap = self.options.paragraph_spacing.filter(|_| {
                    !block.heading
                        && doc.blocks.get(start).is_some_and(|next| !next.heading)
                        && !doc.fixed_breaks.contains(&start)
                        && !images.contains_key(&start)
                        && !doc.tables.contains_key(&start)
                        && !doc.styled.contains_key(&start)
                });
                lines.resize_with(lines.len() + usize::from(gap.unwrap_or(1)), || PageLine {
                    start: Anchor {
                        block: end,
                        byte: block.text.len(),
                    },
                    ..Default::default()
                });
            }
        }
        if range.end == doc.blocks.len()
            && lines.last().is_some_and(|l| {
                l.figures.is_empty() && l.text.is_empty() && !l.has_visible_surface()
            })
        {
            lines.pop();
        }
        if range.end == doc.blocks.len() {
            draw_images(lines, doc.blocks.len());
        }
    }
}

impl PageLine {
    pub(super) fn indivisible_blocks(
        &self,
        page_height: usize,
    ) -> impl Iterator<Item = (u16, u16)> + '_ {
        let (_, equation_height) = self.equation;
        let whole_equation = equation_height > 1 && usize::from(equation_height) <= page_height;
        self.figures
            .iter()
            .map(|f| (f.row, f.height))
            .chain(whole_equation.then_some(self.equation))
    }
}

impl Layout {
    /// Keep illustrations and display equations whole when they fit on a fresh page.
    fn page_end(self, rows: &[PageLine], first: usize) -> usize {
        let limit = (first + self.height).min(rows.len());
        let next = (first + 1).min(limit);
        let boundary = if self.scrolling {
            limit
        } else {
            rows[next..limit]
                .iter()
                .position(|line| line.chapter_start)
                .map_or(limit, |i| next + i)
        };
        let mut end = boundary;
        while !self.scrolling && end > first {
            let mut before = end;
            for (row, height) in rows[end - 1].indivisible_blocks(self.height) {
                if row + 1 < height && usize::from(height) <= self.height {
                    let top = (end - 1).saturating_sub(row.into());
                    if top <= first {
                        // Overlapping floats or an exact text anchor inside an image.
                        // No earlier whole-image break is possible on this page.
                        return boundary;
                    }
                    before = before.min(top);
                }
            }
            if before == end {
                break;
            }
            end = before;
        }
        end
    }
}

/// The exact rows currently visible in the reading pane, including both pages of a spread.
pub struct Viewport<'a> {
    layout: Layout,
    rows: &'a [PageLine],
    bounds: [usize; 3],
}
impl<'a> Viewport<'a> {
    fn new(layout: Layout, rows: &'a [PageLine], first: usize) -> Self {
        let mut bounds = [first; 3];
        for column in 0..layout.columns {
            if column > 0 && rows.get(bounds[column]).is_some_and(|r| r.chapter_start) {
                bounds[column + 1] = bounds[column];
                break;
            }
            bounds[column + 1] = layout.page_end(rows, bounds[column]);
        }
        Self {
            layout,
            rows,
            bounds,
        }
    }
    pub(super) fn end(&self) -> usize {
        self.bounds[self.layout.columns]
    }
    pub fn columns(&self) -> usize {
        self.layout.columns
    }
    pub fn width(&self) -> u16 {
        self.layout.width
    }
    pub fn gutter(&self) -> u16 {
        self.layout.gutter
    }
    pub fn lines(&self, column: usize) -> &'a [PageLine] {
        if column >= self.layout.columns {
            return &[];
        }
        &self.rows[self.bounds[column]..self.bounds[column + 1]]
    }
}

pub(super) struct Pending {
    layout: Layout,
    visible: Arc<[PageLine]>,
    cancel: Arc<AtomicBool>,
    start: Option<mpsc::Sender<()>>,
    task: Option<thread::JoinHandle<Option<Pager>>>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Pager {
    pub(super) fn index_pages(&mut self) {
        self.page_starts.clear();
        self.spread_starts.clear();
        if self.layout.scrolling
            || (!self.book.has_figures
                && !self.book.has_chapters
                && !self.lines.iter().any(|l| l.equation.1 > 1))
            || self.layout.height == 0
        {
            return;
        }
        let mut start = 0;
        while start < self.lines.len() {
            let view = Viewport::new(self.layout, &self.lines, start);
            if view.columns() == 2 {
                self.spread_starts.push(start);
            }
            for column in 0..view.columns() {
                if !view.lines(column).is_empty() {
                    self.page_starts.push(view.bounds[column]);
                }
            }
            start = view.end();
        }
    }

    pub(super) fn regular_views(&self) -> &[usize] {
        if self.layout.columns == 2 {
            &self.spread_starts
        } else {
            &self.page_starts
        }
    }

    pub(super) fn view_at(&self, first: usize) -> Viewport<'_> {
        Viewport::new(self.layout, &self.lines, first)
    }

    /// Logical percentage of the chapter at the first visible content row.
    pub fn chapter_progress(&self) -> Option<ChapterProgress<'_>> {
        let view = self.viewport_rows();
        let first = view.rows[view.bounds[0]..view.end()]
            .iter()
            .find(|row| row.reading_anchor().is_some())?;
        let figure = first
            .figures
            .first()
            .filter(|_| first.text.trim().is_empty());
        self.book
            .chapter_progress
            .at(first.start, figure.map(|f| f.reference))
    }

    pub(super) fn complete_view(&self) -> Viewport<'_> {
        self.view_at(self.first)
    }

    pub(super) fn index_figures(lines: &[PageLine]) -> Vec<(usize, Figure)> {
        lines
            .iter()
            .enumerate()
            .flat_map(|(row, line)| {
                line.figures
                    .iter()
                    .filter(|f| f.row == 0)
                    .map(move |f| (row, *f))
            })
            .collect()
    }

    /// The closest two illustrations beyond each edge of the current spread.
    pub fn nearby_illustrations(&self) -> impl Iterator<Item = &Figure> {
        let index = if self.pending.is_none() {
            self.figure_starts.as_slice()
        } else {
            &[]
        };
        let before = index.partition_point(|(row, _)| *row < self.first);
        let after = if index.is_empty() {
            0
        } else {
            let end = self.complete_view().end();
            index.partition_point(|(row, _)| *row < end)
        };
        let previous = index[..before]
            .iter()
            .rev()
            .filter(|(row, f)| row + usize::from(f.height) <= self.first);
        index[after..]
            .iter()
            .take(2)
            .chain(previous.take(2))
            .map(|(_, f)| f)
    }

    /// Illustrations on the next page or spread, in reading order.
    pub fn next_illustrations(&self) -> impl Iterator<Item = &Figure> {
        let rows = if self.book.has_figures && self.pending.is_none() {
            let first = self.complete_view().end();
            &self.lines[first..self.view_at(first).end()]
        } else {
            &[]
        };
        rows.iter().flat_map(|line| &line.figures)
    }

    pub fn viewport_rows(&self) -> Viewport<'_> {
        match &self.pending {
            Some(pending) => Viewport::new(pending.layout, &pending.visible, 0),
            None => self.complete_view(),
        }
    }

    pub fn is_paginating(&self) -> bool {
        self.pending.is_some()
    }

    /// Let the remaining chapters use the CPU once the first frame is on screen.
    pub fn start_pagination(&mut self) {
        if let Some(start) = self.pending.as_mut().and_then(|p| p.start.take()) {
            let _ = start.send(());
        }
    }

    /// Complete the book-wide index before an action such as a search or a page turn.
    pub fn finish_resize(&mut self) {
        self.start_pagination();
        if let Some(mut pending) = self.pending.take() {
            let mut ready = pending
                .task
                .take()
                .unwrap()
                .join()
                .expect("EPUB layout worker panicked")
                .expect("active EPUB layout was cancelled");
            std::mem::swap(&mut ready.recent_views, &mut self.recent_views);
            std::mem::swap(&mut ready.reflow_cache, &mut self.reflow_cache);
            let previous = std::mem::replace(self, ready);
            if previous.natural.is_none() {
                self.reflow_cache.put(Snapshot {
                    size: previous.book.size(previous.layout),
                    sources: previous.source_lines,
                    lines: previous.lines,
                });
            }
        }
    }

    /// Prepare the visible rows. Call `start_pagination` after presenting them.
    /// The previous complete index stays usable until the replacement is ready.
    pub fn resize_for_display(
        &mut self,
        doc: &Arc<Document>,
        width: u16,
        height: u16,
        mode: LayoutMode,
        gap: u16,
    ) -> bool {
        let layout = Layout {
            image_cell: self.image_cell,
            ..Layout::new(width, height, mode, gap)
        };
        let replaced = self.pending.as_ref().is_some_and(|p| p.layout != layout);
        if let Some(p) = &self.pending {
            if p.layout == layout {
                if p.task.as_ref().unwrap().is_finished() {
                    self.finish_resize();
                }
                return false;
            }
            self.pending = None;
        }
        if layout == self.layout {
            if replaced {
                self.open_position(doc, self.resize_position());
            }
            return replaced;
        }
        let position = self.resize_position();
        self.resize_origin = Some(position);
        let cached_view = self
            .recent_views
            .iter()
            .find(|(size, at, _)| *size == layout && *at == position)
            .map(|(_, _, rows)| rows.clone());
        if let Some(visible) = cached_view
            && self.prepare_index(doc, layout, visible, 0..0, Vec::new())
        {
            return true;
        }
        let chapter_page = !layout.scrolling && self.pagination_mode == PaginationMode::Chapter;
        if self.natural.is_none() && chapter_page && self.use_cached_layout(layout) {
            self.open_position(doc, position);
        } else if !(doc.styled.len() > 1
            && width > 0
            && height > 0
            && self.prepare_viewport(doc, layout))
        {
            self.relayout_with_gap(doc, width, height, mode, gap);
        }
        true
    }

    fn prepare_viewport(&mut self, doc: &Arc<Document>, layout: Layout) -> bool {
        // Chapter pagination needs the preceding page breaks. Exact-word views
        // can start layout in the spine item containing the target.
        let anchor = self.resize_position().anchor;
        let from = if layout.scrolling || self.pagination_mode == PaginationMode::KeepWord {
            anchor.block
        } else {
            doc.chapter_starts
                .range(..=anchor.block)
                .next_back()
                .copied()
                .unwrap_or(0)
        };
        let start = doc.styled.range(..=from).next_back().map_or(0, |(&s, _)| s);
        self.prepare_from(doc, layout, start)
    }

    fn prepare_from(&mut self, doc: &Arc<Document>, layout: Layout, start: usize) -> bool {
        use std::ops::Bound::{Excluded, Unbounded};
        let boundaries = doc
            .styled
            .range((Excluded(start), Unbounded))
            .map(|(&i, _)| i)
            .chain(std::iter::once(doc.blocks.len()));
        let mut end = start;
        let mut rows = Vec::new();
        let count = layout.height * layout.columns;
        let position = self.resize_position();
        let chapter_page = !layout.scrolling && self.pagination_mode == PaginationMode::Chapter;
        let mut visible = Vec::new();
        for next in boundaries {
            rows.extend(self.book.lines(doc, layout, end..next));
            if end != start {
                rows = self.book.format_rows(rows, doc, layout);
            }
            end = next;
            if end <= position.anchor.block {
                continue;
            }
            let mut preview =
                Self::from_lines(doc, layout, self.book.clone(), self.pagination_mode, rows);
            // Hidden chapter openings and figures may resolve outside this range.
            if (start > 0
                && preview
                    .source_lines
                    .first()
                    .is_none_or(|(at, _)| *at > position.anchor))
                || position.illustration.is_some_and(|hint| {
                    hint.resolve(&preview.lines, &preview.figure_starts)
                        .is_none()
                })
            {
                return false;
            }
            if chapter_page {
                let row = preview.position_row(position);
                if start > 0
                    && !preview
                        .lines
                        .iter()
                        .take(row + 1)
                        .any(|line| line.chapter_start)
                {
                    return self.prepare_from(doc, layout, 0);
                }
                preview.index_pages();
            }
            preview.locate(position, chapter_page);
            if !chapter_page {
                preview.exact_top_in(doc, position, start..end);
            }
            let available = &preview.lines[preview.first..];
            let ready = available.len() >= count || end == doc.blocks.len();
            if ready {
                visible.extend_from_slice(&available[..count.min(available.len())]);
            }
            // The index worker needs the original rows even when the visible
            // paragraph was rewrapped to put the saved word at the top.
            preview.restore_natural();
            rows = preview.lines;
            if ready {
                break;
            }
        }
        let visible: Arc<[PageLine]> = visible.into();
        let prepared = self.prepare_index(doc, layout, visible.clone(), start..end, rows);
        if prepared {
            // Keep only the last few displayed windows, never whole chapters here.
            self.recent_views.truncate(3);
            self.recent_views.push_front((layout, position, visible));
        }
        prepared
    }

    fn prepare_index(
        &mut self,
        doc: &Arc<Document>,
        layout: Layout,
        visible: Arc<[PageLine]>,
        range: Range<usize>,
        rows: Vec<PageLine>,
    ) -> bool {
        let book = self.book.clone();
        let position = self.resize_position();
        let mode = self.pagination_mode;
        let cached = self.reflow_cache.take(book.size(layout));
        let doc = doc.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let (start_index, frame_presented) = mpsc::channel();
        let task = thread::Builder::new()
            .name("epub-pagination".into())
            .spawn(move || {
                frame_presented.recv().ok()?;
                let lines = if let Some(snapshot) = cached {
                    snapshot.lines
                } else {
                    let (start, end) = (range.start, range.end);
                    let mut styled = book.reflow.layout(&doc, layout, &book, |i| {
                        !stop.load(Ordering::Relaxed) && !range.contains(&i)
                    });
                    if stop.load(Ordering::Relaxed) {
                        return None;
                    }
                    let mut lines = if start == 0 {
                        rows
                    } else {
                        let mut prefix = Vec::new();
                        book.append(&mut prefix, &doc, layout, 0..start, &mut styled);
                        prefix.extend(rows);
                        prefix
                    };
                    if end < doc.blocks.len() {
                        book.append(&mut lines, &doc, layout, end..doc.blocks.len(), &mut styled);
                    }
                    if stop.load(Ordering::Relaxed) {
                        return None;
                    }
                    book.format_rows(lines, &doc, layout)
                };
                if stop.load(Ordering::Relaxed) {
                    return None;
                }
                let mut ready = Self::from_lines(&doc, layout, book, mode, lines);
                ready.index_pages();
                ready.open_position(&doc, position);
                ready.resize_origin = Some(position);
                Some(ready)
            });
        let Ok(task) = task else { return false };
        self.focus = position.anchor;
        self.focus_illustration = position.illustration;
        self.pending = Some(Pending {
            layout,
            visible,
            cancel,
            start: Some(start_index),
            task: Some(task),
        });
        true
    }
}
