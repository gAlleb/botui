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
fn caption(profile: &Profile, source: &str, target: &str) -> String {
    match (profile.uses_language(true), profile.uses_language(false)) {
        (true, true) => format!("{} · {} → {}", profile.name, source, target),
        (true, false) => format!("{} · from {}", profile.name, source),
        (false, true) => format!("{} · into {}", profile.name, target),
        (false, false) => profile.name.clone(),
    }
}
fn actions(profile: &Profile) -> Vec<(&'static str, KeyCode)> {
    let mut actions = vec![("[P Connection]", KeyCode::Char('p'))];
    if profile.uses_language(false) {
        actions.push(("[L Target language]", KeyCode::Char('l')));
    }
    if profile.uses_language(true) {
        actions.push(("[S Source language]", KeyCode::Char('s')));
    }
    actions.extend([
        ("[Y Copy]", KeyCode::Char('y')),
        ("[N Save note]", KeyCode::Char('n')),
        ("[R Retry]", KeyCode::Char('r')),
        ("[Esc Back]", KeyCode::Esc),
    ]);
    actions
}
fn panel(screen: Rect, body: &Paragraph, labels: &[&str]) -> Rect {
    let limit = centered(screen, 88, 24);
    let inner = appearance::block().inner(limit);
    let (_, button_height) = crate::reader::controls::layout(inner, labels);
    let lines = body
        .line_count(inner.width.saturating_sub(2))
        .min(u16::MAX as usize) as u16;
    let chrome = limit.height.saturating_sub(inner.height);
    let height = lines
        .saturating_add(button_height + 3 + chrome)
        .clamp(10, 24);
    centered(screen, 88, height)
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
        let actions = actions(profile);
        let labels: Vec<_> = actions.iter().map(|(label, _)| *label).collect();
        let body = Paragraph::new(format!("{}\n\n{}", text, result)).wrap(Wrap { trim: false });
        let mut buttons = vec![];
        let mut max_scroll = 0;
        term.draw(|f| {
            underneath(f);
            let area = panel(f.area(), &body, &labels);
            crate::overlay::modal(f, area, |f| {
                f.render_widget(Clear, area);
                let block =
                    appearance::block().title(format!(" {} ", caption(profile, &source, &target)));
                let inner = block.inner(area);
                f.render_widget(block, area);
                let (rects, height) = crate::reader::controls::layout(inner, &labels);
                buttons = rects;
                let content = Rect::new(
                    inner.x + 1,
                    inner.y + 1,
                    inner.width.saturating_sub(2),
                    inner.height.saturating_sub(height + 3),
                );
                max_scroll = body
                    .line_count(content.width)
                    .saturating_sub(usize::from(content.height))
                    .min(u16::MAX as usize) as u16;
                offset = offset.min(max_scroll);
                f.render_widget(body.scroll((offset, 0)), content);
                if content.bottom() < inner.bottom() {
                    f.render_widget(
                        Paragraph::new(status.as_str()).dim(),
                        Rect::new(inner.x, content.bottom(), inner.width, 1),
                    );
                }
                for ((s, _), r) in actions.iter().zip(&buttons) {
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
        let code = match crate::keymap::Scope::Dialogs.event(e) {
            Event::Key(k) if k.kind != KeyEventKind::Release => k.code,
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => buttons
                .iter()
                .position(|r| r.contains(Position::new(m.column, m.row)))
                .map(|i| actions[i].1)
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
            KeyCode::Char('l' | 'L') if !profile.uses_language(false) => {
                status = "This connection does not use a target-language parameter.".into();
            }
            KeyCode::Char('s' | 'S') if !profile.uses_language(true) => {
                status = "This connection does not use a source-language parameter.".into();
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
                    "{}\n\n{}",
                    caption(profile, &source, &target),
                    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_controls_and_saved_caption_follow_the_connection_parameters() {
        for (args, languages, title) in [
            (vec!["{text}"], vec![], "Fixture"),
            (vec!["{text}", "{source}"], vec!['s'], "Fixture · from en"),
            (vec!["{text}", "{target}"], vec!['l'], "Fixture · into ru"),
            (
                vec!["{text}", "{source?}", "{target}"],
                vec!['l', 's'],
                "Fixture · en → ru",
            ),
        ] {
            let profile = Profile {
                name: "Fixture".into(),
                command: args.into_iter().map(str::to_owned).collect(),
                ..Default::default()
            };
            assert_eq!(caption(&profile, "en", "ru"), title);
            let buttons = actions(&profile);
            let expected: Vec<_> = [KeyCode::Char('p')]
                .into_iter()
                .chain(languages.into_iter().map(KeyCode::Char))
                .chain([
                    KeyCode::Char('y'),
                    KeyCode::Char('n'),
                    KeyCode::Char('r'),
                    KeyCode::Esc,
                ])
                .collect();
            assert_eq!(
                buttons.iter().map(|(_, key)| *key).collect::<Vec<_>>(),
                expected
            );
            for (label, key) in buttons {
                if let KeyCode::Char(key) = key {
                    assert!(label.starts_with(&format!("[{} ", key.to_ascii_uppercase())));
                }
            }
        }
    }

    #[test]
    fn short_lookup_leaves_the_page_visible_and_long_answers_keep_a_scrollable_viewport() {
        let actions = actions(&Profile::default());
        let labels: Vec<_> = actions.iter().map(|(label, _)| *label).collect();
        let short = Paragraph::new("word\n\nShort definition.").wrap(Wrap { trim: false });
        let long = Paragraph::new("Long definition.\n".repeat(80)).wrap(Wrap { trim: false });
        let screen = Rect::new(3, 5, 120, 40);
        assert_eq!(panel(screen, &short, &labels).height, 10);
        assert_eq!(panel(screen, &long, &labels).height, 24);
        for (width, height) in [(120, 40), (60, 16), (24, 10), (4, 3), (0, 0)] {
            let screen = Rect::new(3, 5, width, height);
            for body in [&short, &long] {
                let area = panel(screen, body, &labels);
                assert_eq!(area.intersection(screen), area);
            }
        }
    }
}
