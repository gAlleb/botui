//! Render the visible EPUB chapters first, then finish the exact book-wide page index.
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
}
impl BookLayout {
    pub(super) fn new(doc: &Document, options: ReflowOptions, resume: Option<Anchor>) -> Self {
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
            has_figures: !images.is_empty() || (options.inline_images && !positioned.is_empty()),
            images,
            reflow: publisher::Reflow::new(doc),
        }
    }
    pub(super) fn size(&self, layout: Layout) -> (u16, usize) {
        (
            layout.width,
            if self.uses_height { layout.height } else { 0 },
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
        lines
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
        let paragraph_indent = self.options.paragraph_indent;
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
            start = end + 1;
            if lines.len() > before {
                lines.push(PageLine {
                    spans: Vec::new(),
                    indent: 0,
                    figures: Vec::new(),
                    table_header: false,
                    text: String::new(),
                    heading: false,
                    start: Anchor {
                        block: end,
                        byte: block.text.len(),
                    },
                });
            }
        }
        if range.end == doc.blocks.len()
            && lines
                .last()
                .is_some_and(|l| l.figures.is_empty() && l.text.is_empty())
        {
            lines.pop();
        }
        if range.end == doc.blocks.len() {
            draw_images(lines, doc.blocks.len());
        }
    }
}

impl Layout {
    /// Find a column break without cutting an illustration that fits on a fresh page.
    fn page_end(self, rows: &[PageLine], first: usize) -> usize {
        let limit = (first + self.height).min(rows.len());
        let mut end = limit;
        while !self.scrolling && end > first {
            let mut before = end;
            for figure in &rows[end - 1].figures {
                if figure.row + 1 < figure.height && usize::from(figure.height) <= self.height {
                    let top = (end - 1).saturating_sub(figure.row.into());
                    if top <= first {
                        // Overlapping floats or an exact text anchor inside an image.
                        // No earlier whole-image break is possible on this page.
                        return limit;
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
    fn new(layout: Layout, rows: &'a [PageLine], first: usize, following: &[usize]) -> Self {
        let mut bounds = [first; 3];
        for column in 0..layout.columns {
            bounds[column + 1] = following
                .get(column)
                .copied()
                .unwrap_or_else(|| layout.page_end(rows, bounds[column]));
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
        if self.layout.scrolling || !self.book.has_figures || self.layout.height == 0 {
            return;
        }
        // A search, bookmark, or synced position can start a page at an exact word.
        // Index both sides of that break so turning back retraces the same columns.
        let mut start = 0;
        while start < self.lines.len() {
            self.page_starts.push(start);
            let end = self.layout.page_end(&self.lines, start);
            start = if start < self.first {
                end.min(self.first)
            } else {
                end
            };
        }
    }

    pub(super) fn complete_view(&self) -> Viewport<'_> {
        let next = self
            .page_starts
            .partition_point(|&start| start <= self.first);
        Viewport::new(
            self.layout,
            &self.lines,
            self.first,
            &self.page_starts[next..],
        )
    }

    pub fn viewport_rows(&self) -> Viewport<'_> {
        match &self.pending {
            Some(pending) => Viewport::new(pending.layout, &pending.visible, 0, &[]),
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
        let layout = Layout::new(width, height, mode, gap);
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
        for next in boundaries {
            rows.extend(self.book.lines(doc, layout, end..next));
            end = next;
            let index = Self::source_index(doc, &rows);
            // A hidden chapter opening can resolve to the last visible word of its predecessor.
            if start > 0 && index.first().is_none_or(|(at, _)| *at > self.focus) {
                return false;
            }
            first = Self::line_at(self.focus, &rows, &index);
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
