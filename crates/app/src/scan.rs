//! A fixed page is addressed by its printed page index, never by a text offset.
mod annotations;
mod contents;
mod controls;
mod flow;
mod history;
mod menu;
mod pages;
mod paper;
mod search;
use crate::{
    AppTerminal, centered,
    cover::{Crop, graphics},
    shelf::background::Worker,
};
use botui_core::Anchor;
use botui_formats::FileKind;
use botui_store::{LocalState, Saved};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use history::Location;
use ratatui::{
    layout::{Position, Rect},
    style::Stylize,
    widgets::{Clear, Paragraph},
};
use std::{io, path::Path, sync::mpsc::TryRecvError, time::Duration};

pub fn read(
    term: &mut AppTerminal,
    path: &Path,
    kind: FileKind,
    notices: &mut Vec<String>,
) -> io::Result<()> {
    let (_, percent) = read_server(term, path, kind, notices, None)?;
    if let Some(p) = percent
        && let Err(e) = botui_store::HistoryEntry::record(path, p)
    {
        notices.push(e.to_string());
    }
    Ok(())
}
pub fn read_server(
    term: &mut AppTerminal,
    path: &Path,
    kind: FileKind,
    notices: &mut Vec<String>,
    base: Option<&botui_bookorbit::progress::Progress>,
) -> io::Result<(Option<botui_bookorbit::progress::Move>, Option<f64>)> {
    read_pages(term, path, kind, notices, base, None, None)
}

