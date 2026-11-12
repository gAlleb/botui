//! Publish each completed frame atomically when the emulator supports DEC 2026.
use crossterm::{
    execute,
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};
use std::io;

pub(crate) struct AppTerminal(
    Terminal<CrosstermBackend<io::BufWriter<io::Stdout>>>,
    SixelFrame,
);
#[derive(Default)]
struct SixelFrame {
    skipped: Vec<bool>,
    phase: bool,
}

struct Update;
impl Drop for Update {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), EndSynchronizedUpdate);
    }
}

impl AppTerminal {
    pub fn new() -> io::Result<Self> {
        let output = io::BufWriter::with_capacity(64 * 1024, io::stdout());
        Terminal::new(CrosstermBackend::new(output))
            .map(|terminal| Self(terminal, SixelFrame::default()))
    }

    pub fn size(&self) -> io::Result<ratatui::layout::Size> {
        self.0.size()
    }

    pub fn draw(&mut self, render: impl FnOnce(&mut Frame)) -> io::Result<()> {
        self.draw_frame(render).map(|_| ())
    }

    pub fn draw_frame(
        &mut self,
        render: impl FnOnce(&mut Frame),
    ) -> io::Result<ratatui::CompletedFrame<'_>> {
        let mut update = None;
        let sixel = &mut self.1;
        let frame = self.0.try_draw(|frame| {
            // Capability queries and image preparation happen before holding
            // the display. Only publishing the finished frame is synchronized.
            render(frame);
            if crate::cover::sixel_active() {
                stabilize_sixel(frame.buffer_mut(), sixel);
            }
            update = Some(Update);
            execute!(io::stdout(), BeginSynchronizedUpdate)
        })?;
        drop(update);
        Ok(frame)
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
