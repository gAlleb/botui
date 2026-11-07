//! A fixed page is addressed by its printed page index, never by a text offset.
mod annotations;
mod contents;
mod pages;
use crate::{
    AppTerminal, centered,
    cover::{Crop, graphics},
    shelf::background::Worker,
};
use botui_core::Anchor;
use botui_formats::FileKind;
use botui_store::{LocalState, Saved};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
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
    let picker = graphics();
    let worker = Worker::new();
    let p = path.to_owned();
    let count_job = worker.start(move |cancel| botui_fixed::page_count(&p, kind, cancel));
    let prefs = crate::settings::Settings::load()?;
    let mut zen = prefs.zen;
    let mut sidebar = contents::Contents::default();
    let outline_path = path.to_owned();
    let mut outline_job =
        Some(worker.start(move |cancel| botui_fixed::outline(&outline_path, kind, cancel)));
    let mut total = None;
    let mut pages = pages::Pages::new();
    let mut crop = Crop::default();
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
    let opened = std::time::Instant::now();
    let mut marks = saved.as_ref().map(|s| s.marks.clone()).unwrap_or_default();
    let start = n;
    let mut spread = saved.is_some_and(|s| s.spread);
    let mut search_job: Option<crate::shelf::background::Ticket<io::Result<Vec<u32>>>> = None;
    let mut found: Vec<u32> = vec![];
    let mut query = String::new();
    let mut message = String::new();
    let mut menu = false;
    let mut focus: usize = 0;
    let mut dirty = false;

    loop {
        if total.is_none() {
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
                    sidebar.items = result.unwrap_or_default();
                    outline_job = None;
                }
                Err(TryRecvError::Disconnected) => outline_job = None,
                Err(TryRecvError::Empty) => (),
            }
        }
        if let Some(job) = &search_job {
            let result = match job.receiver.try_recv() {
                Ok(r) => Some(r),
                Err(TryRecvError::Empty) => None,
                Err(_) => Some(Err(io::Error::other("Search interrupted"))),
            };
            if let Some(r) = result {
                search_job = None;
                match r {
                    Ok(v) => {
                        found = v;
                        if found.is_empty() {
                            message = "No matches in the embedded text".into();
                        } else {
                            n = found.iter().copied().find(|p| *p >= n).unwrap_or(found[0]);
                            dirty = true;
                            crop = Crop::default();
                            message = if found.len() == 1 {
                                "1 matching page".to_owned()
                            } else {
                                format!("{} matching pages · N next", found.len())
                            };
                        }
                    }
                    Err(e) => message = e.to_string(),
                }
            }
        }
        let screen = term.size()?;
        let rail = if !zen && screen.width >= 78 {
            (screen.width / 5).clamp(22, 32)
        } else {
            0
        };
        let available = screen.width - rail;
        let cols = if spread && total != Some(1) && available >= 68 {
            2u16
        } else {
            1
        };
        let size = picker.font_size();
        let side = (available / cols)
            .saturating_mul(size.width)
            .max(screen.height.saturating_mul(size.height))
            .clamp(240, 1600);
        let side = side.saturating_mul(1 << crop.level).min(1920);
        if let Some(count) = total {
            pages.update(path, kind, n, count, cols, side)?;
        }
        let mut hits = vec![];
        term.draw(|f| {
            let screen = f.area();
            sidebar.draw(
                f,
                Rect::new(screen.x, screen.y, rail, screen.height),
                n,
                total.unwrap_or(0),
            );
            let area = Rect::new(
                screen.x + rail,
                screen.y,
                available,
                screen.height.saturating_sub(u16::from(!zen)),
            );
            if !zen && screen.height > 0 {
                f.render_widget(
                    Paragraph::new(format!(
                        " {}   ·   {} / {}   ·   Z Zen   M Menu",
                        kind.label(),
                        n,
                        total.unwrap_or(0)
                    ))
                    .dim(),
                    Rect::new(area.x, screen.bottom() - 1, area.width, 1),
                );
            }
            for i in 0..cols {
                let Some(page) = pages.get(n.saturating_add(u32::from(i)), side) else {
                    continue;
                };
                let w = if cols == 2 {
                    area.width.saturating_sub(2) / 2
                } else {
                    area.width
                };
                page.draw_region(
                    f,
                    Rect::new(area.x + (w + 2).saturating_mul(i), area.y, w, area.height),
                    &picker,
                    crop,
                );
            }
            if pages.get(n, side).is_none() {
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
                let panel = centered(screen, 48, 20);
                let block = crate::appearance::block().title(format!(
                    " {} · {} / {} ",
                    kind.label(),
                    n,
                    total.unwrap_or(0)
                ));
                let inner = block.inner(panel);
                f.render_widget(Clear, panel);
                f.render_widget(block, panel);
                let labels = [
                    ("Resume", "Esc"),
                    ("Previous page", "←"),
                    ("Next page", "→"),
                    ("Single page", "1"),
                    ("Two-page spread", "2"),
                    ("Close book", "Q"),
                    ("First page", "Home"),
                    ("Last page", "End"),
                    ("Zoom in", "+"),
                    ("Zoom out", "−"),
                    ("Pan left", "←"),
                    ("Pan right", "→"),
                    ("Pan up", "↑"),
                    ("Pan down", "↓"),
                    ("Toggle zen mode", "Z"),
                    ("Toggle opaque background", "F"),
                    ("Find in text layer", "/"),
                    ("Next matching page", "N"),
                    ("Toggle page bookmark", "B"),
                    ("Page bookmarks", "G"),
                    ("Edit page note", "I"),
                    ("Page notes", "A"),
                    ("Export notes to Markdown", "E"),
                    ("Reading statistics", "S"),
                    ("Go to page number", "O"),
                    ("Change borders", "U"),
                ];
                let offset = focus
                    .saturating_add(1)
                    .saturating_sub(usize::from(inner.height));
                for (i, (label, shortcut)) in labels
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
                    let key_width = r.width.min(5);
                    f.render_widget(
                        Paragraph::new(*label).style(style),
                        Rect::new(r.x, r.y, r.width - key_width, 1),
                    );
                    f.render_widget(
                        Paragraph::new(*shortcut).right_aligned().style(style),
                        Rect::new(r.right() - key_width, r.y, key_width, 1),
                    );
                    hits.push((r, i + offset));
                }
            }
            crate::settings::paint(f);
        })?;
        if (search_job.is_some() || outline_job.is_some() || total.is_none() || pages.busy())
            && !event::poll(Duration::from_millis(60))?
        {
            continue;
        }
        let e = event::read()?;
        if !menu && let Some(target) = sidebar.input(&e, total.unwrap_or(0)) {
            if let Some(target) = target
                && let Some(count) = total
            {
                n = target.clamp(1, count);
                crop = Crop::default();
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
                    focus = (focus + 1).min(25);
                    continue;
                }
                KeyCode::Enter if menu => focus,
                KeyCode::Char('u' | 'U') => 25,
                KeyCode::Char('b' | 'B') => 18,
                KeyCode::Char('g' | 'G') => 19,
                KeyCode::Char('i' | 'I') => 20,
                KeyCode::Char('a' | 'A') => 21,
                KeyCode::Char('e' | 'E') => 22,
                KeyCode::Char('s' | 'S') => 23,
                KeyCode::Char('o' | 'O') => 24,
                KeyCode::Char('/') => 16,
                KeyCode::Char('n' | 'N') => 17,
                KeyCode::Char('z' | 'Z') => 14,
                KeyCode::Char('f' | 'F') => 15,
                KeyCode::Char('q' | 'Q') => 5,
                KeyCode::Esc if !menu => 5,
                KeyCode::Char('m' | 'M' | '?') | KeyCode::Esc | KeyCode::Enter => 0,
                KeyCode::Char('+' | '=') => 8,
                KeyCode::Char('-') => 9,
                KeyCode::Left if crop.level > 0 => 10,
                KeyCode::Right if crop.level > 0 => 11,
                KeyCode::Up if crop.level > 0 => 12,
                KeyCode::Down if crop.level > 0 => 13,
                KeyCode::Left | KeyCode::PageUp | KeyCode::Up | KeyCode::Char('h' | 'k') => 1,
                KeyCode::Right
                | KeyCode::PageDown
                | KeyCode::Down
                | KeyCode::Char('l' | 'j' | ' ') => 2,
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
                    focus = (focus + 1).min(25);
                    continue;
                }
                MouseEventKind::ScrollUp if crop.level > 0 => 12,
                MouseEventKind::ScrollDown if crop.level > 0 => 13,
                MouseEventKind::ScrollUp => 1,
                MouseEventKind::ScrollDown => 2,
                MouseEventKind::Down(MouseButton::Left) if menu => hits
                    .iter()
                    .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
                    .map(|(_, i)| *i)
                    .unwrap_or(0),
                MouseEventKind::Down(MouseButton::Left) => 0,
                _ => continue,
            },
            _ => continue,
        };
        message.clear();
        let old = (n, spread);
        match action {
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
            3 => spread = false,
            4 => spread = true,
            5 => break,
            6 => n = 1,
            7 => n = total.unwrap_or(n),
            8 => crop.level = crop.level.saturating_add(1).min(2),
            9 => crop.level = crop.level.saturating_sub(1),
            10 => crop.x = crop.x.saturating_sub(64),
            11 => crop.x = crop.x.saturating_add(64),
            12 => crop.y = crop.y.saturating_sub(64),
            13 => crop.y = crop.y.saturating_add(64),
            18..=22 => {
                if let Some(store) = store.as_ref() {
                    match annotations::open(term, action, store, path, n, &mut marks) {
                        Ok((page, msg)) => {
                            if let Some(page) = page {
                                n = page.min(total.unwrap_or(page)).max(1);
                            }
                            message = msg;
                            dirty |= action == 18;
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
            25 => {
                if let Err(e) = crate::appearance::cycle() {
                    message = e.to_string();
                }
                continue;
            }
            24 => {
                if let Some(count) = total
                    && let Some(input) = crate::prompt::edit(
                        term,
                        &format!("Go to page · 1–{count}"),
                        "[ Go ↵ ]",
                        "",
                        |_| {},
                    )?
                {
                    if let Ok(page) = input.parse::<u32>()
                        && (1..=count).contains(&page)
                    {
                        n = page;
                    } else {
                        message = format!("Enter a page from 1 to {count}");
                    }
                }
            }
            16 => {
                if let Some(text) =
                    crate::prompt::edit(term, "Find in text layer", "[ Search ↵ ]", &query, |_| {})?
                    && !text.is_empty()
                {
                    query = text.clone();
                    let path = path.to_owned();
                    found.clear();
                    message = "Searching…".into();
                    search_job = Some(
                        worker.start(move |cancel| botui_fixed::search(&path, kind, &text, cancel)),
                    );
                }
            }
            17 => {
                if let Some(first) = found.first() {
                    n = found.iter().copied().find(|p| *p > n).unwrap_or(*first);
                    message = if found.len() == 1 {
                        "1 matching page".to_owned()
                    } else {
                        format!("{} matching pages · N next", found.len())
                    };
                }
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
        if old.0 != n {
            crop.x = 0;
            crop.y = 0;
        }
        menu = false;
        dirty |= old != (n, spread);
    }
    crate::stats::finish(store.as_ref(), opened, notices);
    if dirty
        && let Some(store) = store
        && let Err(e) = store.save(&Saved {
            anchor: Anchor {
                block: n.saturating_sub(1) as usize,
                byte: 0,
            },
            spread,
            theme: "terminal".into(),
            margin: 0,
            marks,
        })
    {
        notices.push(e.to_string());
    }
    Ok((
        if base.is_some_and(|p| p.text.cfi.is_none()) && n != start {
            total.map(|count| botui_bookorbit::progress::Move {
                target: botui_bookorbit::progress::Target::Page(n),
                percentage: f64::from(n) * 100.0 / f64::from(count),
            })
        } else {
            None
        },
        total.map(|count| f64::from(n) * 100. / f64::from(count)),
    ))
}