pub(super) fn read_pages(
    term: &mut AppTerminal,
    path: &Path,
    kind: FileKind,
    notices: &mut Vec<String>,
    base: Option<&botui_bookorbit::progress::Progress>,
    album: Option<botui_core::Document>,
    mut live: Option<&mut crate::live_progress::Live>,
) -> io::Result<(Option<botui_bookorbit::progress::Move>, Option<f64>)> {
    let _uploads = crate::cover::ScanGraphics;
    let worker = Worker::new();
    let p = path.to_owned();
    let count_job = album
        .is_none()
        .then(|| worker.start(move |cancel| botui_fixed::page_count(&p, kind, cancel)));
    let prefs = crate::settings::Settings::load()?;
    let mut zen = prefs.zen;
    let mut zen_progress = prefs.zen_progress;
    let mut sidebar = contents::Contents::default();
    let mut splitter =
        crate::splitter::Splitter::new(crate::splitter::Pane::Contents, prefs.contents_width);
    let outline_path = path.to_owned();
    let mut outline_job = album
        .is_none()
        .then(|| worker.start(move |cancel| botui_fixed::outline(&outline_path, kind, cancel)));
    let plates = album.map(|doc| {
        let mut depths = Vec::new();
        for e in &doc.contents {
            depths.push(e.parent.map_or(0, |p| depths[p] + 1));
        }
        sidebar.set_outline(
            doc.contents
                .into_iter()
                .enumerate()
                .filter_map(|(i, entry)| {
                    Some(botui_fixed::OutlineEntry {
                        page: entry.anchor()?.block as u32 + 1,
                        title: entry.title,
                        depth: depths[i],
                    })
                })
                .collect(),
        );
        std::sync::Arc::<[String]>::from(doc.image_pages)
    });
    let mut total = plates.as_ref().map(|p| p.len() as u32);
    let mut crop = Crop {
        percent: prefs.scan_zoom.unwrap_or(100).clamp(25, 400),
        ..Crop::default()
    };
    let initial_zoom = crop.percent;
    let mut dialog: Option<controls::Dialog> = None;
    let store = LocalState::for_file(path)
        .map_err(|e| notices.push(e.to_string()))
        .ok();
    let saved = store.as_ref().and_then(|s| {
        s.load()
            .map_err(|e| notices.push(e.to_string()))
            .ok()
            .flatten()
    });
    let mut n = saved.as_ref().map_or(1, |s| {
        u32::try_from(s.anchor.block).unwrap_or(0).saturating_add(1)
    });
    if let Some(page) = base.and_then(|p| p.text.page) {
        n = page.max(1);
    }
    if base.is_some_and(|p| p.percentage == 0. && p.text.cfi.is_none() && p.text.page.is_none()) {
        n = 1;
    }
    if let Some(count) = total {
        n = n.clamp(1, count);
    }
    let opened = std::time::Instant::now();
    let start = n;
    let mut spread = saved
        .as_ref()
        .map_or(prefs.reading_layout.as_deref() == Some("spread"), |s| {
            s.spread
        });
    let mut scrolling = saved
        .as_ref()
        .map_or(prefs.reading_layout.as_deref() == Some("scroll"), |s| {
            s.scrolling
        });
    let mut offset = saved
        .as_ref()
        .filter(|s| s.anchor.block + 1 == n as usize)
        .map_or(0, |s| s.scan_offset);
    let fresh =
        base.is_some_and(|p| p.percentage == 0. && p.text.page.is_none() && p.text.cfi.is_none());
    if fresh {
        offset = 0;
    }
    let mut finished = !fresh && saved.as_ref().is_some_and(|s| s.finished);
    let initial_finished = finished;
    let initial_offset = offset;
    let mut observed = (n, offset, finished);
    let mut checkpoint = saved.unwrap_or_else(|| Saved {
        theme: "terminal".into(),
        ..Saved::default()
    });
    let mut pages = pages::Pages::new(checkpoint.scan_appearance, || match &plates {
        Some(members) => botui_fixed::PageReader::epub(path, members.clone()),
        None => botui_fixed::PageReader::new(path, kind),
    });
    let mut last_save = opened;
    let mut save_failed = false;
    let mut end_visible;
    let mut finish_on_arrival = None;
    let mut search = search::Search::default();
    let mut history = history::History::default();
    let mut viewer = crate::cover_viewer::Viewer::default();
    let mut last_sheet: Option<(u32, u16, bool)> = None;
    let mut message = String::new();
    let mut menu = false;
    let mut focus: usize = 0;
    let mut dirty = false;

    loop {
        if let Some(msg) = viewer.collect() {
            message = msg;
        }
        if total.is_none()
            && let Some(count_job) = &count_job
        {
            match count_job.receiver.try_recv() {
                Ok(result) => {
                    let count = result?;
                    n = n.clamp(1, count);
                    total = Some(count);
                }
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Disconnected) => {
                    return Err(io::Error::other("Page count interrupted"));
                }
            }
        }
        if let Some(job) = &outline_job {
            match job.receiver.try_recv() {
                Ok(result) => {
                    sidebar.set_outline(result.unwrap_or_default());
                    outline_job = None;
                }
                Err(TryRecvError::Disconnected) => outline_job = None,
                Err(TryRecvError::Empty) => (),
            }
        }
        if let Some(result) = search.poll(n) {
            match result {
                Ok(page) => {
                    if let Some(page) = page {
                        let previous = Location {
                            page: n,
                            offset,
                            crop,
                        };
                        n = page;
                        offset = 0;
                        crop.x = 128;
                        crop.y = 0;
                        history.record(
                            previous,
                            Location {
                                page: n,
                                offset,
                                crop,
                            },
                        );
                        dirty = true;
                    }
                    message = search.status();
                }
                Err(e) => message = e.to_string(),
            }
        }
        let screen = term.size()?;
        let show_progress = zen && zen_progress && screen.height > 1;
        let (sidebar_area, body) = splitter.layout(
            Rect::new(0, 0, screen.width, screen.height),
            !zen && screen.width >= 78,
        );
        let available = body.width;
        let cols = if !scrolling && spread && total != Some(1) && available >= 68 {
            2u16
        } else {
            1
        };
        let mut picker = graphics();
        let size = picker.font_size();
        let side = (available / cols)
            .saturating_mul(size.width)
            .max(screen.height.saturating_mul(size.height))
            .clamp(240, 1600);
        let side = (u32::from(side) * u32::from(crop.percent.max(100)) / 100).min(1920) as u16;
        let appearance = checkpoint.scan_appearance;
        let paper = appearance
            .terminal_paper
            .then(|| paper::Paper::for_reader(picker.protocol_type()));
        // A scaled page may end partway through a terminal cell. The block
        // renderer drops alpha there, so pad that fraction with the same paper.
        picker.set_background_color(paper.and_then(|p| p.cell_padding(picker.protocol_type())));
        pages.configure(appearance, paper);
        pages.poll();
        let view = flow::Viewport {
            side,
            width: crop.fit(body, false).width,
            height: screen
                .height
                .saturating_sub(u16::from(!zen || show_progress)),
            picker: &picker,
            crop,
            count: total.unwrap_or(0),
        };
        if let Some(count) = total {
            // Smaller sheets (especially landscape slides) can fit more than a spread.
            // Keep every visible slice, then prefetch two neighbours off screen.
            let visible = if scrolling {
                view.strip(&pages).parts(n, offset).count().max(1) as u16
            } else {
                cols
            };
            pages.update(
                n,
                count,
                visible,
                side,
                scrolling.then_some((&picker, view.width, crop)),
            );
        }
        let strip = view.strip(&pages);
        let (shown, shown_offset) = if pages.get(n).is_some() {
            last_sheet = Some((n, offset, scrolling));
            (n, offset)
        } else if pages.error(n, side).is_none()
            && let Some((p, o, flow)) = last_sheet
            && flow == scrolling
            && pages.get(p).is_some()
        {
            (p, o)
        } else {
            (n, offset)
        };
        end_visible = if scrolling {
            strip.end(n, offset)
        } else {
            total.is_some_and(|t| {
                n.saturating_add(u32::from(cols)) > t && (n..=t).all(|p| pages.get(p).is_some())
            })
        };
        if finish_on_arrival == Some(n) && end_visible {
            finished = true;
            dirty = true;
            finish_on_arrival = None;
        }
        if let Some(sync) = live.as_mut()
            && let Some(count) = total
            && base.is_some_and(|p| p.text.cfi.is_none())
        {
            if observed != (n, offset, finished) {
                observed = (n, offset, finished);
                sync.moved(Some(botui_bookorbit::progress::Move {
                    target: botui_bookorbit::progress::Target::Page(n),
                    percentage: scan_percent(n, offset, count, scrolling, finished && end_visible),
                }));
            }
            sync.tick(term)?;
            if sync.reopen {
                break;
            }
        }
        checkpoint.anchor = Anchor {
            block: n.saturating_sub(1) as usize,
            byte: 0,
        };
        checkpoint.spread = spread;
        checkpoint.scrolling = scrolling;
        checkpoint.scan_offset = offset;
        checkpoint.finished = finished;
        let interval = Duration::from_millis(600);
        if dirty && last_save.elapsed() >= interval {
            if let Some(store) = &store {
                match store.save_position(&checkpoint) {
                    Ok(()) => save_failed = false,
                    Err(e) => {
                        save_failed = true;
                        message = format!("Could not save reading position: {e}");
                    }
                }
            }
            dirty = false;
            last_save = std::time::Instant::now();
        }
        let mut hits = vec![];
        let mut dialog_hits = Vec::new();
        let mut width_button = Rect::default();
        let mut go_button = Rect::default();
        let mut footer_buttons = crate::preferences::Buttons::default();
        term.draw(|f| {
            let screen = f.area();
            sidebar.draw(f, sidebar_area, shown, total.unwrap_or(0));
            let area = Rect {
                height: body.height.saturating_sub(u16::from(!zen || show_progress)),
                ..body
            };
            if !zen && screen.height > 0 {
                let y = screen.bottom() - 1;
                let mut right = area.right();
                if !menu && area.width >= 45 && screen.height > 1 {
                    right -= 23;
                    footer_buttons.draw(f, Rect::new(right, y, 23, 1));
                }
                if !menu && area.width >= 74 && screen.height > 1 {
                    let label = "[P Page…]";
                    let w = label.chars().count() as u16;
                    right -= w + 1;
                    go_button = Rect::new(right, y, w, 1);
                    f.render_widget(Paragraph::new(label).cyan(), go_button);
                }
                if !menu && area.width >= 56 {
                    let label = format!("[W {}%]", crop.percent);
                    let w = label.len() as u16;
                    right -= w + 1;
                    width_button = Rect::new(right, y, w, 1);
                    f.render_widget(Paragraph::new(label).cyan(), width_button);
                }
                let space = right.saturating_sub(area.x);
                let mut status =
                    format!(" {}   ·   {} / {}", kind.label(), shown, total.unwrap_or(0));
                if shown != n {
                    status.push_str(&format!(" → {n}"));
                }
                if scrolling {
                    status.push_str(" · Scroll");
                    if crop.percent == 100 {
                        status.push_str(" · Fit width");
                    } else {
                        status.push_str(&format!(" · {}%", crop.percent));
                    }
                }
                let hints = "   ·   Z Zen   M Menu";
                if status.chars().count() + hints.chars().count() <= usize::from(space) {
                    status.push_str(hints);
                }
                f.render_widget(Paragraph::new(status).dim(), Rect::new(area.x, y, space, 1));
            }
            if show_progress && let Some(count) = total.filter(|n| *n > 0) {
                crate::appearance::zen_progress(
                    f,
                    Some((shown as usize, count as usize)),
                    scan_percent(
                        shown,
                        shown_offset,
                        count,
                        scrolling,
                        finished && end_visible && shown == n,
                    ),
                    Default::default(),
                );
            }
            if scrolling {
                strip.draw(f, area, shown, shown_offset);
            } else {
                for i in 0..cols {
                    let page_number = shown.saturating_add(u32::from(i));
                    let w = if cols == 2 {
                        area.width.saturating_sub(2) / 2
                    } else {
                        area.width
                    };
                    let slot =
                        Rect::new(area.x + (w + 2).saturating_mul(i), area.y, w, area.height);
                    if let Some(page) = pages.get(page_number) {
                        page.draw_region(f, crop.fit(slot, true), &picker, crop);
                    } else if let Some(error) = pages.error(page_number, side) {
                        pages::draw_error(f, slot, page_number, error);
                    }
                }
            }
            if let Some(paper) = paper
                && paper.cell_background
            {
                paper.clear_cells(f, area);
            }
            if pages.get(shown).is_none()
                && pages.error(shown, side).is_none()
                && opened.elapsed() >= Duration::from_millis(150)
            {
                f.render_widget(
                    Paragraph::new("Rendering page…").centered(),
                    centered(area, 36, 1),
                );
            }
            if !message.is_empty() && area.height > 0 {
                f.render_widget(
                    Paragraph::new(message.as_str()).reversed(),
                    Rect::new(area.x, area.bottom() - 1, area.width, 1),
                );
            }
            if menu {
                let panel = centered(area, 48, 20);
                crate::overlay::paint(f, panel, |f| {
                    let block = crate::appearance::block().title(format!(
                        " {} · {} / {} ",
                        kind.label(),
                        n,
                        total.unwrap_or(0)
                    ));
                    let inner = block.inner(panel);
                    f.render_widget(Clear, panel);
                    f.render_widget(block, panel);
                    let offset = focus
                        .saturating_add(1)
                        .saturating_sub(usize::from(inner.height));
                    for (i, (label, shortcut, action)) in menu::ITEMS
                        .iter()
                        .skip(offset)
                        .enumerate()
                        .take(inner.height as usize)
                    {
                        let r = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
                        let style = if i + offset == focus {
                            ratatui::style::Style::new().reversed()
                        } else {
                            Default::default()
                        };
                        let key_width = r.width.min(10);
                        f.render_widget(
                            Paragraph::new(*label).style(style),
                            Rect::new(r.x, r.y, r.width - key_width, 1),
                        );
                        f.render_widget(
                            Paragraph::new(*shortcut).right_aligned().style(style),
                            Rect::new(r.right() - key_width, r.y, key_width, 1),
                        );
                        hits.push((r, *action));
                    }
                });
            }
            if let Some(d) = &dialog {
                d.draw(
                    f,
                    area,
                    total.unwrap_or(0),
                    crop.percent,
                    scrolling,
                    &mut dialog_hits,
                );
            }
            crate::settings::paint_interface(f);
        })?;
        let busy = viewer.busy()
            || search.busy()
            || outline_job.is_some()
            || total.is_none()
            || pages.busy();
        let timeout = if busy {
            Some(Duration::from_millis(20))
        } else if dirty {
            Some(interval.saturating_sub(last_save.elapsed()))
        } else {
            None
        };
        let timeout = timeout
            .into_iter()
            .chain(
                live.as_ref()
                    .filter(|_| total.is_some() && base.is_some_and(|p| p.text.cfi.is_none()))
                    .map(|s| s.timeout()),
            )
            .min();
        let event = crate::settings::read_scan_event(timeout, appearance.terminal_paper)?;
        let Some(e) = event else {
            continue;
        };
        if matches!(e, Event::FocusGained)
            && let Some(s) = live.as_mut()
        {
            s.refresh();
            continue;
        }
        let previous = Location {
            page: n,
            offset,
            crop,
        };
        if let Some(d) = &mut dialog {
            match d.input(&e, &dialog_hits, total.unwrap_or(0)) {
                controls::Command::None => (),
                controls::Command::Close => dialog = None,
                controls::Command::Find => {
                    if let Some(controls::Dialog::Search(input)) = dialog.take() {
                        let text = input.finish(false);
                        if !text.is_empty() {
                            search.start(&worker, path, kind, text);
                            message = "Searching…".into();
                        }
                    }
                }
                controls::Command::Fit => crop = Crop::default(),
                controls::Command::Zoom(more) => crop.step(more),
                controls::Command::Go(page) => {
                    n = page;
                    offset = 0;
                    crop.x = 128;
                    crop.y = 0;
                    history.record(
                        previous,
                        Location {
                            page: n,
                            offset,
                            crop,
                        },
                    );
                    finish_on_arrival = None;
                    dirty |= n != previous.page || previous.offset != 0;
                    dialog = None;
                }
            }
            continue;
        }
        if !menu && let Some(result) = splitter.input(&e) {
            if let Err(error) = result {
                message = error.to_string();
            }
            continue;
        }
        if !menu && let Some(target) = sidebar.input(&e, total.unwrap_or(0)) {
            if let Some(target) = target
                && let Some(count) = total
            {
                n = target.clamp(1, count);
                offset = 0;
                crop.x = 128;
                crop.y = 0;
                history.record(
                    previous,
                    Location {
                        page: n,
                        offset,
                        crop,
                    },
                );
                dirty = true;
            }
            continue;
        }
        let action = match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => 5,
                KeyCode::Up | KeyCode::BackTab if menu => {
                    focus = focus.saturating_sub(1);
                    continue;
                }
                KeyCode::Down | KeyCode::Tab if menu => {
                    focus = (focus + 1).min(menu::ITEMS.len() - 1);
                    continue;
                }
                KeyCode::Char('c' | 'C') => 44,
                KeyCode::Enter if menu => menu::ITEMS[focus].2,
                KeyCode::Char('u' | 'U') => 25,
                KeyCode::Char('b' | 'B') => 18,
                KeyCode::Char('g') if k.modifiers.contains(KeyModifiers::CONTROL) => 24,
                KeyCode::Char('g' | 'G') => 19,
                KeyCode::Char('i' | 'I') => 20,
                KeyCode::Char('a' | 'A') => 21,
                KeyCode::Char('e' | 'E') => 22,
                KeyCode::Char('s' | 'S') => 23,
                KeyCode::Char('p' | 'P' | 'o' | 'O') => 24,
                KeyCode::Char('/') => 16,
                KeyCode::Char('[') | KeyCode::F(4) => 37,
                KeyCode::Char('n' | 'N' | ']') | KeyCode::F(3) => 17,
                KeyCode::F(5) => 38,
                KeyCode::Char(',') => 35,
                KeyCode::Char('.') => 36,
                KeyCode::Char('%') => 45,
                KeyCode::Char('z' | 'Z') => 14,
                KeyCode::Char('f' | 'F') => 15,
                KeyCode::Char('q' | 'Q') => 5,
                KeyCode::Esc if !menu => 5,
                KeyCode::F(2) => 26,
                KeyCode::Char('?') | KeyCode::F(1) => 27,
                KeyCode::Char('m' | 'M') | KeyCode::Esc | KeyCode::Enter => 0,
                KeyCode::Char('+' | '=') => 8,
                KeyCode::Char('-' | '−') => 9,
                KeyCode::Char('w' | 'W') => 39,
                KeyCode::Char('0') => 40,
                KeyCode::Char('r') => 41,
                KeyCode::Char('R') => 42,
                KeyCode::Char('v' | 'V') => 43,
                KeyCode::Down | KeyCode::Char('j') if scrolling => 30,
                KeyCode::Up | KeyCode::Char('k') if scrolling => 29,
                KeyCode::PageUp | KeyCode::Left if scrolling && crop.percent <= 100 => 31,
                KeyCode::PageDown | KeyCode::Right | KeyCode::Char(' ')
                    if scrolling && crop.percent <= 100 =>
                {
                    32
                }
                KeyCode::PageUp if scrolling => 31,
                KeyCode::PageDown if scrolling => 32,
                KeyCode::Left if crop.percent > 100 => 10,
                KeyCode::Right if crop.percent > 100 => 11,
                KeyCode::Up if crop.percent > 100 => 12,
                KeyCode::Down if crop.percent > 100 => 13,
                KeyCode::Left | KeyCode::PageUp | KeyCode::Up | KeyCode::Char('h' | 'k') => 1,
                KeyCode::Right
                | KeyCode::PageDown
                | KeyCode::Down
                | KeyCode::Char('l' | 'j' | ' ') => 2,
                KeyCode::Char('3') => 28,
                KeyCode::Char('1') => 3,
                KeyCode::Char('2') => 4,
                KeyCode::Home => 6,
                KeyCode::End => 7,
                _ => continue,
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp if menu => {
                    focus = focus.saturating_sub(1);
                    continue;
                }
                MouseEventKind::ScrollDown if menu => {
                    focus = (focus + 1).min(menu::ITEMS.len() - 1);
                    continue;
                }
                MouseEventKind::ScrollUp if scrolling => 33,
                MouseEventKind::ScrollDown if scrolling => 34,
                MouseEventKind::ScrollUp if crop.percent > 100 => 12,
                MouseEventKind::ScrollDown if crop.percent > 100 => 13,
                MouseEventKind::ScrollUp => 1,
                MouseEventKind::ScrollDown => 2,
                MouseEventKind::Down(MouseButton::Left) if menu => hits
                    .iter()
                    .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
                    .map(|(_, i)| *i)
                    .unwrap_or(0),
                MouseEventKind::Down(MouseButton::Left)
                    if !menu && width_button.contains(Position::new(m.column, m.row)) =>
                {
                    39
                }
                MouseEventKind::Down(MouseButton::Left)
                    if !menu && go_button.contains(Position::new(m.column, m.row)) =>
                {
                    24
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    match footer_buttons.hit(Position::new(m.column, m.row)) {
                        Some(crate::preferences::Open::Settings) => 26,
                        Some(crate::preferences::Open::Help) => 27,
                        None => 0,
                    }
                }
                _ => continue,
            },
            _ => continue,
        };
        message.clear();
        let old = (n, spread, scrolling, offset);
        match action {
            44 => {
                viewer.document(path, kind);
                continue;
            }
            0 => {
                menu = !menu;
                focus = 0;
                continue;
            }
            1 => n = n.saturating_sub(cols.into()).max(1),
            2 => {
                if let Some(count) = total
                    && n.saturating_add(cols.into()) <= count
                {
                    n += u32::from(cols);
                }
            }
            3 | 4 => {
                spread = action == 4;
                scrolling = false;
                offset = 0;
            }
            28 => {
                scrolling = true;
                spread = false;
            }
            29..=34 => {
                let d = match action {
                    29 => -1,
                    30 => 1,
                    31 => -i32::from(strip.height.max(1)),
                    32 => i32::from(strip.height.max(1)),
                    33 => -3,
                    _ => 3,
                };
                strip.shift(&mut n, &mut offset, d);
                if d > 0 && strip.end(n, offset) {
                    finished = true;
                    dirty = true;
                }
            }
            5 => break,
            6 => {
                n = 1;
                offset = 0;
            }
            7 => n = total.unwrap_or(n),
            8 | 9 => crop.step(action == 8),
            39 => dialog = Some(controls::Dialog::Width),
            40 => crop = Crop::default(),
            41..=43 => {
                let appearance = &mut checkpoint.scan_appearance;
                if action == 43 {
                    (appearance.inverted, appearance.terminal_paper) =
                        match (appearance.inverted, appearance.terminal_paper) {
                            (_, true) => (false, false),
                            (true, false) => (false, true),
                            (false, false) => (true, false),
                        };
                } else {
                    let step = if action == 41 { 1 } else { 3 };
                    appearance.quarter_turns = (appearance.quarter_turns % 4 + step) % 4;
                    crop.x = 128;
                    crop.y = 0;
                }
                message = format!(
                    "{}° · {}",
                    u16::from(appearance.quarter_turns) * 90,
                    if appearance.terminal_paper {
                        "Match background"
                    } else if appearance.inverted {
                        "Inverted colors"
                    } else {
                        "Original colors"
                    }
                );
                dirty = true;
            }
            10 => crop.x = crop.x.saturating_sub(64),
            11 => crop.x = crop.x.saturating_add(64),
            12 => crop.y = crop.y.saturating_sub(64),
            13 => crop.y = crop.y.saturating_add(64),
            18..=22 => {
                if let Some(store) = store.as_ref() {
                    match annotations::open(term, action, store, path, &mut checkpoint) {
                        Ok((page, msg)) => {
                            if let Some(page) = page {
                                n = page.min(total.unwrap_or(page)).max(1);
                            }
                            message = msg;
                        }
                        Err(e) => message = e.to_string(),
                    }
                } else {
                    message = "Local reading state unavailable".into();
                }
            }
            23 => {
                if let Err(e) = crate::stats::show(
                    term,
                    store.as_ref(),
                    opened,
                    total.map_or(0., |v| f64::from(n) * 100. / f64::from(v)),
                ) {
                    message = e.to_string();
                }
            }
            26 | 27 => {
                let before = crate::settings::Settings::load()?.reading_layout;
                crate::preferences::show(
                    term,
                    if action == 26 {
                        crate::preferences::Open::Settings
                    } else {
                        crate::preferences::Open::Help
                    },
                    crate::preferences::Context::Scan,
                )?;
                let updated = crate::settings::Settings::load()?;
                zen = updated.zen;
                zen_progress = updated.zen_progress;
                splitter.preferred = updated.contents_width;
                if before != updated.reading_layout {
                    scrolling = updated.reading_layout.as_deref() == Some("scroll");
                    spread = updated.reading_layout.as_deref() == Some("spread");
                    offset = 0;
                    dirty = true;
                }
            }
            25 => {
                if let Err(e) = crate::appearance::cycle() {
                    message = e.to_string();
                }
                continue;
            }
            24 => dialog = Some(controls::Dialog::page()),
            16 if plates.is_some() => {
                message = "This scanned EPUB has no searchable text layer.".into()
            }
            16 => {
                dialog = Some(controls::Dialog::Search(crate::prompt::LineInput::new(
                    &search.query,
                )))
            }
            17 | 37 => {
                if let Some(page) = search.step(n, action == 17) {
                    n = page;
                    offset = 0;
                    message = search.status();
                }
            }
            38 => {
                if let Some(page) = search.choose(term, n)? {
                    n = page;
                    offset = 0;
                }
            }
            35 | 36 => {
                let target = if action == 35 {
                    history.back(previous)
                } else {
                    history.forward(previous)
                };
                if let Some(target) = target {
                    n = target.page;
                    offset = target.offset;
                    crop = target.crop;
                } else {
                    message = if action == 35 {
                        "No earlier jump"
                    } else {
                        "No later jump"
                    }
                    .into();
                }
            }
            45 => {
                zen_progress = !zen_progress;
                crate::settings::Settings::update(|s| s.zen_progress = zen_progress)?;
            }
            14 => {
                zen = !zen;
                crate::settings::Settings::update(|s| s.zen = zen)?;
            }
            15 => {
                crate::settings::Settings::update(|s| s.opaque_background = !s.opaque_background)?;
            }
            _ => (),
        }
        if old.0 != n && !(29..=36).contains(&action) {
            crop.x = 128;
            crop.y = 0;
            offset = 0;
        }
        menu = false;
        if action == 7 && scrolling {
            // The last raster might still be rendering. Its final viewport is
            // resolved from its actual dimensions when it becomes available.
            offset = u16::MAX;
        }
        if matches!(action, 6 | 7 | 17 | 19 | 21 | 24 | 37 | 38) {
            history.record(
                previous,
                Location {
                    page: n,
                    offset,
                    crop,
                },
            );
        }
        if action == 2 && total.is_some_and(|t| n.saturating_add(u32::from(cols)) > t) {
            finish_on_arrival = Some(n);
        } else if old != (n, spread, scrolling, offset) {
            finish_on_arrival = None;
        }
        dirty |= old != (n, spread, scrolling, offset);
    }
    if initial_zoom != crop.percent
        && let Err(e) = crate::settings::Settings::update(|s| s.scan_zoom = Some(crop.percent))
    {
        notices.push(e.to_string());
    }
    crate::stats::finish(store.as_ref(), opened, notices);
    if (dirty || save_failed)
        && let Some(store) = &store
        && let Err(e) = store.save_position(&checkpoint)
    {
        notices.push(e.to_string());
    }
    let percentage =
        total.map(|count| scan_percent(n, offset, count, scrolling, finished && end_visible));
    let shelf_percent = if base.is_none()
        && let (Some(count), Some(saved)) =
            (total, store.as_ref().and_then(|s| s.load().ok()).flatten())
    {
        let page = (saved.anchor.block.saturating_add(1)).min(count as usize) as u32;
        Some(scan_percent(
            page.max(1),
            saved.scan_offset,
            count,
            saved.scrolling,
            saved.finished,
        ))
    } else {
        percentage
    };
    Ok((
        if base.is_some_and(|p| p.text.cfi.is_none())
            && (n != start || finished != initial_finished || offset != initial_offset)
        {
            percentage.map(|percentage| botui_bookorbit::progress::Move {
                target: botui_bookorbit::progress::Target::Page(n),
                percentage,
            })
        } else {
            None
        },
        shelf_percent,
    ))
}

fn scan_percent(n: u32, offset: u16, count: u32, scrolling: bool, at_finish: bool) -> f64 {
    if at_finish {
        100.
    } else if scrolling {
        ((f64::from(n - 1) + f64::from(offset) / 65535.) * 100. / f64::from(count)).min(99.)
    } else {
        (f64::from(n) * 100. / f64::from(count)).min(99.)
    }
}
