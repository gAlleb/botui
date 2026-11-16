//! A separate viewport over the current EPUB table leaves the reading pager in place.
use super::{Reader, controls, paint_surfaces};
use crate::AppTerminal;
use botui_core::{Anchor, PageLine, publisher::TableView};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{layout::Rect, style::Stylize, widgets::Paragraph};
use std::{collections::BTreeMap, io};

pub(super) fn open(term: &mut AppTerminal, reader: &Reader) -> io::Result<()> {
    let view = reader.pager.viewport_rows();
    let mut visible = BTreeMap::new();
    for line in (0..view.columns()).flat_map(|c| view.lines(c)) {
        for at in line.spans.iter().filter_map(|s| s.source) {
            if let Some((&start, chapter)) = reader.doc.styled.range(..=at.block).next_back() {
                for (index, table) in chapter.tables.iter().enumerate() {
                    if table.source.contains(&at) {
                        visible.entry((start, index)).or_insert(at);
                    }
                }
            }
        }
    }
    let choices: Vec<_> = visible.into_iter().collect();
    let selected = match choices.as_slice() {
        [] => return crate::prompt::message(term, "Table", "There is no data table on this page."),
        [one] => *one,
        _ => {
            let labels = choices
                .iter()
                .map(|&((start, index), _)| reader.doc.styled[&start].tables[index].title.clone())
                .collect::<Vec<_>>();
            let Some(i) = crate::prompt::choose(term, "Tables on this page", &labels, 0)? else {
                return Ok(());
            };
            choices[i]
        }
    };
    let ((start, index), at) = selected;
    let chapter = &reader.doc.styled[&start];
    let title = &chapter.tables[index].title;
    let mut at = Some(at);
    let (mut top, mut left) = (0usize, 0u16);
    let mut rendered = None;
    let mut geometry = Rect::default();
    let keys = [
        KeyCode::Esc,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Up,
        KeyCode::Down,
    ];
    let labels = ["[Esc Back]", "[←]", "[→]", "[↑]", "[↓]"];
    loop {
        let size = term.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        let block = crate::appearance::block().title(format!(" Table · {title} "));
        let inner = block.inner(area);
        let (buttons, toolbar_height) = controls::layout(inner, &labels);
        let body = Rect {
            height: inner.height.saturating_sub(toolbar_height),
            ..inner
        };
        if geometry != body || rendered.is_none() {
            if let Some(old) = &rendered {
                at = content_anchor(old, top, geometry.height);
            }
            let Some(layout) = chapter.table_view(
                index,
                &reader.doc.blocks,
                body.width,
                body.height,
                reader.pager.reflow_options(),
            ) else {
                return crate::prompt::message(term, "Table", "Could not open this table.");
            };
            if let Some(anchor) = at {
                let row = layout
                    .rows
                    .iter()
                    .position(|line| {
                        line.spans.iter().any(|s| {
                            s.source.is_some_and(|a| {
                                a.block == anchor.block
                                    && (a.byte..s.source_end).contains(&anchor.byte)
                            })
                        })
                    })
                    .unwrap_or(0);
                top = if row >= layout.header.end {
                    row.saturating_sub(layout.header.len())
                } else {
                    0
                };
            }
            rendered = Some(layout);
            geometry = body;
        }
        let layout = rendered.as_ref().unwrap();
        top = top.min(layout.rows.len().saturating_sub(body.height.into()));
        let horizontal = layout.width.saturating_sub(body.width);
        left = left.min(horizontal);
        let pin = pinned(layout, top, body.height);
        term.draw(|f| {
            let theme = reader.theme;
            let style = theme.style();
            f.render_widget(block.style(style), area);
            for (y, i) in pin
                .clone()
                .chain((top + pin.len())..layout.rows.len())
                .take(body.height.into())
                .enumerate()
            {
                let line = &layout.rows[i];
                let rect = Rect::new(body.x, body.y + y as u16, body.width, 1);
                f.render_widget(
                    Paragraph::new(reader.search.line(line, theme))
                        .style(style)
                        .scroll((0, left)),
                    rect,
                );
                paint_surfaces(f, line, rect, left, theme, style);
            }
            for (r, label) in buttons.iter().copied().zip(labels) {
                f.render_widget(Paragraph::new(label).cyan(), r);
            }
            crate::settings::paint_dialog(f);
        })?;
        let key = match crate::settings::read_event()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => k.code,
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let Some(i) = buttons
                        .iter()
                        .position(|r| r.contains((m.column, m.row).into()))
                    else {
                        continue;
                    };
                    keys[i]
                }
                MouseEventKind::ScrollDown => KeyCode::Down,
                MouseEventKind::ScrollUp => KeyCode::Up,
                MouseEventKind::ScrollLeft => KeyCode::Left,
                MouseEventKind::ScrollRight => KeyCode::Right,
                _ => continue,
            },
            _ => continue,
        };
        let page = usize::from(body.height).saturating_sub(pin.len()).max(1);
        match key {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Left | KeyCode::Char('h') => left = left.saturating_sub(4),
            KeyCode::Right | KeyCode::Char('l') => left = left.saturating_add(4),
            KeyCode::Up | KeyCode::Char('k') => top = top.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => top = top.saturating_add(1),
            KeyCode::PageUp => top = top.saturating_sub(page),
            KeyCode::PageDown => top = top.saturating_add(page),
            KeyCode::Home => top = 0,
            KeyCode::End => top = layout.rows.len(),
            _ => (),
        }
    }
}

fn pinned(layout: &TableView, top: usize, height: u16) -> std::ops::Range<usize> {
    if top >= layout.header.start && layout.header.len() < usize::from(height) {
        layout.header.clone()
    } else {
        0..0
    }
}

fn content_anchor(layout: &TableView, top: usize, height: u16) -> Option<Anchor> {
    layout
        .rows
        .get(top + pinned(layout, top, height).len()..)?
        .iter()
        .find(|line| !line.text.trim().is_empty())
        .map(PageLine::anchor)
}
