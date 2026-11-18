//! PDF text interaction is independent of page rendering and saved reading progress.
mod dialogs;
mod geometry;
mod motions;
use crate::{
    cover::{Crop, Picture},
    shelf::background::{Ticket, Worker},
};
use botui_core::{Anchor, TextRange};
use botui_fixed::text::PageText;
use botui_store::{HighlightColor, LocalState, Note, PdfRegion};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
pub(super) use geometry::rotate;
use geometry::{Pointer, View};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    widgets::Paragraph,
};
use ratatui_image::picker::Picker;
use std::{
    collections::BTreeMap,
    io,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct WordAt {
    page: u32,
    index: usize,
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    Copy,
    Lookup(crate::lookup::Purpose),
    Mark(char),
    Jump(char),
    Key(KeyCode),
    Comment,
    Highlight,
    Clear,
    Follow,
    None,
}
type ColoredBounds = ([f32; 4], [u8; 3]);
pub(super) struct ContextPoint {
    at: Position,
    source: Option<(u32, [f32; 2])>,
}
pub(super) enum ContextAction {
    Search(String),
    Source(u32, [f32; 2]),
}
pub(super) fn location(
    region: &PdfRegion,
    mut crop: Crop,
    turns: u8,
    scrolling: bool,
) -> super::history::Location {
    let r = rotate(region.bounds, turns);
    let span = 100. / f32::from(crop.percent.max(100));
    let pan = |center: f32| {
        if span >= 1. {
            0
        } else {
            ((center - span / 2.) / (1. - span) * 255.).clamp(0., 255.) as u8
        }
    };
    crop.x = pan((r[0] + r[2]) / 2.);
    crop.y = pan((r[1] + r[3]) / 2.);
    super::history::Location {
        page: region.page,
        offset: if scrolling {
            ((r[1] - 0.01).max(0.) * 65535.) as u16
        } else {
            0
        },
        crop,
    }
}
struct Patch {
    key: (u64, View, Vec<ColoredBounds>),
    picture: Picture,
}
pub(super) struct Text {
    enabled: bool,
    normal: bool,
    yank_origin: Option<WordAt>,
    keys: crate::vim::Keys,
    previous: Option<(WordAt, WordAt)>,
    cursor: Option<WordAt>,
    start: Option<WordAt>,
    drag: bool,
    dragged: bool,
    note_pending: bool,
    clicks: Option<(Pointer, Instant, u8)>,
    pending: Option<Pointer>,
    pending_end: Option<Pointer>,
    auto_cursor: bool,
    pending_visual: bool,
    context_pending: Option<ContextPoint>,
    follow_link: bool,
    search_forward: bool,
    search_regions: Vec<PdfRegion>,
    search_at: Option<(WordAt, usize)>,
    search_cursor_pending: bool,
    reveal: Option<PdfRegion>,
    worker: Worker,
    job: Option<(u32, Ticket<io::Result<PageText>>)>,
    cache: BTreeMap<u32, Result<PageText, String>>,
    views: Vec<View>,
    patches: Vec<Patch>,
    sixel: Option<super::sixel::Cache>,
    pub notes: Vec<Note>,
    pub message: String,
    pub toolbar: crate::reader::controls::ActionRow,
    buttons: Vec<(Rect, Action)>,
}
impl Text {
    pub fn new(store: Option<&LocalState>) -> Self {
        Self {
            enabled: false,
            normal: false,
            yank_origin: None,
            keys: crate::vim::Keys::default(),
            previous: None,
            cursor: None,
            start: None,
            drag: false,
            dragged: false,
            note_pending: false,
            clicks: None,
            pending: None,
            pending_end: None,
            auto_cursor: false,
            pending_visual: false,
            context_pending: None,
            follow_link: false,
            search_forward: true,
            search_regions: vec![],
            search_at: None,
            search_cursor_pending: false,
            reveal: None,
            worker: Worker::new(),
            job: None,
            cache: BTreeMap::new(),
            views: vec![],
            patches: vec![],
            sixel: None,
            notes: store.and_then(|s| s.notes().ok()).unwrap_or_default(),
            message: String::new(),
            toolbar: Default::default(),
            buttons: vec![],
        }
    }
    pub fn reload(&mut self, store: Option<&LocalState>) {
        *self = Self {
            enabled: self.enabled,
            normal: self.normal,
            auto_cursor: self.normal,
            ..Self::new(store)
        };
    }
    pub fn clear_search(&mut self) {
        self.search_regions.clear();
        self.search_at = None;
    }
    pub fn hide_search(&mut self) -> bool {
        let visible = !self.search_regions.is_empty();
        self.search_regions.clear();
        visible
    }
    pub fn show_source(&mut self, region: PdfRegion) {
        // An editor jump ends the old text interaction. Its highlighted PDF
        // region is a navigation target, not an unfinished text selection.
        self.clear();
        self.clear_search();
        self.search_regions = vec![region];
    }
    pub fn source_point(
        &self,
        click: Option<Position>,
        active_page: Option<u32>,
    ) -> Option<(u32, [f32; 2])> {
        if let Some(p) = click {
            return self
                .views
                .iter()
                .find_map(|v| v.pointer(p.into()).map(|(x, y)| (v.page, [x, y])));
        }
        let focus = self
            .cursor
            .and_then(|at| self.word(at).map(|w| (at.page, w.bounds)))
            .or_else(|| self.search_regions.first().map(|r| (r.page, r.bounds)));
        if let Some((page, b)) = focus
            && (self.views.iter().any(|v| v.page == page && v.rect(b).is_some())
                // A forward jump already supplies valid PDF coordinates before
                // the first raster arrives. F8 can use that known target now.
                || (active_page == Some(page) && !self.views.iter().any(|v| v.page == page)))
        {
            return Some((page, [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5]));
        }
        let v = self
            .views
            .iter()
            .find(|v| active_page.is_none_or(|page| v.page == page))?;
        let (x, y) = v.point(Position::new(
            v.screen.x + v.screen.width / 2,
            v.screen.y + v.screen.height / 2,
        ))?;
        Some((v.page, [x, y]))
    }
    pub fn show_search(
        &mut self,
        hit: &botui_fixed::PageMatch,
        page: &PageText,
    ) -> Option<PdfRegion> {
        let page = self
            .cache
            .get(&hit.page)
            .and_then(|p| p.as_ref().ok())
            .filter(|p| p.interactive && p.text == page.text)
            .cloned()
            .unwrap_or_else(|| page.clone());
        self.search_regions = page
            .units()
            .iter()
            .filter(|w| w.bytes.start < hit.bytes.end && hit.bytes.start < w.bytes.end)
            .map(|w| PdfRegion {
                page: hit.page,
                bounds: w.bounds,
            })
            .collect();
        self.search_at = page
            .units()
            .iter()
            .position(|w| w.bytes.end > hit.bytes.start)
            .map(|index| WordAt {
                page: hit.page,
                index,
            })
            .map(|at| (at, hit.bytes.start));
        self.cache.insert(hit.page, Ok(page));
        if self.normal {
            self.cursor = self.search_at.map(|(at, _)| at);
            self.search_cursor_pending = true;
            self.start = None;
            self.pending = None;
            self.pending_visual = false;
            self.reveal = None;
        }
        self.search_regions.first().cloned()
    }
    pub fn search_origin(&self) -> Option<(u32, usize)> {
        let at = self.cursor.filter(|_| self.normal)?;
        let byte = self.search_at.filter(|(word, _)| *word == at).map_or_else(
            || self.word(at).map(|w| w.bytes.start),
            |(_, byte)| Some(byte),
        )?;
        Some((at.page, byte))
    }
    pub fn search_forward(&self) -> bool {
        self.search_forward
    }
    pub fn expects_mark(&self) -> bool {
        self.keys.expects_literal()
    }
    pub fn normal(&self) -> bool {
        self.normal
    }
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.sixel.as_ref().is_some_and(super::sixel::Cache::busy)
    }
    pub fn poll(&mut self, path: &Path) {
        if let Some(cache) = &mut self.sixel {
            cache.poll();
        }
        if let Some((page, job)) = &self.job {
            let page = *page;
            match job.receiver.try_recv() {
                Ok(text) => {
                    if let Ok(new) = &text {
                        self.remap_units(page, new);
                    }
                    self.cache.insert(page, text.map_err(|e| e.to_string()));
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.cache
                        .insert(page, Err("PDF text extraction interrupted".into()));
                    self.job = None;
                }
                Err(_) => (),
            }
        }
        self.trim_cache();
        if !self.enabled {
            return;
        }
        // A search jump changes the logical page before the next raster frame.
        // Do not replace its cursor with a word from the old viewport meanwhile.
        if self.search_cursor_pending
            && self
                .cursor
                .and_then(|at| self.word(at).map(|w| (at, w)))
                .is_some_and(|(at, w)| {
                    self.views
                        .iter()
                        .any(|v| v.page == at.page && v.rect(w.bounds).is_some())
                })
        {
            self.search_cursor_pending = false;
        }
        // Manual page turns/scrolling should not strand the keyboard cursor on
        // a word that is no longer on screen. An active range keeps its anchor.
        if self.start.is_none()
            && !self.search_cursor_pending
            && self.reveal.is_none()
            && self.cursor.is_some_and(|at| {
                self.cache
                    .get(&at.page)
                    .and_then(|t| t.as_ref().ok())
                    .and_then(|t| t.units().get(at.index))
                    .is_some_and(|w| {
                        !self
                            .views
                            .iter()
                            .any(|v| v.page == at.page && v.rect(w.bounds).is_some())
                    })
            })
        {
            self.cursor = None;
        }
        if let Some(p) = self.pending {
            if let Some(view) = self.views.iter().find(|v| v.screen.contains(p.cell)) {
                if let Some(result) = self.cache.get(&view.page) {
                    self.message = match result {
                        Ok(t) if t.units().is_empty() => {
                            "This page has no embedded text layer".into()
                        }
                        Err(e) => e.clone(),
                        _ => String::new(),
                    };
                    self.cursor = self.hit(p, false);
                    self.start = self.cursor;
                    if self.cursor.is_some()
                        && let Some(end) = self.pending_end.take()
                    {
                        self.cursor = self.hit(end, true).or(self.cursor);
                    }
                    if !self.dragged
                        && !self.drag
                        && self.cursor.is_some()
                        && self.active_note().is_none()
                    {
                        self.select_word();
                    }
                    self.pending = None;
                }
            } else {
                self.pending = None;
            }
        }
        if self.auto_cursor && self.cursor.is_none() && self.pending.is_none() {
            self.cursor = self.views.iter().find_map(|v| {
                self.cache
                    .get(&v.page)?
                    .as_ref()
                    .ok()?
                    .units()
                    .iter()
                    .position(|w| v.rect(w.bounds).is_some())
                    .map(|index| WordAt {
                        page: v.page,
                        index,
                    })
            });
            if self.cursor.is_none() {
                self.message = self
                    .views
                    .iter()
                    .find_map(|v| match self.cache.get(&v.page)? {
                        Ok(t) if t.units().is_empty() => {
                            Some("This page has no embedded text layer".into())
                        }
                        Err(e) => Some(e.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
            }
        }
        if self.pending_visual && self.cursor.is_some() {
            self.start = self.cursor;
            self.pending_visual = false;
        }
        if self
            .job
            .as_ref()
            .is_some_and(|(p, _)| !self.views.iter().any(|v| v.page == *p))
        {
            self.job = None;
        }
        if self.job.is_none()
            && let Some(page) = self.views.iter().map(|v| v.page).find(|p| {
                self.cache
                    .get(p)
                    .is_none_or(|v| v.as_ref().is_ok_and(|t| !t.interactive))
            })
        {
            let path = path.to_owned();
            self.job = Some((
                page,
                self.worker
                    .start(move |cancel| botui_fixed::text::interactive_page(&path, page, cancel)),
            ));
        }
        self.trim_cache();
    }
    fn remap_units(&mut self, page: u32, new: &PageText) {
        let Some(Ok(old)) = self.cache.get(&page) else {
            return;
        };
        let map = |at: WordAt, end: bool| -> WordAt {
            if at.page != page {
                return at;
            }
            let Some(old) = old.units().get(at.index) else {
                return at;
            };
            let byte = if end {
                old.bytes.end.saturating_sub(1)
            } else {
                old.bytes.start
            };
            new.units()
                .iter()
                .position(|u| u.bytes.end > byte)
                .map_or(at, |index| WordAt { index, ..at })
        };
        let forward = self.start.zip(self.cursor).is_some_and(|(a, b)| a <= b);
        self.cursor = self
            .cursor
            .map(|at| map(at, self.start.is_some() && forward));
        self.start = self.start.map(|at| map(at, !forward));
        self.yank_origin = self.yank_origin.map(|at| map(at, false));
        self.previous = self.previous.map(|(a, b)| (map(a, a > b), map(b, a <= b)));
        if let Some((at, byte)) = &mut self.search_at
            && at.page == page
            && let Some(index) = new.units().iter().position(|u| u.bytes.end > *byte)
        {
            at.index = index;
        }
    }
    fn trim_cache(&mut self) {
        // Only the current viewport and a small neighbourhood remain resident.
        if self.cache.len() > 8 {
            let visible: Vec<_> = self.views.iter().map(|v| v.page).collect();
            self.cache.retain(|p, _| {
                visible.contains(p)
                    || self.start.is_some_and(|a| a.page == *p)
                    || self.cursor.is_some_and(|a| a.page == *p)
                    || self.search_at.is_some_and(|(a, _)| a.page == *p)
            });
        }
    }
    pub fn begin_frame(&mut self) {
        self.views.clear();
        self.buttons.clear();
    }
    pub fn page(&mut self, page: u32, screen: Rect, window: [f32; 4], turns: u8) {
        self.views.push(View {
            page,
            screen,
            window,
            turns,
        });
    }
    pub fn strip(
        &mut self,
        page: u32,
        picture: &Picture,
        screen: Rect,
        crop: Crop,
        slice: (u32, u32),
        turns: u8,
    ) {
        let (skip, rows) = slice;
        if screen.is_empty() || rows == 0 {
            return;
        }
        let (w, _) = picture.dimensions();
        let cw = (w * 100 / u32::from(crop.percent.max(100))).max(1);
        let x = (w - cw) * u32::from(crop.x) / 255;
        self.views.push(View {
            page,
            screen,
            window: [
                x as f32 / w as f32,
                skip as f32 / rows as f32,
                (x + cw) as f32 / w as f32,
                (skip + u32::from(screen.height)) as f32 / rows as f32,
            ],
            turns,
        });
    }
    fn word(&self, at: WordAt) -> Option<&botui_fixed::text::Word> {
        self.cache
            .get(&at.page)?
            .as_ref()
            .ok()?
            .units()
            .get(at.index)
    }
    fn hit(&self, pointer: Pointer, nearest: bool) -> Option<WordAt> {
        let p = pointer.cell;
        let view = self.views.iter().find(|v| v.screen.contains(p))?;
        let (x, y) = view.pointer(pointer)?;
        let text = self.cache.get(&view.page)?.as_ref().ok()?;
        text.units()
            .iter()
            .enumerate()
            .filter(|(_, w)| view.rect(w.bounds).is_some())
            .filter_map(|(index, w)| {
                let r = view.rect(w.bounds)?;
                if !nearest && !r.contains(p) {
                    return None;
                }
                let dx = (w.bounds[0] - x).max(0.).max(x - w.bounds[2]);
                let dy = (w.bounds[1] - y).max(0.).max(y - w.bounds[3]);
                Some((index, dy * 10. + dx))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| WordAt {
                page: view.page,
                index,
            })
    }
    fn ends(&self) -> Option<(WordAt, WordAt)> {
        let b = self.cursor?;
        let a = self.start.unwrap_or(b);
        Some((a.min(b), a.max(b)))
    }
    pub fn selection(&self) -> Option<(TextRange, String, Vec<PdfRegion>)> {
        let (a, b) = self.ends()?;
        let mut quote = String::new();
        let mut regions = Vec::new();
        for page in a.page..=b.page {
            let text = self.cache.get(&page)?.as_ref().ok()?;
            let first = if page == a.page { a.index } else { 0 };
            let last = if page == b.page {
                b.index
            } else {
                text.units().len().checked_sub(1)?
            };
            let words = text.units().get(first..=last)?;
            if !quote.is_empty() {
                quote.push_str("\n\n");
            }
            quote.push_str(&text.text[words.first()?.bytes.start..words.last()?.bytes.end]);
            let bytes = words.first()?.bytes.start..words.last()?.bytes.end;
            // Keep the established word rectangles for complete words. Only
            // partial words use their measured glyph boxes, so old saved marks
            // and BookOrbit geometry remain compatible.
            regions.extend(
                text.words
                    .iter()
                    .filter(|w| w.bytes.start < bytes.end && w.bytes.end > bytes.start)
                    .filter_map(|w| {
                        let bounds = if bytes.start <= w.bytes.start && bytes.end >= w.bytes.end {
                            w.bounds
                        } else {
                            words
                                .iter()
                                .filter(|c| {
                                    c.bytes.start < w.bytes.end && c.bytes.end > w.bytes.start
                                })
                                .map(|c| c.bounds)
                                .reduce(|a, b| {
                                    [
                                        a[0].min(b[0]),
                                        a[1].min(b[1]),
                                        a[2].max(b[2]),
                                        a[3].max(b[3]),
                                    ]
                                })?
                        };
                        Some(PdfRegion { page, bounds })
                    }),
            );
        }
        let range = TextRange::new(
            Anchor {
                block: a.page as usize - 1,
                byte: self.word(a)?.bytes.start,
            },
            Anchor {
                block: b.page as usize - 1,
                byte: self.word(b)?.bytes.end,
            },
        );
        Some((range, quote, regions))
    }
    pub fn active_note(&self) -> Option<&Note> {
        let (_, _, regions) = self.selection()?;
        if regions.is_empty() {
            return None;
        }
        self.notes
            .iter()
            .filter(|n| {
                regions.iter().all(|r| {
                    n.pdf_regions.iter().any(|saved| {
                        saved.page == r.page
                            && saved.bounds[0] <= r.bounds[0] + 0.0001
                            && saved.bounds[1] <= r.bounds[1] + 0.0001
                            && saved.bounds[2] + 0.0001 >= r.bounds[2]
                            && saved.bounds[3] + 0.0001 >= r.bounds[3]
                    })
                })
            })
            .min_by_key(|n| n.pdf_regions.len())
    }
    pub fn take_note_click(&mut self) -> bool {
        if self.pending.is_some() || !std::mem::take(&mut self.note_pending) {
            return false;
        }
        self.active_note().is_some()
    }
    pub fn clear(&mut self) {
        self.note_pending = false;
        self.context_pending = None;
        self.follow_link = false;
        self.enabled = false;
        self.search_cursor_pending = false;
        self.normal = false;
        self.keys.reset();
        self.cursor = None;
        self.start = None;
        self.pending = None;
        self.pending_end = None;
        self.auto_cursor = false;
        self.pending_visual = false;
        self.reveal = None;
        self.job = None;
        self.drag = false;
        self.message.clear();
    }
    fn escape(&mut self) {
        if self.keys.pending() {
            self.keys.reset();
        } else if self.pending_visual {
            self.pending_visual = false;
        } else if let Some(a) = self.start.take() {
            self.previous = self.cursor.map(|b| (a, b));
            self.message.clear();
        } else {
            self.clear();
        }
    }
    fn activate(&mut self) {
        self.enabled = true;
        self.normal = true;
        self.auto_cursor = true;
        if self.cursor.is_none() && !self.search_regions.is_empty() {
            self.cursor = self.search_at.map(|(at, _)| at);
            self.search_cursor_pending = self.cursor.is_some();
        }
        self.message.clear();
    }
    pub fn input(&mut self, e: &Event) -> Option<Action> {
        match e {
            Event::Mouse(m) => {
                let p = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::Down(MouseButton::Right) => {
                        self.note_pending = false;
                        if !self.views.iter().any(|v| v.screen.contains(p)) {
                            return None;
                        }
                        self.enabled = true;
                        self.keys.reset();
                        // Keep the original PDF point, including sub-cell mouse precision.
                        // Selecting a menu row later must not replace it with that row's position.
                        self.context_pending = Some(ContextPoint {
                            at: p,
                            source: self.source_point(Some(p), None),
                        });
                        if let Some(hit) = self.hit(p.into(), false) {
                            let inside = self
                                .start
                                .zip(self.cursor)
                                .is_some_and(|(a, b)| hit >= a.min(b) && hit <= a.max(b));
                            if !inside {
                                self.cursor = Some(hit);
                                self.select_word();
                            }
                        } else {
                            self.cursor = None;
                            self.start = None;
                            self.pending = Some(p.into());
                        }
                        Some(Action::None)
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if self.enabled && self.toolbar.advance(p) {
                            return Some(Action::None);
                        }
                        if let Some((_, a)) = self.buttons.iter().find(|(r, _)| r.contains(p)) {
                            return Some(*a);
                        }
                        if !self.views.iter().any(|v| v.screen.contains(p)) {
                            return None;
                        }
                        self.activate();
                        self.normal = false;
                        self.auto_cursor = false;
                        self.pending_visual = false;
                        self.pending = None;
                        self.pending_end = None;
                        self.drag = true;
                        self.dragged = false;
                        self.note_pending = false;
                        let count = self
                            .clicks
                            .filter(|(old, t, _)| {
                                old.cell == p && t.elapsed() < Duration::from_millis(450)
                            })
                            .map_or(1, |(_, _, n)| n % 3 + 1);
                        self.clicks = Some((p.into(), Instant::now(), count));
                        self.cursor = self.hit(p.into(), false);
                        self.start = self.cursor;
                        if let Some(at) = self.cursor {
                            if count == 3 {
                                self.select_block(at);
                            } else if count == 2 {
                                self.select_word();
                            }
                        } else {
                            self.pending = Some(p.into());
                        }
                        Some(Action::None)
                    }
                    MouseEventKind::Drag(MouseButton::Left) if self.drag => {
                        self.dragged = true;
                        if self.pending.is_some() {
                            self.pending_end = Some(p.into());
                        }
                        if let Some(at) = self.hit(p.into(), true) {
                            self.cursor = Some(at);
                        }
                        Some(Action::None)
                    }
                    MouseEventKind::Up(MouseButton::Left) if self.drag => {
                        self.drag = false;
                        if !self.dragged
                            && self.clicks.is_some_and(|(_, _, count)| count < 3)
                            && self.active_note().is_none()
                        {
                            self.select_word();
                        }
                        self.note_pending = !self.dragged
                            && self
                                .clicks
                                .is_some_and(|(at, _, count)| at.cell == p && count == 1);
                        Some(Action::None)
                    }
                    _ => None,
                }
            }
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                // Vim letters belong to the explicit cursor mode entered with Space c.
                if self.normal {
                    if k.code == KeyCode::Esc {
                        self.escape();
                        return Some(Action::None);
                    }
                    if k.code == KeyCode::Tab {
                        return Some(Action::Key(KeyCode::Char('.')));
                    }
                    if k.modifiers.contains(KeyModifiers::CONTROL) {
                        return match k.code {
                            KeyCode::Char('o') => Some(Action::Key(KeyCode::Char(','))),
                            KeyCode::Char('i') | KeyCode::Tab => {
                                Some(Action::Key(KeyCode::Char('.')))
                            }
                            KeyCode::Char('c') => Some(Action::Copy),
                            _ => None,
                        };
                    }
                    if k.code == KeyCode::Enter && !self.keys.expects_literal() {
                        self.follow_link = true;
                        return Some(Action::None);
                    }
                    if k.code == KeyCode::Char('y')
                        && self.start.is_some()
                        && !self.keys.expects_literal()
                    {
                        return Some(Action::Copy);
                    }
                    if let Some(edit) = self.keys.input(k.code, self.start.is_some()) {
                        return Some(self.vim(edit));
                    }
                    return match k.code {
                        KeyCode::F(1..=5) | KeyCode::PageUp | KeyCode::PageDown => None,
                        KeyCode::Char('K') => {
                            Some(Action::Lookup(crate::lookup::Purpose::Dictionary))
                        }
                        _ => Some(Action::None),
                    };
                }
                if !self.enabled {
                    return None;
                }
                match k.code {
                    KeyCode::Esc => {
                        if self.start.take().is_none() {
                            self.clear();
                        }
                        Some(Action::None)
                    }
                    KeyCode::Char('y' | 'Y') => Some(Action::Copy),
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        Some(Action::Copy)
                    }
                    KeyCode::Char('H') => Some(Action::Highlight),
                    KeyCode::Enter if self.cursor_link().is_some() => {
                        self.follow_link = true;
                        Some(Action::None)
                    }
                    KeyCode::Enter if self.active_note().is_some() => Some(Action::Comment),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    pub fn take_context_menu(&mut self) -> Option<ContextPoint> {
        if self.pending.is_none() {
            self.context_pending.take()
        } else {
            None
        }
    }
    pub fn take_link(&mut self) -> Option<botui_fixed::text::LinkTarget> {
        if self.pending.is_some() {
            return None;
        }
        let target = if std::mem::take(&mut self.follow_link) {
            self.cursor_link().cloned()
        } else if self.note_pending {
            let (p, _, _) = self.clicks?;
            let v = self.views.iter().find(|v| v.screen.contains(p.cell))?;
            let (x, y) = v.pointer(p)?;
            self.cache
                .get(&v.page)?
                .as_ref()
                .ok()?
                .links
                .iter()
                .find(|l| {
                    l.bounds[0] <= x && x <= l.bounds[2] && l.bounds[1] <= y && y <= l.bounds[3]
                })
                .map(|l| l.target.clone())
        } else {
            None
        };
        if target.is_some() {
            self.clear();
        }
        target
    }
    fn cursor_link(&self) -> Option<&botui_fixed::text::LinkTarget> {
        let at = self.cursor?;
        let b = self.word(at)?.bounds;
        self.cache
            .get(&at.page)?
            .as_ref()
            .ok()?
            .links
            .iter()
            .find(|l| {
                l.bounds[0] <= (b[0] + b[2]) / 2.
                    && (b[0] + b[2]) / 2. <= l.bounds[2]
                    && l.bounds[1] <= (b[1] + b[3]) / 2.
                    && (b[1] + b[3]) / 2. <= l.bounds[3]
            })
            .map(|l| &l.target)
    }
    fn select_word(&mut self) {
        let Some(at) = self.cursor else { return };
        let Some(Ok(page)) = self.cache.get(&at.page) else {
            return;
        };
        let Some(unit) = page.units().get(at.index) else {
            return;
        };
        let Some(word) = page
            .words
            .iter()
            .find(|w| w.bytes.contains(&unit.bytes.start))
        else {
            return;
        };
        let start = page
            .units()
            .partition_point(|c| c.bytes.end <= word.bytes.start);
        let end = page
            .units()
            .partition_point(|c| c.bytes.start < word.bytes.end)
            .saturating_sub(1);
        self.start = Some(WordAt { index: start, ..at });
        self.cursor = Some(WordAt { index: end, ..at });
    }
    pub fn leader_action(&mut self, c: crate::leader::Command) -> Option<Action> {
        use crate::leader::Command as C;
        match c {
            C::Dictionary => Some(Action::Lookup(crate::lookup::Purpose::Dictionary)),
            C::Translate => Some(Action::Lookup(crate::lookup::Purpose::Translation)),
            C::Key(KeyCode::F(7)) => {
                if self.normal {
                    self.clear();
                } else {
                    self.activate();
                }
                Some(Action::None)
            }
            C::Key(KeyCode::Char('Y')) => Some(Action::Copy),
            C::Key(KeyCode::Char('N')) => Some(Action::Comment),
            C::Key(KeyCode::Char('H')) => Some(Action::Highlight),
            _ => None,
        }
    }
    fn vim(&mut self, edit: crate::vim::Edit) -> Action {
        use crate::vim::{Edit as V, Motion as M};
        match edit {
            V::Wait => (),
            V::Cancel => {
                if let (Some(a), Some(b)) = (self.start.take(), self.cursor) {
                    self.previous = Some((a, b));
                }
            }
            V::Visual => {
                if self.cursor.is_none() {
                    self.pending_visual = !self.pending_visual;
                    return Action::None;
                }
                self.start = if self.start.is_some() {
                    None
                } else {
                    self.cursor
                }
            }
            V::VisualLine => {
                self.select_line();
            }
            V::Reselect => {
                if let Some((a, b)) = self.previous {
                    self.start = Some(a);
                    self.cursor = Some(b);
                }
            }
            V::Mark(c) => return Action::Mark(c),
            V::Jump(c) => return Action::Jump(c),
            V::Search(forward) => {
                self.search_forward = forward;
                return Action::Key(KeyCode::Char('/'));
            }
            V::Next(forward) => {
                return Action::Key(KeyCode::F(if forward == self.search_forward {
                    3
                } else {
                    4
                }));
            }
            V::Move(m, count) | V::Yank(m, count) => {
                let origin = self.cursor;
                let yank = matches!(edit, V::Yank(..));
                if self
                    .cursor
                    .and_then(|at| self.cache.get(&at.page))
                    .is_some_and(|t| t.as_ref().is_ok_and(|t| !t.characters.is_empty()))
                {
                    return self.precise_motion(m, count, yank);
                }
                if matches!(m, M::Find { .. } | M::Delimited { .. }) {
                    self.message = "Exact character selection requires Python Poppler GI".into();
                    return Action::None;
                }
                if yank {
                    self.yank_origin = origin;
                    self.start = self.cursor;
                    if matches!(m, M::Word | M::EndWord | M::InnerWord | M::AroundWord)
                        && let Some(at) = origin
                        && let Some(Ok(page)) = self.cache.get(&at.page)
                    {
                        self.cursor = Some(WordAt {
                            index: (at.index + count - 1).min(page.units().len().saturating_sub(1)),
                            ..at
                        });
                        return Action::Copy;
                    }
                }
                let mut object_start = None;
                for step in 0..count {
                    match m {
                        M::InnerWord | M::AroundWord | M::InnerBigWord | M::AroundBigWord => {
                            self.start = self.cursor;
                        }
                        M::Paragraph | M::Line => {
                            if step > 0 {
                                self.move_word(KeyCode::Char('w'));
                            }
                            if m == M::Line {
                                self.select_line();
                            } else if let Some(at) = self.cursor {
                                self.select_block(at);
                            }
                            object_start = object_start.or(self.start);
                            self.start = object_start;
                        }
                        M::Top | M::Bottom => {
                            if let Some(at) = self.cursor
                                && let Some(Ok(t)) = self.cache.get(&at.page)
                            {
                                self.cursor = Some(WordAt {
                                    index: if m == M::Top {
                                        0
                                    } else {
                                        t.units().len().saturating_sub(1)
                                    },
                                    ..at
                                });
                            }
                        }
                        _ => self.move_word(KeyCode::Char(match m {
                            M::Left | M::BackWord | M::BackBigWord => 'b',
                            M::NextParagraph => '}',
                            M::PreviousParagraph => '{',
                            M::Down | M::ScreenDown => 'j',
                            M::Up | M::ScreenUp => 'k',
                            M::Start | M::First | M::ScreenStart => '0',
                            M::End | M::ScreenEnd => '$',
                            _ => 'w',
                        })),
                    }
                }
                if yank {
                    if m == M::Word && self.cursor != origin {
                        self.move_word(KeyCode::Char('b'));
                    }
                    return Action::Copy;
                }
            }
        }
        Action::None
    }
    fn select_line(&mut self) {
        self.move_word(KeyCode::Char('0'));
        self.start = self.cursor;
        self.move_word(KeyCode::Char('$'));
    }
    fn select_block(&mut self, at: WordAt) {
        if let Some(Ok(text)) = self.cache.get(&at.page) {
            let block = text.units()[at.index].block;
            self.start = text
                .units()
                .iter()
                .position(|w| w.block == block)
                .map(|index| WordAt {
                    page: at.page,
                    index,
                });
            self.cursor = text
                .units()
                .iter()
                .rposition(|w| w.block == block)
                .map(|index| WordAt {
                    page: at.page,
                    index,
                });
        }
    }
    fn move_word(&mut self, key: KeyCode) {
        let Some(at) = self.cursor else {
            return;
        };
        let Some(Ok(text)) = self.cache.get(&at.page) else {
            return;
        };
        let w = &text.units()[at.index];
        let index = match key {
            KeyCode::Char('}') => text
                .units()
                .iter()
                .enumerate()
                .skip(at.index + 1)
                .find(|(_, v)| v.block != w.block)
                .map_or(at.index, |(i, _)| i),
            KeyCode::Char('{') => text.units()[..at.index]
                .iter()
                .rfind(|v| v.block != w.block)
                .and_then(|prev| text.units().iter().position(|v| v.block == prev.block))
                .unwrap_or(at.index),
            KeyCode::Char('b' | 'h') | KeyCode::Left => at.index.saturating_sub(1),
            KeyCode::Char('0') => text
                .units()
                .iter()
                .position(|v| v.line == w.line)
                .unwrap_or(at.index),
            KeyCode::Char('$') => text
                .units()
                .iter()
                .rposition(|v| v.line == w.line)
                .unwrap_or(at.index),
            KeyCode::Up | KeyCode::Down | KeyCode::Char('j' | 'k') => {
                let up = matches!(key, KeyCode::Up | KeyCode::Char('k'));
                let line = if up {
                    w.line.saturating_sub(1)
                } else {
                    w.line + 1
                };
                text.units()
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.line == line)
                    .min_by(|(_, a), (_, b)| {
                        (a.bounds[0] - w.bounds[0])
                            .abs()
                            .total_cmp(&(b.bounds[0] - w.bounds[0]).abs())
                    })
                    .map_or(at.index, |(i, _)| i)
            }
            _ => (at.index + 1).min(text.units().len() - 1),
        };
        let bounds = text.units()[index].bounds;
        if !self
            .views
            .iter()
            .any(|v| v.page == at.page && v.rect(bounds).is_some())
        {
            self.reveal = Some(PdfRegion {
                page: at.page,
                bounds,
            });
        }
        self.cursor = Some(WordAt {
            page: at.page,
            index,
        });
    }
    pub fn take_reveal(&mut self) -> Option<PdfRegion> {
        self.reveal.take()
    }
    pub fn paint(&mut self, f: &mut Frame, pages: &super::pages::Pages, picker: &Picker) {
        let selected = self.selection().map(|(_, _, r)| r).unwrap_or_default();
        let mut next = Vec::new();
        for view in &self.views {
            let Some(picture) = pages.get(view.page) else {
                continue;
            };
            let mut marks = Vec::new();
            for note in &self.notes {
                for region in note.pdf_regions.iter().filter(|r| r.page == view.page) {
                    let mut bounds = region.bounds;
                    if note.color.is_none() {
                        if note.body.trim().is_empty() {
                            continue;
                        }
                        // A comment without fill still has a visible, clickable passage.
                        bounds[1] = bounds[3] - (bounds[3] - bounds[1]) * 0.12;
                    }
                    marks.push((
                        rotate(bounds, view.turns),
                        note.color.map_or([100, 130, 170], |c| c.rgb()),
                    ));
                }
            }
            marks.extend(
                self.search_regions
                    .iter()
                    .filter(|r| r.page == view.page)
                    .map(|r| (rotate(r.bounds, view.turns), [250, 204, 21])),
            );
            if self.enabled {
                marks.extend(
                    selected
                        .iter()
                        .filter(|r| r.page == view.page)
                        .map(|r| (rotate(r.bounds, view.turns), [160, 185, 255])),
                );
            }
            marks.retain(|(r, _)| view.rotated_rect(*r).is_some());
            let Some(rect) = marks
                .iter()
                .filter_map(|(r, _)| view.rotated_rect(*r))
                .reduce(|a, b| a.union(b))
            else {
                continue;
            };
            let key = (picture.identity, *view, marks);
            let patch = if let Some(i) = self.patches.iter().position(|p| p.key == key) {
                self.patches.swap_remove(i)
            } else {
                Patch {
                    picture: picture.text_patch(
                        view.window_for(rect),
                        rect.as_size(),
                        picker,
                        &key.2,
                    ),
                    key,
                }
            };
            if picker.protocol_type() == ratatui_image::picker::ProtocolType::Sixel {
                let cache = self.sixel.get_or_insert_with(super::sixel::Cache::new);
                crate::overlay::paint(f, rect, |f| {
                    cache.draw_patch(f, &patch.picture, rect, picker)
                });
            } else {
                crate::overlay::paint(f, rect, |f| patch.picture.draw(f, rect, picker));
            }
            next.push(patch);
        }
        self.patches = next;
    }
    pub fn footer(&mut self, f: &mut Frame, area: Rect) -> bool {
        if !self.enabled || area.is_empty() {
            return false;
        }
        let label = if self.job.is_some() || (self.busy() && self.cursor.is_none()) {
            "Loading text layer…"
        } else if self.cursor.is_none() {
            "No text at this position"
        } else if self.start.is_some() {
            if self.normal { "VISUAL" } else { "SELECT" }
        } else if self.normal {
            "NORMAL"
        } else {
            "TEXT"
        };
        let mut actions = vec![
            (label, None),
            ("[Space a y Copy]", Some(Action::Copy)),
            ("[Space a h Highlight]", Some(Action::Highlight)),
            ("[Space a n Note]", Some(Action::Comment)),
            (
                if self.start.is_some() || self.keys.pending() || self.pending_visual {
                    "[Esc Clear]"
                } else {
                    "[Esc Read]"
                },
                Some(Action::Clear),
            ),
        ];
        if self.cursor_link().is_some() {
            actions.insert(1, ("[Enter Follow link]", Some(Action::Follow)));
        }
        self.toolbar.draw(
            f,
            area,
            Default::default(),
            (!self.message.is_empty()).then_some(self.message.as_str()),
            &actions,
            &mut self.buttons,
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_jump_ends_selection_without_losing_the_target_or_future_selection() {
        let mut text = Text::new(None);
        text.page(1, Rect::new(0, 0, 100, 100), [0., 0., 1., 1.], 0);
        let mut layer = page("Alpha bravo charlie");
        layer.interactive = true;
        text.cache.insert(1, Ok(layer));
        let click = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 35,
            row: 22,
            modifiers: KeyModifiers::NONE,
        });
        text.input(&click);
        assert!(text.selection().is_some());
        let target = PdfRegion {
            page: 1,
            bounds: [0.6, 0.5, 0.8, 0.6],
        };
        text.show_source(target.clone());
        text.poll(Path::new("/nonexistent-source-test.pdf"));
        assert!(!text.enabled);
        assert!(!text.drag);
        assert!(text.selection().is_none());
        assert_eq!(text.search_regions, vec![target]);
        let (_, point) = text.source_point(None, Some(1)).unwrap();
        assert!((point[0] - 0.7).abs() < 0.00001);
        assert!((point[1] - 0.55).abs() < 0.00001);
        text.input(&click);
        assert!(text.enabled);
        assert!(text.selection().is_some());
    }

    #[test]
    fn forward_target_is_available_before_the_first_raster_but_not_after_leaving_its_page() {
        let mut text = Text::new(None);
        text.show_source(PdfRegion {
            page: 2,
            bounds: [0.2, 0.3, 0.4, 0.5],
        });
        let (page, point) = text.source_point(None, Some(2)).unwrap();
        assert_eq!(page, 2);
        assert!((point[0] - 0.3).abs() < 0.00001);
        assert!((point[1] - 0.4).abs() < 0.00001);
        assert!(text.source_point(None, Some(3)).is_none());
        assert!(
            text.source_point(Some(Position::new(10, 10)), Some(2))
                .is_none()
        );
        // A stale raster from the previous page must not replace the known target.
        text.page(1, Rect::new(0, 0, 100, 100), [0., 0., 1., 1.], 0);
        assert_eq!(text.source_point(None, Some(2)).unwrap().0, 2);
        assert!(text.source_point(None, Some(3)).is_none());
    }
    use botui_fixed::text::Word;

    #[test]
    fn context_on_a_textless_page_keeps_its_original_rotated_point() {
        let mut text = Text::new(None);
        text.page(3, Rect::new(0, 0, 100, 100), [0., 0., 1., 1.], 1);
        text.cache.insert(3, Ok(PageText::default()));
        let event = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 20,
            row: 30,
            modifiers: KeyModifiers::NONE,
        });
        assert!(matches!(text.input(&event), Some(Action::None)));
        assert!(text.take_context_menu().is_none());
        text.poll(Path::new("/nonexistent-context-test.pdf"));
        assert!(text.selection().is_none());
        // A menu redraw or a later mouse event must not change the source point.
        text.begin_frame();
        let context = text.take_context_menu().unwrap();
        assert_eq!(context.at, Position::new(20, 30));
        let (page, [x, y]) = context.source.unwrap();
        assert_eq!(page, 3);
        assert!((x - 0.305).abs() < 0.00001);
        assert!((y - 0.795).abs() < 0.00001);
        assert!(text.take_context_menu().is_none());
    }

    #[test]
    fn context_preserves_a_selected_range_and_uses_the_clicked_source_point() {
        let mut text = Text::new(None);
        text.page(1, Rect::new(0, 0, 100, 100), [0., 0., 1., 1.], 0);
        text.cache.insert(1, Ok(page("Alpha bravo charlie")));
        text.start = Some(WordAt { page: 1, index: 0 });
        text.cursor = Some(WordAt { page: 1, index: 2 });
        text.input(&Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 35,
            row: 22,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(text.selection().unwrap().1, "Alpha bravo charlie");
        let context = text.take_context_menu().unwrap();
        let (page, [x, y]) = context.source.unwrap();
        assert_eq!(page, 1);
        assert!((x - 0.355).abs() < 0.00001);
        assert!((y - 0.225).abs() < 0.00001);
    }

    #[test]
    fn ordinary_selection_keeps_search_keys_and_leader_note_separate() {
        let mut text = Text::new(None);
        text.enabled = true;
        text.cache.insert(1, Ok(page("Alpha bravo charlie")));
        text.start = Some(WordAt { page: 1, index: 0 });
        text.cursor = Some(WordAt { page: 1, index: 1 });
        for c in ['n', 'N'] {
            let event = Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            ));
            assert!(text.input(&event).is_none());
            assert_eq!(text.selection().unwrap().1, "Alpha bravo");
            assert!(!text.normal);
        }
        assert!(matches!(
            text.leader_action(crate::leader::Command::Key(KeyCode::Char('N'))),
            Some(Action::Comment)
        ));
    }
    #[test]
    fn found_word_is_visible_without_enabling_text_mode_and_moves_the_vim_cursor() {
        let page = page("Alpha bravo charlie");
        let hit = botui_fixed::PageMatch {
            page: 3,
            bytes: 7..10,
            excerpt: "rav".into(),
        };
        let mut text = Text::new(None);
        let region = text.show_search(&hit, &page).unwrap();
        assert_eq!(region.bounds, page.units()[1].bounds);
        assert_eq!(region.page, 3);
        assert!(!text.enabled);
        assert!(text.cursor.is_none());
        text.activate();
        text.show_search(&hit, &page);
        assert_eq!(text.cursor, Some(WordAt { page: 3, index: 1 }));
        assert_eq!(text.search_origin(), Some((3, 7)));
        assert!(text.start.is_none());
        assert_eq!(text.selection().unwrap().1, "bravo");
        text.page(1, Rect::new(0, 0, 80, 24), [0., 0., 1., 1.], 0);
        text.poll(Path::new("/nonexistent-search-test.pdf"));
        assert_eq!(text.cursor, Some(WordAt { page: 3, index: 1 }));
        assert!(
            text.search_cursor_pending,
            "old raster must not steal the search cursor"
        );
        text.begin_frame();
        text.page(3, Rect::new(0, 0, 80, 24), [0., 0., 1., 1.], 0);
        text.poll(Path::new("/nonexistent-search-test.pdf"));
        assert!(!text.search_cursor_pending);
    }
    #[test]
    fn selection_crossing_saved_pdf_highlight_creates_a_new_annotation() {
        let mut text = Text::new(None);
        text.cache.insert(1, Ok(page("Alpha bravo charlie")));
        text.start = Some(WordAt { page: 1, index: 1 });
        text.cursor = Some(WordAt { page: 1, index: 2 });
        let (_, _, regions) = text.selection().unwrap();
        text.notes.push(
            serde_json::from_value(serde_json::json!({
                "id": "saved", "block": 0, "byte": 6, "end": [0, 19], "body": "", "updated": 0,
                "pdf_regions": regions
            }))
            .unwrap(),
        );
        text.start = text.cursor;
        assert_eq!(text.active_note().unwrap().id, "saved");
        text.start = Some(WordAt { page: 1, index: 0 });
        assert!(text.active_note().is_none());
    }
    #[test]
    fn reading_leaves_color_keys_to_pdf_but_normal_uses_visual() {
        let mut text = Text::new(None);
        let event = |c| {
            Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            ))
        };
        for c in ['v', 'V'] {
            assert!(text.input(&event(c)).is_none());
            assert!(!text.enabled);
        }
        text.activate();
        assert!(text.input(&event('v')).is_some());
        assert!(text.normal);
        assert!(text.pending_visual);
        assert!(
            text.leader_action(crate::leader::Command::Key(KeyCode::Char('V')))
                .is_none()
        );
    }
    fn page(value: &str) -> PageText {
        let mut offset = 0;
        let words = value
            .split(' ')
            .enumerate()
            .map(|(i, w)| {
                let start = offset;
                offset += w.len() + 1;
                Word {
                    bytes: start..start + w.len(),
                    bounds: [0.1 + i as f32 * 0.2, 0.2, 0.25 + i as f32 * 0.2, 0.25],
                    line: 0,
                    block: 0,
                }
            })
            .collect();
        PageText {
            size: [100., 100.],
            text: value.into(),
            words,
            interactive: true,
            ..Default::default()
        }
    }
    #[test]
    fn visual_can_exit_to_reading_with_escape_or_space_c() {
        let mut text = Text::new(None);
        text.cache.insert(1, Ok(page("Alpha bravo")));
        text.normal = true;
        text.enabled = true;
        text.cursor = Some(WordAt { page: 1, index: 0 });
        text.start = text.cursor;
        text.escape();
        assert!(text.normal);
        assert!(text.start.is_none());
        text.escape();
        assert!(!text.enabled);
        assert!(!text.normal);
        assert!(text.cursor.is_none());
        text.activate();
        text.pending_visual = true;
        text.leader_action(crate::leader::Command::Key(KeyCode::F(7)));
        assert!(!text.enabled);
        assert!(!text.pending_visual);
        assert!(!text.normal);
    }
    #[test]
    fn reverse_range_across_a_spread_copies_unicode_and_saves_physical_coordinates() {
        let mut text = Text::new(None);
        text.cache.insert(8, Ok(page("Начало второй страницы")));
        text.cache.insert(9, Ok(page("Another page here")));
        text.start = Some(WordAt { page: 9, index: 1 });
        text.cursor = Some(WordAt { page: 8, index: 1 });
        let (range, quote, regions) = text.selection().unwrap();
        assert_eq!(quote, "второй страницы\n\nAnother page");
        assert_eq!(
            range.start,
            Anchor {
                block: 7,
                byte: "Начало ".len()
            }
        );
        assert_eq!(
            range.end,
            Anchor {
                block: 8,
                byte: "Another page".len()
            }
        );
        assert_eq!(regions.len(), 4);
        assert_eq!(regions[0].page, 8);
        assert_eq!(regions[3].page, 9);
        text.views.push(View {
            page: 8,
            screen: Rect::new(10, 2, 100, 30),
            window: [0., 0., 1., 1.],
            turns: 0,
        });
        let hit = text.hit(Position::new(45, 8).into(), false).unwrap();
        assert_eq!(hit, WordAt { page: 8, index: 1 });
    }
    #[test]
    fn highlight_patch_has_separate_identity_and_preserves_page_geometry() {
        use ratatui::{Terminal, backend::TestBackend};
        // The same picture geometry is stable; changing zoom/rotation changes the key.
        let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
        let picker = Picker::halfblocks();
        let picture = Picture::new(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            300,
            400,
            image::Rgb([255, 255, 255]),
        )));
        let crop = Crop::default();
        let area = Rect::new(0, 0, 80, 30);
        let font = picker.font_size();
        let placement = crate::scan::layout::pages(
            area,
            ratatui::layout::Size::new(font.width, font.height),
            &[picture.dimensions()],
            0,
            crop,
            false,
        )[0]
        .unwrap();
        terminal
            .draw(|f| picture.draw_window(f, placement.screen, &picker, placement.window))
            .unwrap();
        assert_eq!(placement.window, [0., 0., 1., 1.]);
        assert!(!placement.screen.is_empty());
        let patch = picture.text_patch(
            [0., 0.1, 0.5, 0.2],
            ratatui::layout::Size::new(20, 3),
            &picker,
            &[([0., 0.1, 0.5, 0.2], [255, 200, 0])],
        );
        assert_ne!(picture.identity, patch.identity);
        assert_eq!(picture.dimensions(), (300, 400));
    }
    #[test]
    fn annotation_jump_reveals_the_same_region_after_rotation_and_zoom() {
        let region = PdfRegion {
            page: 7,
            bounds: [0.7, 0.8, 0.8, 0.85],
        };
        for turns in 0..4 {
            let crop = Crop {
                percent: 200,
                x: 0,
                y: 0,
            };
            let target = location(&region, crop, turns, false);
            let r = rotate(region.bounds, turns);
            let left = f32::from(target.crop.x) / 255. * 0.5;
            let top = f32::from(target.crop.y) / 255. * 0.5;
            assert!(left <= r[0] && left + 0.5 >= r[2]);
            assert!(top <= r[1] && top + 0.5 >= r[3]);
            let scrolling = location(&region, crop, turns, true);
            assert!((f32::from(scrolling.offset) / 65535. - (r[1] - 0.01)).abs() < 0.001);
            assert_eq!(scrolling.page, 7);
        }
    }
    #[test]
    fn keyboard_requests_reveal_when_the_next_word_is_beyond_the_crop() {
        let mut text = Text::new(None);
        text.cache.insert(1, Ok(page("Alpha bravo charlie")));
        text.cursor = Some(WordAt { page: 1, index: 0 });
        text.move_word(KeyCode::Right);
        assert_eq!(text.cursor, Some(WordAt { page: 1, index: 1 }));
        let region = text.take_reveal().unwrap();
        assert_eq!(region.page, 1);
        assert_eq!(
            region.bounds,
            text.word(text.cursor.unwrap()).unwrap().bounds
        );
    }
}
