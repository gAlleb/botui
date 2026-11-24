//! Publish each completed frame atomically when the emulator supports DEC 2026.
use crossterm::{
    execute, queue,
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};
use std::{
    cell::Cell,
    io::{self, Write},
    rc::Rc,
};

pub(crate) struct AppTerminal {
    terminal: Terminal<CrosstermBackend<FrameWriter<io::Stdout>>>,
    sixel: SixelFrame,
    preparing: Rc<Cell<bool>>,
}

/// Crossterm flushes each resize clear. Hold those few control bytes in the
/// regular output buffer until Ratatui has the replacement cells to send.
struct FrameWriter<W: Write> {
    output: io::BufWriter<W>,
    preparing: Rc<Cell<bool>>,
}

impl<W: Write> Write for FrameWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.output.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.preparing.get() {
            return Ok(());
        }
        self.output.flush()
    }
}
#[derive(Default)]
struct SixelFrame {
    skipped: Vec<bool>,
    phase: bool,
}

pub(crate) struct Update;
impl Drop for Update {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), EndSynchronizedUpdate);
    }
}

impl AppTerminal {
    pub fn new() -> io::Result<Self> {
        let preparing = Rc::new(Cell::new(false));
        let output = FrameWriter {
            output: io::BufWriter::with_capacity(64 * 1024, io::stdout()),
            preparing: preparing.clone(),
        };
        Terminal::new(CrosstermBackend::new(output)).map(|terminal| {
            crate::cover::kitty_lifetime::open();
            Self {
                terminal,
                sixel: SixelFrame::default(),
                preparing,
            }
        })
    }

    pub fn size(&self) -> io::Result<ratatui::layout::Size> {
        self.terminal.size()
    }

    pub fn draw(&mut self, render: impl FnOnce(&mut Frame)) -> io::Result<()> {
        self.draw_frame(render).map(|_| ())
    }

    pub(crate) fn begin_resize(
        &mut self,
        size: ratatui::layout::Size,
    ) -> io::Result<Option<Update>> {
        if self.terminal.current_buffer_mut().area.as_size() == size {
            return Ok(None);
        }
        execute!(io::stdout(), BeginSynchronizedUpdate)?;
        Ok(Some(Update))
    }

    pub fn draw_frame(
        &mut self,
        render: impl FnOnce(&mut Frame),
    ) -> io::Result<ratatui::CompletedFrame<'_>> {
        self.publish(render, &mut None)
    }

    /// Sixel stores pixels directly on the terminal. If a pending background
    /// image arrives underneath the dialog, publish its raster before the dialog
    /// masks the image's first cell. Both passes share one synchronized update.
    pub fn draw_layered(
        &mut self,
        refresh_background: bool,
        mut background: impl FnMut(&mut Frame),
        overlay: impl FnOnce(&mut Frame),
    ) -> io::Result<()> {
        let mut update = None;
        if refresh_background && crate::cover::sixel_active() {
            self.publish(&mut background, &mut update)?;
        }
        self.publish(
            |frame| {
                background(frame);
                overlay(frame);
            },
            &mut update,
        )?;
        Ok(())
    }

    pub(crate) fn publish(
        &mut self,
        render: impl FnOnce(&mut Frame),
        update: &mut Option<Update>,
    ) -> io::Result<ratatui::CompletedFrame<'_>> {
        // Ratatui flushes a screen clear before calling render on a resize.
        // Queue the opening marker on the same buffered writer so that clear
        // is part of the frame. Ordinary frames can still prepare images and
        // query the terminal before the first backend flush.
        if update.is_none() {
            queue!(self.terminal.backend_mut(), BeginSynchronizedUpdate)?;
            *update = Some(Update);
        }
        crate::cover::kitty_lifetime::take_retired().write(self.terminal.backend_mut())?;
        let sixel = &mut self.sixel;
        let preparing = &self.preparing;
        preparing.set(true);
        let result = self.terminal.draw(|frame| {
            render(frame);
            if crate::cover::sixel_active() {
                stabilize_sixel(frame.buffer_mut(), sixel);
            }
            preparing.set(false);
        });
        // autoresize can also fail before the render callback is reached.
        preparing.set(false);
        result
    }
}

impl Drop for AppTerminal {
    fn drop(&mut self) {
        let _ = crate::cover::kitty_lifetime::close(&mut io::stdout().lock());
    }
}

