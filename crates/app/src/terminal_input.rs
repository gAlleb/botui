//! Keyboard input and terminal replies share one byte stream. Keep the same
//! decoder across reads, image queries and frame redraws.
mod graphics_probe;
mod graphics_replies;
mod keyboard;

use crate::shell_theme::TerminalPaper;
use crossterm::event::Event;
use graphics_replies::GraphicsReplies;
use keyboard::Keyboard;
use nix::{
    errno::Errno,
    poll::{PollFd, PollFlags, PollTimeout, poll},
    unistd,
};
use ratatui::style::Color;
use ratatui_image::picker::{Picker, cap_parser::QueryStdioOptions};
use std::{
    cell::{Cell, RefCell},
    fs::File,
    io::{self, IsTerminal, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    thread::LocalKey,
    time::{Duration, Instant},
};

const PERIOD: Duration = Duration::from_secs(1);
const ESCAPE_WAIT: Duration = Duration::from_millis(25);

thread_local! { static INPUT: RefCell<Option<Input>> = const { RefCell::new(None) }; }
struct Input {
    tty: File,
    resize_signal: UnixStream,
    registration: signal_hook::SigId,
    replies: GraphicsReplies,
    keyboard: Keyboard,
    graphics_requested: bool,
    escape_deadline: Option<Instant>,
    next_color_query: Option<Instant>,
    unanswered: u8,
}
fn with_input<T>(f: impl FnOnce(&mut Input) -> io::Result<T>) -> io::Result<T> {
    INPUT.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(Input::new()?);
        }
        f(slot.as_mut().unwrap())
    })
}

pub(crate) fn graphics(options: QueryStdioOptions) -> Option<Picker> {
    with_input(|input| {
        input.graphics_requested = true;
        Ok(graphics_probe::query(
            input.tty.as_fd(),
            options,
            &mut input.replies,
            |bytes| input.keyboard.feed(bytes),
        ))
    })
    .ok()
    .flatten()
}

pub(crate) fn read(
    paper: &'static LocalKey<Cell<TerminalPaper>>,
    timeout: Option<Duration>,
    watch: bool,
) -> io::Result<Option<Event>> {
    let deadline = timeout.map(|t| Instant::now() + t);
    with_input(|input| input.read(paper, deadline, watch))
}

impl Input {
    fn new() -> io::Result<Self> {
        let tty = if io::stdin().is_terminal() {
            File::from(io::stdin().as_fd().try_clone_to_owned()?)
        } else {
            File::open("/dev/tty")?
        };
        let (resize_signal, writer) = UnixStream::pair()?;
        let registration =
            signal_hook::low_level::pipe::register(signal_hook::consts::SIGWINCH, writer)?;
        Ok(Self {
            tty,
            resize_signal,
            registration,
            replies: Default::default(),
            keyboard: Default::default(),
            graphics_requested: false,
            escape_deadline: None,
            next_color_query: None,
            unanswered: 0,
        })
    }

    fn read(
        &mut self,
        paper: &'static LocalKey<Cell<TerminalPaper>>,
        deadline: Option<Instant>,
        watch: bool,
    ) -> io::Result<Option<Event>> {
        // A short caller timeout can expire before a split Escape sequence is
        // complete. Keep its deadline across calls to read().
        loop {
            if let Some(color) = self.replies.take_background()
                && self.unanswered > 0
            {
                self.unanswered = 0;
                let mut p = paper.get();
                if p.bg != Some(color) {
                    p.bg = Some(color);
                    paper.set(p);
                    self.queue_resize()?;
                }
            }
            if let Some(event) = self.keyboard.queued.pop_front() {
                return Ok(Some(event));
            }
            let now = Instant::now();
            if !watch || paper.get().bg.is_none() {
                self.next_color_query = None;
            } else if self.next_color_query.is_none_or(|at| at <= now) {
                let mut out = io::stdout().lock();
                out.write_all(b"\x1b]11;?\x07")?;
                out.flush()?;
                self.unanswered = self.unanswered.saturating_add(1);
                self.next_color_query = Some(
                    now + if self.unanswered > 3 {
                        Duration::from_secs(20)
                    } else {
                        PERIOD
                    },
                );
            }
            let escape = self.replies.escape_pending() || self.keyboard.escape_pending();
            if escape {
                self.escape_deadline.get_or_insert(now + ESCAPE_WAIT);
            } else {
                self.escape_deadline = None;
            }
            let wake = deadline
                .into_iter()
                .chain(self.next_color_query)
                .chain(self.escape_deadline)
                .min();
            let timeout = wake.map(|until| {
                PollTimeout::try_from(
                    until
                        .saturating_duration_since(now)
                        .as_millis()
                        .saturating_add(1),
                )
                .unwrap_or(PollTimeout::MAX)
            });
            if self.wait_for_input(timeout)? {
                continue;
            }
            let now = Instant::now();
            if self.escape_deadline.is_some_and(|at| now >= at) {
                let mut bytes = Vec::new();
                self.replies.finish(&mut bytes);
                self.keyboard.feed(&bytes);
                self.keyboard.flush_escape();
                self.escape_deadline = None;
                continue;
            }
            if deadline.is_some_and(|d| now >= d) {
                return Ok(None);
            }
        }
    }

    fn wait_for_input(&mut self, timeout: Option<PollTimeout>) -> io::Result<bool> {
        let mut fds = [
            PollFd::new(self.tty.as_fd(), PollFlags::POLLIN),
            PollFd::new(self.resize_signal.as_fd(), PollFlags::POLLIN),
        ];
        match poll(&mut fds, timeout) {
            Err(Errno::EINTR) => return Ok(true),
            Err(e) => return Err(e.into()),
            Ok(0) => return Ok(false),
            Ok(_) => {}
        }
        let ready = fds.map(|fd| !fd.revents().unwrap_or(PollFlags::POLLERR).is_empty());
        let mut buf = [0; 4096];
        if ready[1] {
            unistd::read(&self.resize_signal, &mut buf)?;
            self.queue_resize()?;
        }
        if ready[0] {
            let size = unistd::read(&self.tty, &mut buf)?;
            if size == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            if self.escape_deadline.take().is_some()
                && self.replies.lone_escape()
                && !matches!(buf[0], b'[' | b']' | b'_' | b'O')
            {
                // Esc followed by a separate write (e.g. Space, m) is a key
                // of its own. A split CSI/OSC prefix can still continue here.
                let mut escape = Vec::new();
                self.replies.finish(&mut escape);
                self.keyboard.feed(&escape);
                self.keyboard.flush_escape();
            }
            let mut bytes = Vec::with_capacity(size);
            // A slow emulator can deliver graphics replies after the picker timed out.
            let mut late_replies = Vec::new();
            for &byte in &buf[..size] {
                self.replies
                    .push(byte, self.graphics_requested, &mut bytes, &mut late_replies);
            }
            self.keyboard.feed(&bytes);
        }
        Ok(true)
    }

    fn queue_resize(&mut self) -> io::Result<()> {
        let (w, h) = crossterm::terminal::size()?;
        self.keyboard.queued.push_back(Event::Resize(w, h));
        Ok(())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.registration);
    }
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
