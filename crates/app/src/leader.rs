//! Space is a prefix, not a timed chord. Only its hint has a delay.
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
    widgets::{Clear, Paragraph},
};
use std::time::{Duration, Instant};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Text,
    Markdown,
    Pdf,
    Images,
    Shelf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Key(KeyCode),
    Dictionary,
    Translate,
}
impl Command {
    pub fn event(self) -> Option<Event> {
        match self {
            Self::Key(code) => Some(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))),
            _ => None,
        }
    }
}
#[derive(Clone, Copy)]
struct Entry {
    key: char,
    label: &'static str,
    command: Option<Command>,
    group: &'static str,
}
fn key(c: char, label: &'static str, code: KeyCode) -> Entry {
    Entry {
        key: c,
        label,
        command: Some(Command::Key(code)),
        group: "",
    }
}
fn group(c: char, label: &'static str, g: &'static str) -> Entry {
    Entry {
        key: c,
        label,
        command: None,
        group: g,
    }
}
/// Ordinary reading keeps navigation and standard function keys, not legacy letters.
pub fn pdf_reading_event(event: &Event) -> bool {
    reading_event(event)
        || matches!(event, Event::Key(k)
        if matches!(k.code, KeyCode::Char('+' | '=' | '-' | '−'))
        && !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT))
}
pub fn reading_event(event: &Event) -> bool {
    let Event::Key(k) = event else {
        return true;
    };
    if k.modifiers.contains(KeyModifiers::CONTROL) {
        return k.code == KeyCode::Char('c');
    }
    matches!(
        k.code,
        KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Enter
            | KeyCode::Esc
            | KeyCode::F(1..=5)
            | KeyCode::F(9)
            | KeyCode::Char('h' | 'j' | 'k' | 'l' | 'q' | 'Q' | 'n' | 'N' | '1' | '2' | '3')
    )
}
fn entries(context: Context, path: &str) -> Vec<Entry> {
    use KeyCode::*;
    if context == Context::Shelf {
        return vec![
            key('s', "Settings", F(2)),
            key('?', "Help", F(1)),
            key('v', "Grid / list", Char('v')),
            key('f', "Find books", Char('/')),
            key('q', "Back", Char('q')),
        ];
    }
    let mut items = match path {
        "lookup" => vec![
            Entry {
                key: 'd',
                label: "Word definitions",
                command: Some(Command::Dictionary),
                group: "",
            },
            Entry {
                key: 't',
                label: "Translate passage",
                command: Some(Command::Translate),
                group: "",
            },
        ],
        "reading" if matches!(context, Context::Text | Context::Markdown) => vec![
            key('i', "Links & illustrations", Char('I')),
            key('c', "Open cover", Char('C')),
            key('s', "Reading statistics", Char('S')),
            key('f', "Visible links", F(6)),
            key('[', "Jump back", Char(',')),
            key(']', "Jump forward", Char('.')),
        ],
        "reading" => vec![
            key('p', "Go to page", Char('P')),
            key('+', "Zoom in", Char('+')),
            key('-', "Zoom out", Char('-')),
            key('0', "Fit page", Char('0')),
            key('r', "Rotate clockwise", Char('r')),
            key('R', "Rotate counterclockwise", Char('R')),
            key('s', "Reading statistics", Char('S')),
            key('c', "Open cover", Char('C')),
            key('[', "Jump back", Char(',')),
            key(']', "Jump forward", Char('.')),
        ],
        "notes" if matches!(context, Context::Text | Context::Markdown) => vec![
            key('a', "All highlights & notes", Char('A')),
            key('n', "Comment", Char('N')),
            key('h', "Highlight", Char('H')),
            key('y', "Copy", Char('Y')),
            key('v', "Show / hide note body", F(8)),
            key('e', "Export Markdown", Char('E')),
            key('s', "BookOrbit sync", F(10)),
        ],
        "notes" => vec![
            key('p', "Page note", Char('I')),
            key('a', "All highlights & notes", Char('A')),
            key('n', "Comment", Char('N')),
            key('h', "Highlight", Char('H')),
            key('y', "Copy", Char('Y')),
            key('e', "Export Markdown", Char('E')),
            key('s', "BookOrbit sync", F(10)),
        ],
        "layout" => vec![
            key('1', "Single page", Char('1')),
            key('2', "Two-page spread", Char('2')),
            key('3', "Continuous scroll", Char('3')),
            key('w', "Width & spacing", Char('W')),
            key('f', "Opaque background", Char('F')),
            key('u', "Borders", Char('U')),
        ],
        "bookmarks" => vec![
            key('a', "Add bookmark", Char('B')),
            key('l', "List bookmarks", Char('G')),
        ],
        _ => vec![
            key('c', "Text cursor / Vim", F(7)),
            key('m', "Reading menu", Char('M')),
            key(
                'o',
                "Contents",
                if matches!(context, Context::Pdf | Context::Images) {
                    Tab
                } else {
                    Char('O')
                },
            ),
            key('z', "Zen mode", Char('Z')),
            key('p', "Zen progress", Char('%')),
            key('f', "Find in book", Char('/')),
            group('r', "Reading tools", "reading"),
            group('b', "Bookmarks", "bookmarks"),
            group('a', "Annotations", "notes"),
            group('d', "Dictionary / translation", "lookup"),
            group('l', "Page layout", "layout"),
            key(
                't',
                "Theme / page colors",
                if matches!(context, Context::Pdf | Context::Images) {
                    Char('V')
                } else {
                    Char('T')
                },
            ),
            key('s', "Settings", F(2)),
            key('?', "Help", F(1)),
            key('q', "Back to library", Char('Q')),
        ],
    };
    if context == Context::Markdown && path == "reading" {
        items.insert(0, key('v', "Formatted / source", F(9)));
    }
    if context == Context::Images {
        if path == "notes" {
            items.retain(|e| !matches!(e.key, 'n' | 'h' | 'y'));
            if let Some(e) = items.iter_mut().find(|e| e.key == 'a') {
                e.label = "All page notes";
            }
        } else if path.is_empty() {
            items.retain(|e| !matches!(e.key, 'c' | 'f' | 'd'));
        }
    }
    items
}
#[derive(Default)]
pub struct Leader {
    started: Option<Instant>,
    path: &'static str,
    shown: bool,
    buttons: Vec<(Rect, Entry)>,
    delay: u16,
}
impl Leader {
    pub fn new() -> Self {
        Self {
            delay: crate::settings::Settings::load()
                .ok()
                .and_then(|s| s.leader_delay_ms)
                .unwrap_or(150)
                .min(1000),
            ..Self::default()
        }
    }
    pub fn active(&self) -> bool {
        self.started.is_some()
    }
    pub fn timeout(&self) -> Option<Duration> {
        self.started
            .filter(|_| !self.shown)
            .map(|s| Duration::from_millis(self.delay.into()).saturating_sub(s.elapsed()))
    }
    pub fn input(&mut self, e: &Event, context: Context) -> Option<Option<Command>> {
        let code = match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => k.code,
            Event::Mouse(m)
                if self.active() && m.kind == MouseEventKind::Down(MouseButton::Left) =>
            {
                self.buttons
                    .iter()
                    .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
                    .map(|(_, e)| KeyCode::Char(e.key))
                    .unwrap_or(KeyCode::Null)
            }
            _ => return self.active().then_some(None),
        };
        if !self.active() {
            if code == KeyCode::Char(' ') {
                self.started = Some(Instant::now());
                self.path = "";
                self.shown = false;
                return Some(None);
            }
            return None;
        }
        if !self.path.is_empty()
            && matches!(
                code,
                KeyCode::Char('q' | 'Q') | KeyCode::Esc | KeyCode::Backspace
            )
        {
            self.path = "";
            self.buttons.clear();
            self.shown = false;
            self.started = Some(Instant::now() - Duration::from_millis(self.delay.into()));
            return Some(None);
        }
        let selected = entries(context, self.path)
            .into_iter()
            .find(|e| code == KeyCode::Char(e.key));
        if let Some(e) = selected {
            if let Some(command) = e.command {
                self.started = None;
                return Some(Some(command));
            }
            self.path = e.group;
            self.buttons.clear();
            self.shown = false;
            self.started = Some(Instant::now() - Duration::from_millis(self.delay.into()));
            return Some(None);
        }
        self.started = None;
        Some(None)
    }
    pub fn draw(&mut self, f: &mut Frame, context: Context) {
        if !self
            .started
            .is_some_and(|s| s.elapsed() >= Duration::from_millis(self.delay.into()))
        {
            return;
        }
        self.shown = true;
        self.buttons.clear();
        let area = f.area();
        let entries = entries(context, self.path);
        let cols = (area.width / 30).max(1);
        let height = (entries.len() as u16).div_ceil(cols) + 2;
        let panel = Rect::new(
            area.x,
            area.bottom().saturating_sub(height),
            area.width,
            height.min(area.height),
        );
        crate::overlay::paint(f, panel, |f| {
            f.render_widget(Clear, panel);
            let block = crate::appearance::block().title(if self.path.is_empty() {
                " Space · Esc close "
            } else {
                " Space · Q / Esc back "
            });
            let inner = block.inner(panel);
            f.render_widget(block, panel);
            let width = inner.width / cols;
            for (i, e) in entries.into_iter().enumerate() {
                let r = Rect::new(
                    inner.x + (i as u16 % cols) * width,
                    inner.y + i as u16 / cols,
                    width,
                    1,
                )
                .intersection(inner);
                if !r.is_empty() {
                    f.render_widget(
                        Paragraph::new(format!(
                            " {}  {}{}",
                            e.key,
                            e.label,
                            if e.command.is_none() { " ›" } else { "" }
                        ))
                        .cyan(),
                        r,
                    );
                    self.buttons.push((r, e));
                }
            }
            crate::settings::paint_dialog(f);
        });
    }
}
pub fn help(context: Context) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for e in entries(context, "") {
        if e.command.is_some() {
            rows.push((format!("Space {}", e.key), e.label.into()));
        } else {
            for child in entries(context, e.group) {
                rows.push((format!("Space {} {}", e.key, child.key), child.label.into()));
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submenu_back_keeps_root_open_without_dispatching_quit() {
        for context in [Context::Text, Context::Pdf] {
            for back in [KeyCode::Char('q'), KeyCode::Esc, KeyCode::Backspace] {
                let mut l = Leader::new();
                let event = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
                l.input(&event(KeyCode::Char(' ')), context);
                l.input(&event(KeyCode::Char('d')), context);
                assert_eq!(l.path, "lookup");
                assert_eq!(l.input(&event(back), context), Some(None));
                assert!(l.active());
                assert_eq!(l.path, "");
                assert_eq!(
                    l.input(&event(KeyCode::Char('c')), context),
                    Some(Some(Command::Key(KeyCode::F(7))))
                );
            }
        }
    }
    #[test]
    fn reading_has_no_legacy_command_letters() {
        for c in "vVtTbBgGaAsSzZ%[]{}/+ -".chars().filter(|c| *c != ' ') {
            assert!(
                !reading_event(&Event::Key(KeyEvent::new(
                    KeyCode::Char(c),
                    KeyModifiers::NONE
                ))),
                "{c}"
            );
        }
        for c in "hjklqQnN123".chars() {
            assert!(reading_event(&Event::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE
            ))));
        }
    }
    #[test]
    fn pdf_keeps_direct_zoom_without_enabling_legacy_letters() {
        let event = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
        for c in "+=-−".chars() {
            assert!(pdf_reading_event(&event(KeyCode::Char(c))));
        }
        assert!(!pdf_reading_event(&event(KeyCode::Char('v'))));
    }
    #[test]
    fn reader_commands_are_documented() {
        for ctx in [Context::Text, Context::Pdf] {
            for (key, _) in help(ctx) {
                for doc in [
                    include_str!("../../../README.md"),
                    include_str!("../../../README.ru.md"),
                    include_str!("../../../docs/guide.md"),
                    include_str!("../../../docs/guide.ru.md"),
                ] {
                    assert!(
                        doc.contains(&format!("`{key}`")),
                        "Undocumented command: {key}"
                    );
                }
            }
        }
    }
    #[test]
    fn fast_prefix_and_cancel() {
        let mut l = Leader::new();
        let event = |c| Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        assert_eq!(l.input(&event(' '), Context::Text), Some(None));
        assert_eq!(
            l.input(&event('c'), Context::Text),
            Some(Some(Command::Key(KeyCode::F(7))))
        );
        assert!(!l.active());
        l.input(&event(' '), Context::Text);
        assert_eq!(l.input(&event('!'), Context::Text), Some(None));
        assert!(!l.active());
    }
    #[test]
    fn translation_group() {
        let mut l = Leader::new();
        for c in [' ', 'd'] {
            assert_eq!(
                l.input(
                    &Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
                    Context::Pdf
                ),
                Some(None)
            );
        }
        assert_eq!(
            l.input(
                &Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)),
                Context::Pdf
            ),
            Some(Some(Command::Translate))
        );
    }
}
