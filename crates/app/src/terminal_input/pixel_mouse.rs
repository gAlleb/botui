//! PDF-only pixel mouse. All ordinary widgets continue to receive cell events.
use crossterm::event::Event;
use ratatui::layout::Position;
use std::{
    cell::RefCell,
    io::{self, Write},
};

#[derive(Default)]
struct State {
    size: Option<(u16, u16)>,
    last: Option<(Position, [f32; 2])>,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

pub(crate) struct Session(bool);
impl Session {
    pub(crate) fn new(pdf: bool) -> Self {
        Self(
            pdf && std::env::var_os("TMUX").is_none()
                && (std::env::var_os("KITTY_WINDOW_ID").is_some()
                    || std::env::var("TERM_PROGRAM").is_ok_and(|t| t == "ghostty")),
        )
    }
    pub(crate) fn size(&self, width: u16, height: u16) {
        if !self.0 || width == 0 || height == 0 {
            return;
        }
        if STATE.with_borrow(|s| s.size == Some((width, height))) {
            return;
        }
        super::settle_mouse_coordinates();
        STATE.with_borrow_mut(|s| {
            if s.size.is_none() {
                let mut out = io::stdout().lock();
                if out
                    .write_all(b"\x1b[?1016h")
                    .and_then(|_| out.flush())
                    .is_err()
                {
                    return;
                }
            }
            s.size = Some((width, height));
        });
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if STATE.with_borrow(|s| s.size.is_some()) {
            super::settle_mouse_coordinates();
        }
        STATE.with_borrow_mut(|s| {
            if s.size.take().is_some() {
                let mut out = io::stdout().lock();
                let _ = out.write_all(b"\x1b[?1016l").and_then(|_| out.flush());
            }
            s.last = None;
        });
    }
}
pub(super) fn clear_last() {
    STATE.with_borrow_mut(|s| s.last = None);
}
pub(super) fn normalize(event: &mut Event) {
    STATE.with_borrow_mut(|s| {
        s.last = None;
        let Event::Mouse(m) = event else { return };
        let Some((w, h)) = s.size else { return };
        let fractions = [
            f32::from(m.column % w) / f32::from(w),
            f32::from(m.row % h) / f32::from(h),
        ];
        m.column /= w;
        m.row /= h;
        s.last = Some((Position::new(m.column, m.row), fractions));
    });
}
pub(crate) fn fractions(p: Position) -> [f32; 2] {
    STATE.with_borrow(|s| {
        s.last
            .filter(|(at, _)| *at == p)
            .map_or([0.5, 0.5], |(_, f)| f)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    #[test]
    fn widgets_keep_cell_coordinates_while_pdf_keeps_subcell_position() {
        STATE.with_borrow_mut(|s| s.size = Some((12, 24)));
        let mut event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 61,
            row: 73,
            modifiers: KeyModifiers::NONE,
        });
        normalize(&mut event);
        let Event::Mouse(mouse) = event else {
            unreachable!()
        };
        assert_eq!((mouse.column, mouse.row), (5, 3));
        let fraction = fractions(Position::new(5, 3));
        assert!((fraction[0] - 1. / 12.).abs() < 0.00001);
        assert!((fraction[1] - 1. / 24.).abs() < 0.00001);
        STATE.with_borrow_mut(|s| *s = State::default());
        normalize(&mut event);
        let Event::Mouse(mouse) = event else {
            unreachable!()
        };
        assert_eq!((mouse.column, mouse.row), (5, 3));
        assert_eq!(fractions(Position::new(5, 3)), [0.5, 0.5]);
    }
}