fn stabilize_sixel(buffer: &mut ratatui::buffer::Buffer, state: &mut SixelFrame) {
    use ratatui::{
        buffer::CellDiffOption,
        style::{Color, Modifier},
    };
    let skipped: Vec<_> = buffer
        .content
        .iter()
        .map(|c| c.diff_option == CellDiffOption::Skip)
        .collect();
    // Closing or moving an overlay reveals image cells. Sixel has no retained
    // image ID to reveal, so reissue the cached pixels once in that case.
    if skipped
        .iter()
        .enumerate()
        .any(|(i, &skip)| skip && !state.skipped.get(i).copied().unwrap_or(false))
    {
        state.phase = !state.phase;
    }
    state.skipped = skipped;
    for cell in &mut buffer.content {
        if matches!(cell.diff_option, CellDiffOption::ForcedWidth(_))
            && cell.symbol().contains("\x1bP")
        {
            // This cell carries an opaque pixel transfer, not visible text.
            // A change to surrounding paper/ink must not resend megabytes of
            // identical Sixel data and overwrite an unchanged popup.
            cell.fg = Color::Reset;
            cell.bg = Color::Reset;
            cell.modifier = Modifier::empty();
            if state.phase {
                cell.set_symbol(&format!("{}\x1b[0m", cell.symbol()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        buffer::{Buffer, CellDiffOption},
        layout::Rect,
        style::Color,
    };
    #[test]
    fn resize_clear_is_buffered_until_replacement_cells_are_ready() {
        use ratatui::backend::Backend;
        use ratatui::buffer::Cell as ScreenCell;

        let preparing = Rc::new(Cell::new(true));
        let mut output = FrameWriter {
            output: io::BufWriter::new(Vec::new()),
            preparing: preparing.clone(),
        };
        CrosstermBackend::new(&mut output).clear().unwrap();
        assert!(output.output.get_ref().is_empty());
        preparing.set(false);
        let mut backend = CrosstermBackend::new(&mut output);
        backend
            .draw([(0, 0, &ScreenCell::new("X"))].into_iter())
            .unwrap();
        Backend::flush(&mut backend).unwrap();
        let bytes = output.output.get_ref();
        assert!(bytes.starts_with(b"\x1b[2J"));
        assert!(bytes.contains(&b'X'));
    }
    #[test]
    fn closing_a_popup_reissues_pixels_once_without_reencoding() {
        let mut image = Buffer::empty(Rect::new(0, 0, 3, 1));
        image[(0, 0)]
            .set_symbol("\x1bPqIMAGE\x1b\\")
            .set_diff_option(CellDiffOption::ForcedWidth(
                std::num::NonZeroU16::new(1).unwrap(),
            ));
        image[(1, 0)].set_diff_option(CellDiffOption::Skip);
        image[(2, 0)].set_diff_option(CellDiffOption::Skip);
        let mut state = SixelFrame::default();
        let mut first = image.clone();
        stabilize_sixel(&mut first, &mut state);
        let mut popup = image.clone();
        popup[(1, 0)]
            .set_symbol("X")
            .set_diff_option(CellDiffOption::AlwaysUpdate);
        stabilize_sixel(&mut popup, &mut state);
        assert!(!first.diff(&popup).iter().any(|(x, _, _)| *x == 0));
        let mut restored = image.clone();
        stabilize_sixel(&mut restored, &mut state);
        assert!(popup.diff(&restored).iter().any(|(x, _, _)| *x == 0));
        stabilize_sixel(&mut image, &mut state);
        assert!(restored.diff(&image).is_empty());
    }
    #[test]
    fn paper_change_keeps_sixel_transfer_out_of_the_diff() {
        let mut before = Buffer::empty(Rect::new(0, 0, 3, 1));
        before[(0, 0)]
            .set_symbol("\x1bPqIMAGE\x1b\\")
            .set_diff_option(CellDiffOption::ForcedWidth(
                std::num::NonZeroU16::new(1).unwrap(),
            ));
        let mut after = before.clone();
        for cell in &mut after.content {
            cell.bg = Color::White;
            cell.fg = Color::Black;
        }
        let mut state = SixelFrame::default();
        stabilize_sixel(&mut before, &mut state);
        stabilize_sixel(&mut after, &mut state);
        let changed: Vec<_> = before.diff(&after).into_iter().map(|(x, _, _)| x).collect();
        assert_eq!(changed, vec![1, 2]);
    }
}
