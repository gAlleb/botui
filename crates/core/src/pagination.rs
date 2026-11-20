//! Render the visible EPUB chapters first, then finish the exact book-wide page index.
mod chapters;
pub use chapters::ChapterProgress;
#[cfg(test)]
mod tests;
use super::{
    Anchor, Document, Figure, Layout, LayoutMode, PageLine, Pager, ReflowOptions, Target, media,
    printed, publisher, reflow_cache::Snapshot,
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
            if !block.heading && typography.alignment != TextAlignment::Book {
                let count = lines.len() - before;
                for (i, line) in lines[before..].iter_mut().enumerate() {
                    let spare = usize::from(width)
                        .saturating_sub(line.text.width() + usize::from(line.indent));
                    match typography.alignment {
                        TextAlignment::Right => line.indent = spare as u16,
                        TextAlignment::Center => line.indent = (spare / 2) as u16,
                        TextAlignment::Justify if i + 1 < count => printed::justify(line, spare),
                        _ => (),
                    }
                }
            }
            if block.heading && typography.heading_rules && lines.len() > before {
                lines.push(PageLine {
                    start: Anchor {
                        block: end,
                        byte: block.text.len(),
                    },
                    surfaces: vec![super::Surface::HeadingRule {
                        double: doc.chapter_starts.contains(&start),
                    }],
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

impl Layout {
    /// Fill the column immediately before an exact reading position. If its
    /// first row would cut a picture, leave the whole picture on the earlier page.
    fn page_start(self, rows: &[PageLine], end: usize) -> usize {
        let earliest = end.saturating_sub(self.height);
        let limit = rows[earliest..end]
            .iter()
            .rposition(|line| line.chapter_start)
            .map_or(earliest, |i| earliest + i);
        let mut start = limit;
        while start < end {
            let mut after = start;
            for figure in &rows[start].figures {
                if figure.row > 0 && usize::from(figure.height) <= self.height {
                    let bottom = start + usize::from(figure.height - figure.row);
                    if bottom >= end {
                        // The reading position is in the image or in an overlapping float.
                        return limit;
                    }
                    after = after.max(bottom);
                }
            }
            if after == start {
                break;
            }
            start = after;
        }
        start
    }

    /// Find a column break without cutting an illustration that fits on a fresh page.
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
            for figure in &rows[end - 1].figures {
                if figure.row + 1 < figure.height && usize::from(figure.height) <= self.height {
                    let top = (end - 1).saturating_sub(figure.row.into());
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
    visible: Vec<PageLine>,
    cancel: Arc<AtomicBool>,
    start: Option<mpsc::Sender<()>>,
    task: Option<thread::JoinHandle<Option<Snapshot>>>,
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
        self.chapter_rows.clear();
        if self.book.has_chapters {
            self.chapter_rows.extend(
                self.lines
                    .iter()
                    .enumerate()
                    .filter_map(|(row, line)| line.chapter_start.then_some(row)),
            );
        }
        if self.layout.scrolling
            || (!self.book.has_figures && !self.book.has_chapters)
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

    pub(super) fn index_position(&mut self) {
        self.positioned_spreads.clear();
        let columns = self.layout.columns;
        if self.page_starts.is_empty() || self.regular_views().binary_search(&self.first).is_ok() {
            return;
        }

        // An exact word can start a view between two regular page starts.
        // Walk back only while forward pagination gives exactly the same break.
        let mut end = self.first;
        let mut preceding = Vec::new();
        while end > 0 && self.page_starts.binary_search(&end).is_err() {
            let start = self.layout.page_start(&self.lines, end);
            if self.layout.page_end(&self.lines, start) != end {
                break;
            }
            preceding.push(start);
            end = start;
        }
        // A preceding ordinary page may overlap the first exact page. Keep both
        // starts so that turning forward again returns to the same spread.
        // Pair pages backwards only inside this chapter. Earlier chapters
        // already have complete, reversible spreads of their own.
        let chapter = self.chapter_rows.partition_point(|&r| r <= self.first);
        let chapter_start = chapter.checked_sub(1).map_or(0, |i| self.chapter_rows[i]);
        let begin = self.page_starts.partition_point(|&r| r < chapter_start);
        let before = self.page_starts.partition_point(|&row| row < end);
        let earlier_columns = preceding
            .into_iter()
            .chain(self.page_starts[begin..before].iter().rev().copied());
        self.positioned_spreads
            .extend(earlier_columns.skip(columns - 1).step_by(columns));
        if self.positioned_spreads.last() != Some(&chapter_start) {
            self.positioned_spreads.push(chapter_start);
        }
        let regular = if columns == 2 {
            &self.spread_starts
        } else {
            &self.page_starts
        };
        let prior = regular.partition_point(|&r| r < chapter_start);
        self.positioned_spreads
            .extend(regular[..prior].iter().rev().copied());
        self.positioned_spreads.reverse();

        let mut start = self.first;
        while start < self.lines.len() {
            if let Ok(index) = regular.binary_search(&start) {
                self.positioned_spreads.extend_from_slice(&regular[index..]);
                break;
            }
            self.positioned_spreads.push(start);
            start = Viewport::new(self.layout, &self.lines, start).end();
        }
    }

    fn view_at(&self, first: usize) -> Viewport<'_> {
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
            let snapshot = pending
                .task
                .take()
                .unwrap()
                .join()
                .expect("EPUB layout worker panicked")
                .expect("active EPUB layout was cancelled");
            self.install(snapshot, pending.layout);
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
    ) {
        let layout = Layout {
            image_cell: self.image_cell,
            ..Layout::new(width, height, mode, gap)
        };
        if let Some(p) = &self.pending {
            if p.layout == layout {
                if p.task.as_ref().unwrap().is_finished() {
                    self.finish_resize();
                }
                return;
            }
            self.pending = None;
        }
        if layout == self.layout {
            return;
        }
        if self.use_cached_layout(layout) {
            return;
        }
        if doc.styled.len() > 1 && width > 0 && height > 0 && self.prepare_viewport(doc, layout) {
            return;
        }
        self.relayout_with_gap(doc, width, height, mode, gap);
    }

    fn prepare_viewport(&mut self, doc: &Arc<Document>, layout: Layout) -> bool {
        use std::ops::Bound::{Excluded, Unbounded};
        let Some((&start, _)) = doc.styled.range(..=self.focus.block).next_back() else {
            return false;
        };
        let boundaries = doc
            .styled
            .range((Excluded(start), Unbounded))
            .map(|(&i, _)| i)
            .chain(std::iter::once(doc.blocks.len()));
        let mut end = start;
        let mut rows = Vec::new();
        let count = layout.height * layout.columns;
        let mut first = 0;
        let position = self.position();
        for next in boundaries {
            rows.extend(self.book.lines(doc, layout, end..next));
            if end != start {
                rows = self.book.format_rows(rows, doc, layout);
            }
            end = next;
            let index = Self::source_index(doc, &rows);
            // A hidden chapter opening can resolve to the last visible word of its predecessor.
            if start > 0 && index.first().is_none_or(|(at, _)| *at > self.focus) {
                return false;
            }
            first = match position.illustration {
                Some(hint) => {
                    let Some(row) = hint.resolve(&rows, &Self::index_figures(&rows)) else {
                        // The partial chapter layout cannot resolve this picture.
                        // Use the full layout, including the text fallback if it is hidden.
                        return false;
                    };
                    row
                }
                None => Self::line_at(self.focus, &rows, &index),
            };
            if end == doc.blocks.len() || first + count <= rows.len() {
                break;
            }
        }
        let visible = rows[first..(first + count).min(rows.len())].to_vec();
        let book = self.book.clone();
        let doc = doc.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let (start_index, frame_presented) = mpsc::channel();
        let task = thread::Builder::new()
            .name("epub-pagination".into())
            .spawn(move || {
                frame_presented.recv().ok()?;
                let range = start..end;
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
                let lines = book.format_rows(lines, &doc, layout);
                Some(Snapshot {
                    size: book.size(layout),
                    sources: Self::source_index(&doc, &lines),
                    lines,
                })
            });
        let Ok(task) = task else { return false };
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
