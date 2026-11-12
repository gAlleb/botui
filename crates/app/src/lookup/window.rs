use super::*;
use crate::{
    AppTerminal, appearance, centered,
    settings::Settings,
    shelf::background::{Ticket, Worker},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    style::Stylize,
    widgets::{Clear, Paragraph, Wrap},
};
use std::{cell::RefCell, collections::VecDeque, io, time::Duration};
type Key = (String, String, String, String);
thread_local! {static WORKER:Worker=Worker::new();static CACHE:RefCell<VecDeque<(Key,String)>>=const{RefCell::new(VecDeque::new())};}
fn key(p: &Profile, text: &str, source: &str, target: &str) -> Key {
    (
        serde_json::to_string(p).unwrap_or_default(),
        text.into(),
        source.into(),
        target.into(),
    )
}
fn start(p: Profile, text: String, source: String, target: String) -> Ticket<io::Result<String>> {
    WORKER.with(|w| w.start(move |cancel| providers::request(&p, &text, &source, &target, cancel)))
}
pub fn test(term: &mut AppTerminal, p: Profile) -> io::Result<()> {
    let target = if p.target_language.is_empty() {
        Settings::load()?.lookup.target_language
    } else {
        p.target_language.clone()
    };
    // A connection test must contact the provider, even after a successful lookup.
    let test_key = key(&p, "hello", "auto", &target);
    CACHE.with(|c| c.borrow_mut().retain(|(k, _)| k != &test_key));
    let _ = run(term, "hello", vec![p], 0, "auto".into(), target, |_| {})?;
    Ok(())
}
/// Return a result only when explicitly asked to save it as a passage note.
pub fn show(
    term: &mut AppTerminal,
    text: &str,
    purpose: Purpose,
    underneath: impl FnMut(&mut ratatui::Frame),
) -> io::Result<Option<String>> {
    let config = Config::from_settings(&Settings::load()?);
    let profiles = config.profiles(purpose);
    if profiles.is_empty() {
        crate::prompt::message(
            term,
            "Lookup",
            "No connection configured. Open F2 → Dictionaries.",
        )?;
        return Ok(None);
    }
    let index = profiles
        .iter()
        .position(|p| p.name == config.selected(purpose))
        .unwrap_or(0);
    run(
        term,
        text,
        profiles,
        index,
        config.source_language,
        config.target_language,
        underneath,
    )
}
fn run(
    term: &mut AppTerminal,
    text: &str,
    profiles: Vec<Profile>,
    mut index: usize,
    mut source: String,
    mut target: String,
    mut underneath: impl FnMut(&mut ratatui::Frame),
) -> io::Result<Option<String>> {
    let default_target = target.clone();
    if !profiles[index].target_language.is_empty() {
        target = profiles[index].target_language.clone();
    }
    let mut job = None;
    let mut requested = None;
    let mut result = String::new();
    let mut good = false;
    let mut offset = 0u16;
    let mut status = String::new();
    loop {
        let profile = &profiles[index];
        let current = key(profile, text, &source, &target);
        if requested.as_ref() != Some(&current) {
            job.take();
            offset = 0;
            status.clear();
            good = false;
            if let Some(cached) = CACHE.with(|c| {
                c.borrow()
                    .iter()
                    .find(|(k, _)| k == &current)
                    .map(|(_, v)| v.clone())
            }) {
                result = cached;
                good = true;
            } else {
                result = "Looking up…".into();
                job = Some(start(
                    profile.clone(),
                    text.into(),
                    source.clone(),
                    target.clone(),
                ));
            }
            requested = Some(current.clone());
        }
        if let Some(done) = job.as_ref().and_then(|j| j.receiver.try_recv().ok()) {
            job.take();
            match done {
                Ok(value) => {
                    result = value;
                    good = true;
                    CACHE.with(|c| {
                        let mut c = c.borrow_mut();
                        c.push_back((current, result.clone()));
                        while c.len() > 32 {
                            c.pop_front();
                        }
                    });
                }
                Err(e) => {
                    result = e.to_string();
                    good = false;
                }
            }
        }
        let actions = [
            "[P Connection]",
            if profile.provider == Provider::FreeDictionary {
                "[L English only]"
            } else {
                "[L Target language]"
            },
            if profile.provider == Provider::FreeDictionary {
                "[S English only]"
            } else {
                "[S Source language]"
            },
            "[Y Copy]",
            "[N Save note]",
            "[R Retry]",
            "[Esc Back]",
        ];
        let mut buttons = vec![];
        let mut max_scroll = 0;
        term.draw(|f| {
            underneath(f);
            let area = centered(f.area(), 88, 24);
            crate::overlay::paint(f, area, |f| {
                f.render_widget(Clear, area);
                let block = appearance::block()
                    .title(format!(" {} · {} → {} ", profile.name, source, target));
                let inner = block.inner(area);
                f.render_widget(block, area);
                let (rects, height) = crate::reader::controls::layout(inner, &actions);
                buttons = rects;
                let content = Rect::new(
                    inner.x + 1,
                    inner.y + 1,
                    inner.width.saturating_sub(2),
                    inner.height.saturating_sub(height + 3),
                );
                let body = format!("{}\n\n{}", text, result);
                let paragraph = Paragraph::new(body).wrap(Wrap { trim: false });
                max_scroll = paragraph
                    .line_count(content.width)
                    .saturating_sub(usize::from(content.height))
                    .min(u16::MAX as usize) as u16;
                offset = offset.min(max_scroll);
                f.render_widget(paragraph.scroll((offset, 0)), content);
                if content.bottom() < inner.bottom() {
                    f.render_widget(
                        Paragraph::new(status.as_str()).dim(),
                        Rect::new(inner.x, content.bottom(), inner.width, 1),
                    );
                }
                for (s, r) in actions.iter().zip(&buttons) {
                    f.render_widget(Paragraph::new(*s).cyan(), *r);
                }
                crate::settings::paint_dialog(f);
            });
        })?;
        let Some(e) =
            crate::settings::read_event_for(job.as_ref().map(|_| Duration::from_millis(60)))?
        else {
            continue;
        };
        let code = match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => k.code,
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => buttons
                .iter()
                .position(|r| r.contains(Position::new(m.column, m.row)))
                .map(|i| {
                    [
                        KeyCode::Char('p'),
                        KeyCode::Char('l'),
                        KeyCode::Char('s'),
                        KeyCode::Char('y'),
                        KeyCode::Char('n'),
                        KeyCode::Char('r'),
                        KeyCode::Esc,
                    ][i]
                })
                .unwrap_or(KeyCode::Null),
            Event::Mouse(m) if m.kind == MouseEventKind::ScrollDown => KeyCode::Down,
            Event::Mouse(m) if m.kind == MouseEventKind::ScrollUp => KeyCode::Up,
            _ => KeyCode::Null,
        };
        match code {
            KeyCode::Esc => return Ok(None),
            KeyCode::Down | KeyCode::Char('j') => offset = offset.saturating_add(1).min(max_scroll),
            KeyCode::Up | KeyCode::Char('k') => offset = offset.saturating_sub(1),
            KeyCode::PageDown => offset = offset.saturating_add(10).min(max_scroll),
            KeyCode::PageUp => offset = offset.saturating_sub(10),
            KeyCode::Char('p' | 'P') => {
                if let Some(i) = crate::prompt::choose(
                    term,
                    "Choose connection",
                    &profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
                    index,
                )? && i != index
                {
                    index = i;
                    target = if profiles[i].target_language.is_empty() {
                        default_target.clone()
                    } else {
                        profiles[i].target_language.clone()
                    };
                }
            }
            KeyCode::Char('l' | 'L' | 's' | 'S')
                if profile.provider == Provider::FreeDictionary =>
            {
                status = "This dictionary contains English definitions. P chooses a different connection.".into();
            }
            KeyCode::Char('l' | 'L') => {
                if let Some(s) = settings::language(term, &target, false)? {
                    target = s;
                }
            }
            KeyCode::Char('s' | 'S') => {
                if let Some(s) = settings::language(term, &source, true)? {
                    source = s;
                }
            }
            KeyCode::Char('y' | 'Y') if good => {
                status = crate::reader::clipboard::copy(&result)?.into()
            }
            KeyCode::Char('n' | 'N') if good => {
                return Ok(Some(format!(
                    "{} ({} → {})\n\n{}",
                    profile.name, source, target, result
                )));
            }
            KeyCode::Char('r' | 'R') => {
                CACHE.with(|c| {
                    c.borrow_mut()
                        .retain(|(k, _)| Some(k) != requested.as_ref())
                });
                requested = None;
            }
            _ => (),
        }
    }
}
