mod illustration;
mod note;
use crate::{
    AppTerminal, centered,
    cover::{Picture, graphics},
};
use botui_core::{Anchor, Document, Target};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Stylize,
    text::Line,
    widgets::{Clear, Paragraph, Wrap},
};
use std::{io, path::Path};
pub(super) fn choose(
    term: &mut AppTerminal,
    doc: &Document,
    at: Anchor,
    visible: Option<&[usize]>,
) -> io::Result<Option<usize>> {
    let ids: Vec<_> = visible
        .map(<[usize]>::to_vec)
        .unwrap_or_else(|| (0..doc.references.len()).collect());
    if visible.is_some() && ids.len() == 1 {
        return Ok(ids.first().copied());
    }
    let labels: Vec<_> = ids
        .iter()
        .map(|&i| {
            let r = &doc.references[i];
            format!(
                " {} · {}",
                match r.target {
                    Target::Web(_) => "Website",
                    Target::Passage(_) => "Passage",
                    Target::Image(_) => "Illustration",
                    _ => "Note",
                },
                r.label.replace(char::is_control, " ")
            )
        })
        .collect();
    let initial = ids
        .iter()
        .position(|&i| doc.references[i].source >= at.block)
        .unwrap_or(labels.len().saturating_sub(1));
    let title = if visible.is_some() {
        "Links on this page"
    } else {
        "Book links & illustrations"
    };
    Ok(crate::prompt::choose(term, title, &labels, initial)?.map(|n| ids[n]))
}
pub(super) fn open(
    term: &mut AppTerminal,
    path: &Path,
    book: &Document,
    r: &botui_core::Reference,
    background: impl FnMut(&mut ratatui::Frame) -> bool,
) -> crate::Result<Option<Anchor>> {
    open_target(term, path, book, &r.label, &r.target, background)
}
pub(super) fn open_target(
    term: &mut AppTerminal,
    path: &Path,
    book: &Document,
    label: &str,
    target: &Target,
    mut background: impl FnMut(&mut ratatui::Frame) -> bool,
) -> crate::Result<Option<Anchor>> {
    match target {
        Target::Passage(a) => return Ok(Some(*a)),
        Target::Note(t) => return Ok(view(term, label, Content::Text(t), None, &mut background)?),
        Target::Supplement {
            member,
            fragment,
            origin,
        } => {
            let origin = origin.as_ref().map(|(m, id)| (m.as_ref(), id.as_str()));
            let (note, footer) =
                botui_formats::supplementary_document(path, member, fragment, origin, book)?;
            let dest = view(
                term,
                label,
                Content::Note(&note),
                footer.as_deref().filter(|_| origin.is_some()),
                &mut background,
            )?;
            // Returning to the citation which opened this popup restores its exact viewport.
            let source = origin
                .and_then(|(file, id)| book.epub_anchors.get(&(file.to_owned(), id.to_owned())));
            return Ok(dest.filter(|a| Some(a) != source));
        }
        Target::Image(member) => {
            let bytes = botui_formats::illustration(path, member)?;
            let picture =
                Picture::illustration(&bytes).ok_or("Cannot display this image format or size")?;
            view(
                term,
                label,
                Content::Picture(&picture, &bytes),
                None,
                &mut background,
            )?;
        }
        Target::Web(url) => {
            let labels = vec![format!(" Open in browser: {url}"), " Cancel".to_owned()];
            if crate::prompt::choose(term, "External website", &labels, 1)? == Some(0) {
                use std::process::{Command, Stdio};
                let mut child = Command::new("xdg-open")
                    .arg(url)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
        }
    }
    Ok(None)
}
enum Content<'a> {
    Text(&'a str),
    Note(&'a Document),
    Picture(&'a Picture, &'a [u8]),
}

const LINK_HINT: &str = "Tab / Shift+Tab · Select link   Enter · Follow";

impl Content<'_> {
    fn note_width(&self, title: &str, buttons: &str) -> Option<u16> {
        let width = |s: &str| Line::raw(s).width();
        let (text, hint) = match self {
            Self::Picture(..) => return None,
            Self::Text(text) => (text.lines().map(width).max().unwrap_or(0), 0),
            Self::Note(doc) => {
                let hint = if doc.references.is_empty() {
                    0
                } else {
                    2 + width(LINK_HINT)
                };
                (
                    doc.blocks
                        .iter()
                        .flat_map(|b| b.text.lines())
                        .map(width)
                        .max()
                        .unwrap_or(0),
                    hint,
                )
            }
        };
        Some((2 + text.max(width(title)).max(width(buttons) + hint)).min(80) as u16)
    }
}

fn view(
    term: &mut AppTerminal,
    label: &str,
    content: Content<'_>,
    backlink: Option<&str>,
    background: &mut impl FnMut(&mut ratatui::Frame) -> bool,
) -> io::Result<Option<Anchor>> {
    let mut note_page = None;
    let mut scroll = 0u16;
    let mut illustration = illustration::Illustration::default();
    let mut viewer = crate::cover_viewer::Viewer::default();
    let mut refresh_background = true;
    let title = format!(" {} ", label.replace(char::is_control, " "));
    let buttons = backlink.map_or_else(
        || "[ Back · Esc ]".into(),
        |s| format!("[ Back · Esc ]  [ {s} ]"),
    );
    let note_width = content.note_width(&title, &buttons);
    loop {
        let mut page = 1;
        let mut limit = 0;
        let mut back = Rect::default();
        if let Some(error) = viewer.collect() {
            crate::prompt::message(term, "Open illustration", &error)?;
        }
        let mut loading = false;
        let draw_dialog = |f: &mut ratatui::Frame| {
            let screen = f.area();
            let mut area = centered(
                screen,
                note_width.unwrap_or_else(|| screen.width.saturating_sub(2)),
                screen.height.saturating_sub(2),
            );
            let block = crate::appearance::block()
                .title_style(ratatui::style::Style::new().cyan().not_dim())
                .title(title.as_str());
            let inner = block.inner(area);
            let available = Rect {
                height: inner.height.saturating_sub(1),
                ..inner
            };
            let rows = match content {
                Content::Text(text) => Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .line_count(available.width.max(1)),
                Content::Note(doc) => note_page
                    .get_or_insert_with(|| note::NotePage::new(doc, available))
                    .visible_height(available),
                Content::Picture(..) => usize::from(available.height),
            };
            limit = rows
                .saturating_sub(usize::from(available.height))
                .min(u16::MAX as usize) as u16;
            let unused = available
                .height
                .saturating_sub(rows.min(u16::MAX as usize) as u16);
            area = centered(screen, area.width, area.height - unused);
            crate::overlay::paint(f, area, |f| {
                let inner = block.inner(area);
                f.render_widget(Clear, area);
                f.render_widget(block, area);
                if let Content::Picture(picture, _) = content {
                    illustration.draw(f, inner, picture, &graphics());
                    return;
                }
                let body = Rect {
                    height: inner.height.saturating_sub(1),
                    ..inner
                };
                page = body.height.max(1);
                if let Some(note) = &mut note_page {
                    note.draw(f, body);
                }
                if let Content::Text(text) = content {
                    let p = Paragraph::new(text).wrap(Wrap { trim: false });
                    scroll = scroll.min(limit);
                    f.render_widget(p.scroll((scroll, 0)), body);
                }
                back = Rect::new(
                    inner.x,
                    inner.bottom().saturating_sub(1),
                    Line::raw(buttons.as_str())
                        .width()
                        .min(usize::from(inner.width)) as u16,
                    inner.height.min(1),
                );
                f.render_widget(Paragraph::new(buttons.as_str()).cyan(), back);
                if matches!(content, Content::Note(doc) if !doc.references.is_empty()) {
                    let x = back.right().saturating_add(2);
                    let hint = Rect::new(x, back.y, inner.right().saturating_sub(x), back.height);
                    f.render_widget(Paragraph::new(LINK_HINT).cyan(), hint);
                }
            });
            crate::settings::paint_dialog(f)
        };
        term.draw_layered(
            refresh_background,
            |f| loading |= background(f),
            draw_dialog,
        )?;
        refresh_background = loading;
        let timeout = (loading || viewer.busy()).then(|| std::time::Duration::from_millis(70));
        let Some(event) = crate::settings::read_event_for(timeout)? else {
            continue;
        };
        refresh_background |= matches!(event, Event::Resize(..));
        if let Content::Picture(image, bytes) = content {
            match illustration.key(&event) {
                Some(KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | ',')) => return Ok(None),
                Some(KeyCode::Char('o')) => (),
                Some(key) => {
                    illustration.adjust(key);
                    continue;
                }
                None => continue,
            }
            // SVGs use the reader's self-contained rendering, including embedded resources.
            let result = if image::guess_format(bytes).is_ok() {
                viewer.illustration(bytes)
            } else {
                image
                    .illustration_png()
                    .and_then(|png| viewer.illustration(&png))
            };
            if let Err(e) = result {
                crate::prompt::message(term, "Open illustration", &e.to_string())?;
            }
            continue;
        }
        let mut move_by = |rows: isize| {
            if let Some(note) = &mut note_page {
                note.scroll(rows);
            } else {
                scroll = ((scroll as isize).saturating_add(rows)).clamp(0, limit as isize) as u16;
            }
        };
        match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q' | ',') => return Ok(None),
                KeyCode::Enter => return Ok(note_page.as_ref().and_then(|p| p.destination(None))),
                KeyCode::Tab | KeyCode::BackTab => {
                    if let Some(note) = &mut note_page {
                        note.select(
                            k.code == KeyCode::Tab
                                && !k.modifiers.contains(crossterm::event::KeyModifiers::SHIFT),
                        );
                    }
                }
                KeyCode::Down => move_by(1),
                KeyCode::Up => move_by(-1),
                KeyCode::PageDown => move_by(page as isize),
                KeyCode::PageUp => move_by(-(page as isize)),
                _ => (),
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let pos = (m.column, m.row).into();
                    if back.contains(pos) {
                        return Ok(None);
                    }
                    if let Some(dest) = note_page.as_ref().and_then(|p| p.destination(Some(pos))) {
                        return Ok(Some(dest));
                    }
                }
                MouseEventKind::ScrollDown => move_by(2),
                MouseEventKind::ScrollUp => move_by(-2),
                _ => (),
            },
            _ => (),
        }
    }
}
