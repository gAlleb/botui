use crate::{
    AppTerminal, centered,
    cover::{Picture, graphics},
};
use botui_core::{Anchor, Document, Target};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Stylize,
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
    r: &botui_core::Reference,
    mut background: impl FnMut(&mut ratatui::Frame),
) -> crate::Result<Option<Anchor>> {
    match &r.target {
        Target::Passage(a) => return Ok(Some(*a)),
        Target::Note(t) => view(term, &r.label, Some(t), None, None, &mut background)?,
        Target::Supplement {
            member,
            fragment,
            origin,
        } => {
            let note = botui_formats::supplementary_note(
                path,
                member,
                fragment,
                origin.as_ref().map(|(m, id)| (m.as_ref(), id.as_str())),
            )?;
            view(
                term,
                &r.label,
                Some(&note.text),
                None,
                note.backlink.as_deref(),
                &mut background,
            )?;
        }
        Target::Image(member) => {
            let bytes = botui_formats::illustration(path, member)?;
            let picture =
                Picture::illustration(bytes).ok_or("Cannot display this image format or size")?;
            view(term, &r.label, None, Some(&picture), None, &mut background)?;
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
fn view(
    term: &mut AppTerminal,
    label: &str,
    text: Option<&str>,
    picture: Option<&Picture>,
    backlink: Option<&str>,
    background: &mut impl FnMut(&mut ratatui::Frame),
) -> io::Result<()> {
    let mut scroll = 0u16;
    let graphics = graphics();
    loop {
        let mut page = 1;
        let mut limit = 0;
        let mut back = Rect::default();
        term.draw(|f| {
            background(f);
            let screen = f.area();
            let area = centered(
                screen,
                if picture.is_some() {
                    screen.width.saturating_sub(2)
                } else {
                    80
                },
                screen.height.saturating_sub(2),
            );
            let block = crate::appearance::block()
                .title(format!(" {} ", label.replace(char::is_control, " ")));
            let inner = block.inner(area);
            f.render_widget(Clear, area);
            f.render_widget(block, area);
            let body = Rect {
                height: inner.height.saturating_sub(1),
                ..inner
            };
            page = body.height.max(1);
            if let Some(picture) = picture {
                picture.draw(f, body, &graphics);
            }
            if let Some(text) = text {
                let p = Paragraph::new(text).wrap(Wrap { trim: false });
                limit = p
                    .line_count(body.width.max(1))
                    .saturating_sub(body.height as usize)
                    .min(u16::MAX as usize) as u16;
                scroll = scroll.min(limit);
                f.render_widget(p.scroll((scroll, 0)), body);
            }
            let label = backlink
                .map(|s| format!("[ Back · Esc ]  [ {} ]", s))
                .unwrap_or_else(|| "[ Back · Esc ]".into());
            let width = ratatui::text::Line::raw(&label).width() as u16;
            back = Rect::new(
                inner.x,
                inner.bottom().saturating_sub(1),
                inner.width.min(width),
                inner.height.min(1),
            );
            f.render_widget(Paragraph::new(label).cyan(), back);
            crate::settings::paint(f)
        })?;
        match crate::settings::read_event()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | ',') => return Ok(()),
                KeyCode::Down => scroll = scroll.saturating_add(1).min(limit),
                KeyCode::Up => scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => scroll = scroll.saturating_add(page).min(limit),
                KeyCode::PageUp => scroll = scroll.saturating_sub(page),
                _ => (),
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left)
                    if back.contains((m.column, m.row).into()) =>
                {
                    return Ok(());
                }
                MouseEventKind::ScrollDown => scroll = scroll.saturating_add(2).min(limit),
                MouseEventKind::ScrollUp => scroll = scroll.saturating_sub(2),
                _ => (),
            },
            _ => (),
        }
    }
}
