use crate::{
    AppTerminal, centered,
    cover::{Picture, graphics},
};
use botui_core::{Anchor, Document, Target};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Stylize,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use std::{io, path::Path};
pub(super) fn open(
    term: &mut AppTerminal,
    path: &Path,
    doc: &Document,
    at: Anchor,
) -> crate::Result<Option<Anchor>> {
    let labels: Vec<_> = doc
        .references
        .iter()
        .map(|r| {
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
    let initial = doc
        .references
        .iter()
        .position(|r| r.source >= at.block)
        .unwrap_or(labels.len().saturating_sub(1));
    let Some(i) = crate::prompt::choose(term, "Book links & illustrations", &labels, initial)?
    else {
        return Ok(None);
    };
    let r = &doc.references[i];
    match &r.target {
        Target::Passage(a) => return Ok(Some(*a)),
        Target::Note(t) => view(term, &r.label, Some(t), None)?,
        Target::Supplement { member, fragment } => {
            let t = botui_formats::supplement(path, member, fragment)?;
            view(term, &r.label, Some(&t), None)?;
        }
        Target::Image(member) => {
            let bytes = botui_formats::illustration(path, member)?;
            let picture =
                Picture::illustration(bytes).ok_or("Cannot display this image format or size")?;
            view(term, &r.label, None, Some(&picture))?;
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
) -> io::Result<()> {
    let mut scroll = 0u16;
    let graphics = graphics();
    loop {
        let mut page = 1;
        let mut limit = 0;
        term.draw(|f| {
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
            let block =
                Block::bordered().title(format!(" {} ", label.replace(char::is_control, " ")));
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
            f.render_widget(
                Paragraph::new("[ Back · Esc ]").cyan(),
                Rect::new(
                    inner.x,
                    inner.bottom().saturating_sub(1),
                    inner.width,
                    inner.height.min(1),
                ),
            );
            crate::settings::paint(f)
        })?;
        match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => return Ok(()),
                KeyCode::Down => scroll = scroll.saturating_add(1).min(limit),
                KeyCode::Up => scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => scroll = scroll.saturating_add(page).min(limit),
                KeyCode::PageUp => scroll = scroll.saturating_sub(page),
                _ => (),
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => return Ok(()),
                MouseEventKind::ScrollDown => scroll = scroll.saturating_add(2).min(limit),
                MouseEventKind::ScrollUp => scroll = scroll.saturating_sub(2),
                _ => (),
            },
            _ => (),
        }
    }
}
