//! A fixed page is addressed by its printed page index, never by a text offset.
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
    widgets::{Block, Clear, Paragraph},
};
use std::{io, path::Path, sync::mpsc::TryRecvError, time::Duration};

pub fn read(
    term: &mut AppTerminal,
    path: &Path,
    kind: FileKind,
    notices: &mut Vec<String>,
) -> io::Result<()> {
    read_server(term, path, kind, notices, None).map(|_| ())
}
pub fn read_server(
    term: &mut AppTerminal,
    path: &Path,
    kind: FileKind,
    notices: &mut Vec<String>,
    base: Option<&botui_bookorbit::progress::Progress>,
) -> io::Result<Option<botui_bookorbit::progress::Move>> {
    let picker = graphics();
    let worker = Worker::new();
    let p = path.to_owned();
    let count_job = worker.start(move |cancel| botui_fixed::page_count(&p, kind, cancel));
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
    let start = n;
    let mut spread = saved.is_some_and(|s| s.spread);
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
                    return Err(io::Error::other("Подсчёт страниц прерван"));
                }
            }
        }
        let screen = term.size()?;
        let cols = if spread && total != Some(1) && screen.width >= 68 {
            2u16
        } else {
            1
        };
        let size = picker.font_size();
        let side = (screen.width / cols)
            .saturating_mul(size.width)
            .max(screen.height.saturating_mul(size.height))
            .clamp(240, 1600);
        let side = side.saturating_mul(1 << crop.level).min(1920);
        if let Some(count) = total {
            pages.update(path, kind, n, count, cols, side)?;
        }
        let mut hits = vec![];
        term.draw(|f| {
            let area = f.area();
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
                    Paragraph::new("Готовим страницу…").centered(),
                    centered(area, 36, 1),
                );
            }
            if menu {
                let panel = centered(area, 48, 16);
                let block = Block::bordered().title(format!(
                    " {} · {} / {} ",
                    kind.label(),
                    n,
                    total.unwrap_or(0)
                ));
                let inner = block.inner(panel);
                f.render_widget(Clear, panel);
                f.render_widget(block, panel);
                let labels = [
                    "Продолжить чтение       Esc",
                    "Предыдущая страница      ←",
                    "Следующая страница       →",
                    "Одна страница            1",
                    "Разворот                 2",
                    "Закрыть книгу            Q",
                    "К началу                 Home",
                    "К концу                  End",
                    "Увеличить                +",
                    "Уменьшить                −",
                    "Сдвиг влево              ←",
                    "Сдвиг вправо             →",
                    "Сдвиг вверх              ↑",
                    "Сдвиг вниз               ↓",
                ];
                let offset = focus
                    .saturating_add(1)
                    .saturating_sub(usize::from(inner.height));
                for (i, label) in labels
                    .iter()
                    .skip(offset)
                    .enumerate()
                    .take(inner.height as usize)
                {
                    let r = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
                    f.render_widget(
                        Paragraph::new(*label).style(if i + offset == focus {
                            ratatui::style::Style::new().reversed()
                        } else {
                            Default::default()
                        }),
                        r,
                    );
                    hits.push((r, i + offset));
                }
            }
        })?;
        if (total.is_none() || pages.busy()) && !event::poll(Duration::from_millis(60))? {
            continue;
        }
        let action = match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => 5,
                KeyCode::Up | KeyCode::BackTab if menu => {
                    focus = focus.saturating_sub(1);
                    continue;
                }
                KeyCode::Down | KeyCode::Tab if menu => {
                    focus = (focus + 1).min(13);
                    continue;
                }
                KeyCode::Enter if menu => focus,
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
                    focus = (focus + 1).min(13);
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
            _ => (),
        }
        if old.0 != n {
            crop.x = 0;
            crop.y = 0;
        }
        menu = false;
        dirty |= old != (n, spread);
    }
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
            marks: vec![],
        })
    {
        notices.push(e.to_string());
    }
    Ok(
        if base.is_some_and(|p| p.text.cfi.is_none()) && n != start {
            total.map(|count| botui_bookorbit::progress::Move {
                target: botui_bookorbit::progress::Target::Page(n),
                percentage: f64::from(n) * 100.0 / f64::from(count),
            })
        } else {
            None
        },
    )
}
