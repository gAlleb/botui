//! The ordinary Terminal theme uses SGR defaults and ANSI slots, which recolor themselves.
//! Only explicit opaque paper needs an RGB. Subscribe to the active emulator by querying
//! OSC 11 while idle. Crossterm decodes OSC into key events: distinguish the complete,
//! bounded response here so prompts never receive its bytes, or lose an unrelated key.
use crate::shell_theme::TerminalPaper;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    io::{self, Write},
    thread::LocalKey,
    time::{Duration, Instant},
};

const PERIOD: Duration = Duration::from_secs(1);
const FRAGMENT_WAIT: Duration = Duration::from_millis(100);
thread_local! { static INPUT: RefCell<Input> = RefCell::new(Input::default()); }
#[derive(Default)]
struct Input {
    queued: VecDeque<Event>,
    next: Option<Instant>,
    queried: bool,
    unanswered: u8,
}
pub(crate) fn read(
    paper: &'static LocalKey<Cell<TerminalPaper>>,
    timeout: Option<Duration>,
    watch: bool,
) -> io::Result<Option<Event>> {
    let deadline = timeout.map(|t| Instant::now() + t);
    INPUT.with_borrow_mut(|s| s.read(paper, deadline, watch))
}
impl Input {
    fn read(
        &mut self,
        paper: &'static LocalKey<Cell<TerminalPaper>>,
        deadline: Option<Instant>,
        watch: bool,
    ) -> io::Result<Option<Event>> {
        loop {
            if let Some(e) = self.queued.pop_front() {
                return Ok(Some(e));
            }
            if !watch || paper.get().bg.is_none() {
                self.next = None;
            } else {
                let now = Instant::now();
                if self.next.is_none_or(|at| at <= now) {
                    let mut out = io::stdout().lock();
                    out.write_all(b"\x1b]11;?\x07")?;
                    out.flush()?;
                    self.queried = true;
                    self.unanswered = self.unanswered.saturating_add(1);
                    self.next = Some(
                        now + if self.unanswered > 3 {
                            Duration::from_secs(20)
                        } else {
                            PERIOD
                        },
                    );
                }
            }
            // An unchanged OSC 11 reply is consumed here. Keep the caller's original
            // deadline while waiting for a real key or a background result to repaint.
            if let Some(wake) = self.next.into_iter().chain(deadline).min()
                && !event::poll(wake.saturating_duration_since(Instant::now()))?
            {
                if deadline.is_some_and(|d| Instant::now() >= d) {
                    return Ok(None);
                }
                continue;
            }
            let first = event::read()?;
            if !self.queried || !(alt(&first, ']') || esc(&first)) {
                return Ok(Some(first));
            }
            let mut sequence = vec![first];
            let mut payload = String::new();
            let split_start = esc(&sequence[0]);
            let mut split_end = false;
            // A response is normally delivered as one write. Bound a fragmented one, including
            // Esc typed by a human, and replay every event if it is not an OSC color reply.
            let end = Instant::now() + FRAGMENT_WAIT;
            while sequence.len() < 32 && event::poll(end.saturating_duration_since(Instant::now()))?
            {
                let e = event::read()?;
                sequence.push(e);
                let e = sequence.last().unwrap();
                if split_start && sequence.len() == 2 {
                    if plain(e) == Some(']') {
                        continue;
                    }
                    break;
                }
                if alt(e, '\\') || ctrl_g(e) || (split_end && plain(e) == Some('\\')) {
                    if let Some(color) = parse(&payload) {
                        self.unanswered = 0;
                        self.queried = false;
                        let mut p = paper.get();
                        if p.bg != Some(color) {
                            p.bg = Some(color);
                            paper.set(p);
                            let (w, h) = crossterm::terminal::size()?;
                            return Ok(Some(Event::Resize(w, h)));
                        }
                        sequence.clear();
                    }
                    break;
                }
                if esc(e) && parse(&payload).is_some() {
                    split_end = true;
                    continue;
                }
                let Some(c) = plain(e) else { break };
                payload.push(c);
                if !prefix(&payload) || split_end {
                    break;
                }
            }
            self.queued.extend(sequence);
        }
    }
}
fn plain(e: &Event) -> Option<char> {
    if let Event::Key(KeyEvent {
        code: KeyCode::Char(c),
        modifiers,
        ..
    }) = e
        && (*modifiers - KeyModifiers::SHIFT).is_empty()
    {
        Some(*c)
    } else {
        None
    }
}
fn alt(e: &Event, c: char) -> bool {
    matches!(e,Event::Key(k) if k.code==KeyCode::Char(c) && k.modifiers==KeyModifiers::ALT)
}
fn esc(e: &Event) -> bool {
    matches!(e,Event::Key(k) if k.code==KeyCode::Esc)
}
fn ctrl_g(e: &Event) -> bool {
    matches!(e,Event::Key(k) if k.code==KeyCode::Char('g') && k.modifiers==KeyModifiers::CONTROL)
}
fn prefix(s: &str) -> bool {
    "11;rgb:".starts_with(s)
        || s.strip_prefix("11;rgb:")
            .is_some_and(|v| v.len() <= 14 && v.bytes().all(|c| c.is_ascii_hexdigit() || c == b'/'))
}
fn parse(s: &str) -> Option<Color> {
    let mut channels = s.strip_prefix("11;rgb:")?.split('/');
    let mut next = || {
        let s = channels.next()?;
        if !(1..=4).contains(&s.len()) {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        let top = (1u32 << (s.len() * 4)) - 1;
        Some(((v * 255 + top / 2) / top) as u8)
    };
    let color = Color::Rgb(next()?, next()?, next()?);
    channels.next().is_none().then_some(color)
}
