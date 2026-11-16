//! Frame a stream of TTY bytes for terminput’s crossterm-compatible event parser.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;

const ESC: u8 = 0x1b;
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
const MAX_SEQUENCE: usize = 256;

#[derive(Default)]
pub(super) struct Keyboard {
    pending: Vec<u8>,
    paste: Option<Vec<u8>>,
    discarding_csi: bool,
    pub(super) queued: VecDeque<Event>,
}

impl Keyboard {
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if let Some(paste) = &mut self.paste {
                paste.push(byte);
                if byte == b'~' && paste.ends_with(PASTE_END) {
                    paste.truncate(paste.len() - PASTE_END.len());
                    let data = self.paste.take().unwrap();
                    let text = String::from_utf8(data)
                        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned());
                    self.queued.push_back(Event::Paste(text));
                }
                continue;
            }

            if byte == ESC {
                // A new Escape interrupts an incomplete input sequence. Two
                // consecutive Esc presses must survive being read together.
                if self.pending == [ESC] {
                    self.flush_escape();
                }
                self.pending.clear();
                self.pending.push(ESC);
                self.discarding_csi = false;
                continue;
            }
            let csi_final = (0x40..=0x7e).contains(&byte);
            if self.discarding_csi {
                self.discarding_csi = !csi_final;
                continue;
            }
            if self.pending.len() == MAX_SEQUENCE {
                self.pending.clear();
                self.discarding_csi = !csi_final;
                continue;
            }
            // A malformed UTF-8 prefix must not consume the next valid key.
            let character = self.pending.strip_prefix(&[ESC]).unwrap_or(&self.pending);
            if character.first().is_some_and(|b| *b >= 0x80) && byte & 0xc0 != 0x80 {
                self.pending.clear();
            }
            self.pending.push(byte);
            if self.pending == PASTE_START {
                self.pending.clear();
                self.paste = Some(Vec::new());
                continue;
            }
            // The Linux console sends ESC [[ A..E for F1..F5 and X10 adds
            // three binary bytes after its final 'M'. Other CSI sequences end
            // at their first final byte, even when the decoder rejects them.
            let csi = self.pending.strip_prefix(b"\x1b[");
            if csi.is_some_and(|body| match body {
                [] | [b'['] => true,
                [b'M', coordinates @ ..] => coordinates.len() < 3,
                [b'[', _] => false,
                _ => !csi_final,
            }) {
                continue;
            }
            if invalid_mouse_coordinates(&self.pending) {
                self.pending.clear();
                continue;
            }
            match terminput::Event::parse_from(&self.pending) {
                Ok(None) if csi.is_none() => continue,
                Ok(Some(event)) => {
                    if let Ok(event) = terminput_crossterm::to_crossterm(event) {
                        self.queued.push_back(event);
                    }
                }
                Ok(None) | Err(_) => {}
            }
            self.pending.clear();
        }
    }

    pub(super) fn escape_pending(&self) -> bool {
        matches!(self.pending.as_slice(), [ESC] | [ESC, b'[' | b'O'])
    }

    pub(super) fn flush_escape(&mut self) {
        let key = match self.pending.as_slice() {
            [ESC] => KeyEvent::from(KeyCode::Esc),
            [ESC, b'['] => KeyEvent::new(KeyCode::Char('['), KeyModifiers::ALT),
            [ESC, b'O'] => {
                KeyEvent::new(KeyCode::Char('O'), KeyModifiers::ALT | KeyModifiers::SHIFT)
            }
            _ => return,
        };
        self.queued.push_back(Event::Key(key));
        self.pending.clear();
    }
}

