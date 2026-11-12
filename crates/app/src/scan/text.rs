//! PDF text interaction is independent of page rendering and saved reading progress.
mod dialogs;
mod geometry;
use crate::{
    cover::{Crop, Picture},
    shelf::background::{Ticket, Worker},
};
use botui_core::{Anchor, TextRange};
use botui_fixed::text::PageText;
use botui_store::{HighlightColor, LocalState, Note, PdfRegion};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use geometry::View;
pub(super) use geometry::rotate;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
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
    None,
}
type ColoredBounds = ([f32; 4], [u8; 3]);
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
    clicks: Option<(Position, Instant, u8)>,
    pending: Option<Position>,
    pending_end: Option<Position>,
    auto_cursor: bool,
    pending_visual: bool,
    context_pending: Option<Position>,
    search_forward: bool,
    reveal: Option<PdfRegion>,
    worker: Worker,
    job: Option<(u32, Ticket<io::Result<PageText>>)>,
    cache: BTreeMap<u32, Result<PageText, String>>,
    views: Vec<View>,
    patches: Vec<Patch>,
    pub notes: Vec<Note>,
    pub message: String,
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
            search_forward: true,
            reveal: None,
            worker: Worker::new(),
            job: None,
            cache: BTreeMap::new(),
            views: vec![],
            patches: vec![],
            notes: store.and_then(|s| s.notes().ok()).unwrap_or_default(),
            message: String::new(),
            buttons: vec![],
        }
    }
    pub fn normal(&self) -> bool {
        self.normal
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn poll(&mut self, path: &Path) {
        if let Some((page, job)) = &self.job {
            match job.receiver.try_recv() {
                Ok(text) => {
                    self.cache.insert(*page, text.map_err(|e| e.to_string()));
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.cache
                        .insert(*page, Err("PDF text extraction interrupted".into()));
                    self.job = None;
                }
                Err(_) => (),
            }
        }
        if !self.enabled {
            return;
        }
        // Manual page turns/scrolling should not strand the keyboard cursor on
        // a word that is no longer on screen. An active range keeps its anchor.
        if self.start.is_none()
            && self.reveal.is_none()
            && self.cursor.is_some_and(|at| {
                self.cache
                    .get(&at.page)
                    .and_then(|t| t.as_ref().ok())
                    .and_then(|t| t.words.get(at.index))
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
            if let Some(view) = self.views.iter().find(|v| v.screen.contains(p)) {
                if let Some(result) = self.cache.get(&view.page) {
                    self.message = match result {
                        Ok(t) if t.words.is_empty() => {
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
                    .words
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
                        Ok(t) if t.words.is_empty() => {
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
            && let Some(page) = self
                .views
                .iter()
                .map(|v| v.page)
                .find(|p| !self.cache.contains_key(p))
        {
            let path = path.to_owned();
            self.job = Some((
                page,
                self.worker
                    .start(move |cancel| botui_fixed::text::page_text(&path, page, cancel)),
            ));
        }
        // Only the current viewport and a small neighbourhood remain resident.
        if self.cache.len() > 8 {
            let visible: Vec<_> = self.views.iter().map(|v| v.page).collect();
            self.cache.retain(|p, _| {
                visible.contains(p)
                    || self.start.is_some_and(|a| a.page == *p)
                    || self.cursor.is_some_and(|a| a.page == *p)
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
        self.cache.get(&at.page)?.as_ref().ok()?.words.get(at.index)
    }
    fn hit(&self, p: Position, nearest: bool) -> Option<WordAt> {
        let view = self.views.iter().find(|v| v.screen.contains(p))?;
        let (x, y) = view.point(p)?;
        let text = self.cache.get(&view.page)?.as_ref().ok()?;
        text.words
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
                text.words.len().checked_sub(1)?
            };
            let words = text.words.get(first..=last)?;
            if !quote.is_empty() {
                quote.push_str("\n\n");
            }
            quote.push_str(&text.text[words.first()?.bytes.start..words.last()?.bytes.end]);
            regions.extend(words.iter().map(|w| PdfRegion {
                page,
                bounds: w.bounds,
            }));
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
        self.notes
            .iter()
            .filter(|n| {
                regions.iter().all(|r| {
                    n.pdf_regions
                        .iter()
                        .any(|saved| saved.page == r.page && saved.bounds == r.bounds)
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
        self.enabled = false;
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
                        self.context_pending = Some(p);
                        if let Some(hit) = self.hit(p, false) {
                            let inside = self
                                .start
                                .zip(self.cursor)
                                .is_some_and(|(a, b)| hit >= a.min(b) && hit <= a.max(b));
                            if !inside {
                                self.cursor = Some(hit);
                                self.start = Some(hit);
                            }
                        } else {
                            self.cursor = None;
                            self.start = None;
                            self.pending = Some(p);
                        }
                        Some(Action::None)
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
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
                                *old == p && t.elapsed() < Duration::from_millis(450)
                            })
                            .map_or(1, |(_, _, n)| n % 3 + 1);
                        self.clicks = Some((p, Instant::now(), count));
                        self.cursor = self.hit(p, false);
                        self.start = self.cursor;
                        if let Some(at) = self.cursor {
                            if count == 3 {
                                self.select_block(at);
                            }
                        } else {
                            self.pending = Some(p);
                        }
                        Some(Action::None)
                    }
                    MouseEventKind::Drag(MouseButton::Left) if self.drag => {
                        self.dragged = true;
                        if self.pending.is_some() {
                            self.pending_end = Some(p);
                        }
                        if let Some(at) = self.hit(p, true) {
                            self.cursor = Some(at);
                        }
                        Some(Action::None)
                    }
                    MouseEventKind::Up(MouseButton::Left) if self.drag => {
                        self.drag = false;
                        self.note_pending = !self.dragged
                            && self
                                .clicks
                                .is_some_and(|(at, _, count)| at == p && count == 1);
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
                    if k.code == KeyCode::Char('y') && self.start.is_some() {
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
                    KeyCode::Char('n' | 'N') => Some(Action::Comment),
                    KeyCode::Char('H') => Some(Action::Highlight),
                    KeyCode::Enter if self.active_note().is_some() => Some(Action::Comment),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    pub fn take_context_menu(&mut self) -> Option<Position> {
        if self.pending.is_none() {
            self.context_pending
                .take()
                .filter(|_| self.cursor.is_some())
        } else {
            None
        }
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
                if yank {
                    self.yank_origin = origin;
                    self.start = self.cursor;
                    if matches!(m, M::Word | M::EndWord | M::InnerWord | M::AroundWord)
                        && let Some(at) = origin
                        && let Some(Ok(page)) = self.cache.get(&at.page)
                    {
                        self.cursor = Some(WordAt {
                            index: (at.index + count - 1).min(page.words.len().saturating_sub(1)),
                            ..at
                        });
                        return Action::Copy;
                    }
                }
                let mut object_start = None;
                for step in 0..count {
                    match m {
                        M::InnerWord | M::AroundWord => {
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
                                        t.words.len().saturating_sub(1)
                                    },
                                    ..at
                                });
                            }
                        }
                        _ => self.move_word(KeyCode::Char(match m {
                            M::Left | M::BackWord => 'b',
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
            let block = text.words[at.index].block;
            self.start = text
                .words
                .iter()
                .position(|w| w.block == block)
                .map(|index| WordAt {
                    page: at.page,
                    index,
                });
            self.cursor = text
                .words
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
        let w = &text.words[at.index];
        let index = match key {
            KeyCode::Char('}') => text
                .words
                .iter()
                .enumerate()
                .skip(at.index + 1)
                .find(|(_, v)| v.block != w.block)
                .map_or(at.index, |(i, _)| i),
            KeyCode::Char('{') => text.words[..at.index]
                .iter()
                .rfind(|v| v.block != w.block)
                .and_then(|prev| text.words.iter().position(|v| v.block == prev.block))
                .unwrap_or(at.index),
            KeyCode::Char('b' | 'h') | KeyCode::Left => at.index.saturating_sub(1),
            KeyCode::Char('0') => text
                .words
                .iter()
                .position(|v| v.line == w.line)
                .unwrap_or(at.index),
            KeyCode::Char('$') => text
                .words
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
                text.words
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
            _ => (at.index + 1).min(text.words.len() - 1),
        };
        let bounds = text.words[index].bounds;
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
            for note in self.notes.iter().filter(|n| n.color.is_some()) {
                for region in note.pdf_regions.iter().filter(|r| r.page == view.page) {
                    marks.push((rotate(region.bounds, view.turns), color(note.color)));
                }
            }
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
            crate::overlay::paint(f, rect, |f| patch.picture.draw(f, rect, picker));
            next.push(patch);
        }
        self.patches = next;
    }
    pub fn footer(&mut self, f: &mut Frame, area: Rect) {
        if !self.enabled || area.is_empty() {
            return;
        }
        let label = if self.busy() && self.cursor.is_none() {
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
        let actions = [
            ("[Space a y Copy]", Action::Copy),
            ("[Space a h Highlight]", Action::Highlight),
            ("[Space a n Note]", Action::Comment),
            (
                if self.start.is_some() || self.keys.pending() || self.pending_visual {
                    "[Esc Clear]"
                } else {
                    "[Esc Read]"
                },
                Action::Clear,
            ),
        ];
        let (rects, h) = crate::reader::controls::layout(area, &actions.map(|v| v.0));
        let panel = Rect::new(
            area.x,
            area.bottom().saturating_sub(h + 1),
            area.width,
            (h + 1).min(area.height),
        );
        crate::overlay::paint(f, panel, |f| {
            f.render_widget(ratatui::widgets::Clear, panel);
            f.render_widget(
                Paragraph::new(if self.message.is_empty() {
                    label
                } else {
                    &self.message
                })
                .dim(),
                Rect::new(panel.x, panel.y, panel.width, 1),
            );
            for ((label, a), r) in actions.into_iter().zip(rects) {
                f.render_widget(Paragraph::new(label), r);
                self.buttons.push((r, a));
            }
        });
    }
}
fn color(c: Option<HighlightColor>) -> [u8; 3] {
    match c {
        Some(HighlightColor::Green) => [130, 220, 140],
        Some(HighlightColor::Blue) => [130, 180, 245],
        Some(HighlightColor::Pink) => [245, 150, 180],
        Some(HighlightColor::Purple) => [195, 150, 240],
        _ => [245, 210, 100],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use botui_fixed::text::Word;
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
            text: value.into(),
            words,
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
        let hit = text.hit(Position::new(45, 8), false).unwrap();
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
