//! Publish each completed frame atomically when the emulator supports DEC 2026.
use crossterm::{
    execute,
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};
use std::io;

pub(crate) struct AppTerminal(Terminal<CrosstermBackend<io::BufWriter<io::Stdout>>>);

struct Update;
impl Drop for Update {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), EndSynchronizedUpdate);
    }
}

impl AppTerminal {
    pub fn new() -> io::Result<Self> {
        let output = io::BufWriter::with_capacity(64 * 1024, io::stdout());
        Terminal::new(CrosstermBackend::new(output)).map(Self)
    }

    pub fn size(&self) -> io::Result<ratatui::layout::Size> {
        self.0.size()
    }

    pub fn draw(&mut self, render: impl FnOnce(&mut Frame)) -> io::Result<()> {
        let mut update = None;
        self.0.try_draw(|frame| {
            // Capability queries and image preparation happen before holding
            // the display. Only publishing the finished frame is synchronized.
            render(frame);
            update = Some(Update);
            execute!(io::stdout(), BeginSynchronizedUpdate)
        })?;
        drop(update);
        Ok(())
    }
}