// SGR and urxvt coordinates are one-based. terminput 0.5 subtracts one
// from these fields without checking for zero.
fn invalid_mouse_coordinates(packet: &[u8]) -> bool {
    let Some((&final_byte, fields)) = packet.split_last() else {
        return false;
    };
    let Some(params) = fields.strip_prefix(b"\x1b[") else {
        return false;
    };
    if !matches!(final_byte, b'M' | b'm') || params.is_empty() || params[0] == b'M' {
        return false;
    }
    params.split(|&b| b == b';').skip(1).take(2).any(|axis| {
        std::str::from_utf8(axis)
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            == Some(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    #[test]
    fn fragmented_terminal_encodings_keep_their_crossterm_bindings() {
        let key = |code, modifiers| Event::Key(KeyEvent::new(code, modifiers));
        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        let shift = KeyModifiers::SHIFT;
        let click = |modifiers| {
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 9,
                row: 4,
                modifiers,
            })
        };
        for (sequence, expected) in [
            (&b"\x1b[[B"[..], key(KeyCode::F(2), none)), // Linux console
            (b"\x1b[Q", key(KeyCode::F(2), none)),       // Kitty
            (b"\x1bOP", key(KeyCode::F(1), none)),       // SS3
            (b"\x1b[1;5D", key(KeyCode::Left, ctrl)),
            (b"\x1b[Z", key(KeyCode::BackTab, shift)),
            (b"\x1b[9;2u", key(KeyCode::BackTab, shift)),
            (b"\0", key(KeyCode::Char(' '), ctrl)),
            (b"\x1c", key(KeyCode::Char('4'), ctrl)),
            (b"\x1f", key(KeyCode::Char('7'), ctrl)),
            ("Ж".as_bytes(), key(KeyCode::Char('Ж'), shift)),
            (
                "\x1bé".as_bytes(),
                key(KeyCode::Char('é'), KeyModifiers::ALT),
            ),
            (b"\x1b[M *%", click(none)),                  // X10
            (b"\x1b[32;10;5M", click(none)),              // urxvt
            (b"\x1b[<8;10;5M", click(KeyModifiers::ALT)), // SGR
            (b"\x1b[I", Event::FocusGained),
            (b"\x1b[O", Event::FocusLost),
            (
                b"\x1b[1;5:2D",
                Event::Key(KeyEvent::new_with_kind(
                    KeyCode::Left,
                    ctrl,
                    crossterm::event::KeyEventKind::Repeat,
                )),
            ),
            (
                b"\x1b[97;1:3u",
                Event::Key(KeyEvent::new_with_kind(
                    KeyCode::Char('a'),
                    none,
                    crossterm::event::KeyEventKind::Release,
                )),
            ),
        ] {
            let mut keyboard = Keyboard::default();
            for &b in sequence {
                keyboard.feed(&[b]);
            }
            assert_eq!(keyboard.queued.len(), 1, "{sequence:?}");
            assert_eq!(keyboard.queued[0], expected, "{sequence:?}");
            if let (Event::Key(actual), Event::Key(expected)) = (&keyboard.queued[0], &expected) {
                assert_eq!(actual.modifiers, expected.modifiers);
            }
        }
    }

    #[test]
    fn paste_keeps_raw_contents_and_the_next_key() {
        let mut keyboard = Keyboard::default();
        let contents = "Ж\r\n\t\x1b]11;rgb:1/2/3\x07\x1b[200~";
        let sequence = [PASTE_START, contents.as_bytes(), PASTE_END, b"q"].concat();
        for split in 0..=sequence.len() {
            keyboard.feed(&sequence[..split]);
            keyboard.feed(&sequence[split..]);
            assert_eq!(
                keyboard.queued.pop_front(),
                Some(Event::Paste(contents.into()))
            );
            assert_eq!(
                keyboard.queued.pop_front(),
                Some(Event::Key(KeyCode::Char('q').into()))
            );
            assert!(keyboard.queued.is_empty());
        }
    }

    #[test]
    fn escape_can_expire_without_flushing_partial_unicode_or_csi_parameters() {
        let mut keyboard = Keyboard::default();
        keyboard.feed(&[ESC]);
        assert!(keyboard.escape_pending());
        keyboard.flush_escape();
        keyboard.feed(b"\x1b[1;5");
        assert!(!keyboard.escape_pending());
        keyboard.flush_escape();
        keyboard.feed(b"D\xc3");
        keyboard.flush_escape();
        keyboard.feed(b"\xa9");
        assert_eq!(
            keyboard.queued,
            [
                Event::Key(KeyCode::Esc.into()),
                Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL)),
                Event::Key(KeyCode::Char('é').into()),
            ]
        );
    }

    #[test]
    fn malformed_input_does_not_swallow_the_next_key() {
        let mut keyboard = Keyboard::default();
        keyboard.feed(b"\x1b[<0;0;1M\x1b[32;2;00M\x1b[<0;1;1Qx\x1b[");
        keyboard.feed(&[b'1'; MAX_SEQUENCE + 100]);
        keyboard.feed(b"A\xc3q");
        assert_eq!(
            keyboard.queued,
            [
                Event::Key(KeyCode::Char('x').into()),
                Event::Key(KeyCode::Char('q').into()),
            ]
        );
    }
}
