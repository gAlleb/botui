use ratatui::style::Color;

const ESC: u8 = 0x1b;
const MAX_REPLY: usize = 1024;

/// Track paste delimiters in the bytes delivered to the input decoder. A pasted
/// escape sequence is text even when it looks exactly like the awaited reply.
#[derive(Default)]
struct Paste {
    active: bool,
    matched: usize,
}

impl Paste {
    fn forward(&mut self, bytes: &[u8], input: &mut Vec<u8>) {
        input.extend_from_slice(bytes);
        for &byte in bytes {
            let marker = if self.active {
                b"\x1b[201~"
            } else {
                b"\x1b[200~"
            };
            if byte == marker[self.matched] {
                self.matched += 1;
                if self.matched == marker.len() {
                    self.active = !self.active;
                    self.matched = 0;
                }
            } else {
                self.matched = usize::from(byte == ESC);
            }
        }
    }
}

#[derive(Default)]
pub(super) struct GraphicsReplies {
    pending: Vec<u8>,
    paste: Paste,
    background: Option<Color>,
}

impl GraphicsReplies {
    /// Only the terminating graphics status report returns true. Callers still
    /// pass the rest of this read through the filter, in order.
    pub(super) fn push(
        &mut self,
        byte: u8,
        graphics: bool,
        input: &mut Vec<u8>,
        reply: &mut Vec<u8>,
    ) -> bool {
        if self.paste.active || (self.pending.is_empty() && byte != ESC) {
            self.paste.forward(std::slice::from_ref(&byte), input);
            return false;
        }
        if byte == ESC {
            // Inside a string this can be the start of its ST terminator.
            if !self.pending.starts_with(b"\x1b_") && !self.pending.starts_with(b"\x1b]")
                || self.pending.ends_with(&[ESC])
            {
                self.finish(input);
            }
            self.pending.push(ESC);
            return false;
        }
        if self.pending.len() > 1 && self.pending.ends_with(&[ESC]) && byte != b'\\' {
            // The string was interrupted. Give its bytes back and let the
            // newest Escape introduce the next sequence.
            self.pending.pop();
            self.finish(input);
            self.pending.push(ESC);
        }
        self.pending.push(byte);
        match classify(&self.pending, graphics) {
            Part::More if self.pending.len() < MAX_REPLY => (),
            Part::Input | Part::More => self.finish(input),
            Part::Graphics { complete } => {
                reply.append(&mut self.pending);
                return complete;
            }
            Part::Paper(color) => {
                self.background = Some(color);
                self.pending.clear();
            }
        }
        false
    }

    pub(super) fn take_background(&mut self) -> Option<Color> {
        self.background.take()
    }

    pub(super) fn escape_pending(&self) -> bool {
        matches!(self.pending.as_slice(), [ESC] | [ESC, b'[' | b']' | b'_'])
    }

    pub(super) fn lone_escape(&self) -> bool {
        self.pending == [ESC]
    }

    pub(super) fn finish(&mut self, input: &mut Vec<u8>) {
        self.paste.forward(&self.pending, input);
        self.pending.clear();
    }
}

enum Part {
    More,
    Input,
    Graphics { complete: bool },
    Paper(Color),
}

fn number(field: &[u8]) -> bool {
    !field.is_empty() && field.iter().all(u8::is_ascii_digit)
}

fn classify(packet: &[u8], graphics: bool) -> Part {
    if let Some(body) = packet.strip_prefix(b"\x1b]") {
        if let Some(text) = body
            .strip_suffix(b"\x07")
            .or_else(|| body.strip_suffix(b"\x1b\\"))
        {
            return std::str::from_utf8(text)
                .ok()
                .and_then(super::parse)
                .map_or(Part::Input, Part::Paper);
        }
        if b"11;rgb:".starts_with(body)
            || body.strip_prefix(b"11;rgb:").is_some_and(|v| {
                v.len() <= 14 && v.iter().all(|b| b.is_ascii_hexdigit() || *b == b'/')
            })
        {
            return Part::More;
        }
    } else if graphics {
        if let Some(body) = packet.strip_prefix(b"\x1b[") {
            if body == b"0n" {
                return Part::Graphics { complete: true };
            }
            if let Some(attrs) = body.strip_prefix(b"?").and_then(|b| b.strip_suffix(b"c"))
                && attrs.split(|&b| b == b';').all(number)
            {
                return Part::Graphics { complete: false };
            }
            if let Some(size) = body.strip_prefix(b"6;").and_then(|b| b.strip_suffix(b"t")) {
                let mut fields = size.split(|&b| b == b';');
                if fields.next().is_some_and(number)
                    && fields.next().is_some_and(number)
                    && fields.next().is_none()
                {
                    return Part::Graphics { complete: false };
                }
            }
            if body.is_empty()
                || body == b"0"
                || body == b"6"
                || body
                    .strip_prefix(b"?")
                    .or_else(|| body.strip_prefix(b"6;"))
                    .is_some_and(|p| p.iter().all(|b| b.is_ascii_digit() || *b == b';'))
            {
                return Part::More;
            }
        } else if let Some(body) = packet.strip_prefix(b"\x1b_") {
            for id in [b"Gi=31;", b"Gi=33;"] {
                if id.starts_with(body) {
                    return Part::More;
                }
                if let Some(payload) = body.strip_prefix(id) {
                    if let Some(status) = payload.strip_suffix(b"\x1b\\") {
                        let code = status.split(|&b| b == b':').next().unwrap_or_default();
                        if status == b"OK"
                            || (code.starts_with(b"E")
                                && code.len() > 1
                                && code
                                    .iter()
                                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
                        {
                            return Part::Graphics { complete: false };
                        }
                        return Part::Input;
                    }
                    if payload.last().is_some_and(|b| !b.is_ascii_control()) {
                        return Part::More;
                    }
                }
            }
        }
    }
    Part::Input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Stream {
        filter: GraphicsReplies,
        input: Vec<u8>,
        reply: Vec<u8>,
        completed: usize,
    }
    impl Stream {
        fn write(&mut self, bytes: &[u8], graphics: bool) {
            for &b in bytes {
                if self
                    .filter
                    .push(b, graphics, &mut self.input, &mut self.reply)
                {
                    self.completed += 1;
                }
            }
        }
    }

    #[test]
    fn complete_replies_are_extracted_between_unmodified_input_bytes() {
        let mut stream = Stream::default();
        stream.write(b"\r\xc3\x1b_Gi=31;OK\x1b\\\xa9\x1b[1;\x1b[6;18;9t5D", true);
        stream.write(b"\x1b[?62;4c\x1b_Gi=33;E2BIG:too big\x1b\\\x1b[0n\r", true);
        assert_eq!(stream.input, "\ré\x1b[1;5D\r".as_bytes());
        assert_eq!(
            stream.reply,
            b"\x1b_Gi=31;OK\x1b\\\x1b[6;18;9t\x1b[?62;4c\x1b_Gi=33;E2BIG:too big\x1b\\\x1b[0n"
        );
        assert_eq!(stream.completed, 1);
    }

    #[test]
    fn paste_delimiters_and_pasted_replies_remain_input() {
        let mut stream = Stream::default();
        stream.write(b"\x1b[20\x1b[?1;2c0~", true);
        let body = b"\r\xc3\xa9\x1b[6;22;11t\x1b]11;rgb:f/0/f\x07\x1b_Gi=31;OK\x1b\\\x1b[0n";
        stream.write(body, true);
        stream.write(b"\x1b[20", false);
        stream.write(b"1~\x1b[0n", true);
        assert_eq!(
            stream.input,
            [b"\x1b[200~".as_slice(), body, b"\x1b[201~"].concat()
        );
        assert_eq!(stream.reply, b"\x1b[?1;2c\x1b[0n");
        assert_eq!(stream.completed, 1);
        assert_eq!(stream.filter.take_background(), None);
    }

    #[test]
    fn recognizes_background_reports_also_outside_graphics_queries() {
        let mut stream = Stream::default();
        stream.write(b"\x1b]11;rgb:f/80/0000\x07", true);
        assert_eq!(
            stream.filter.take_background(),
            Some(Color::Rgb(255, 128, 0))
        );
        stream.write(b"\x1b]11;rgb:aaaa/0011/2222\x1b\\", false);
        assert_eq!(
            stream.filter.take_background(),
            Some(Color::Rgb(170, 0, 34))
        );
        let other = b"\x1b]10;rgb:0/0/0\x07\x1b]11;rgb:ff/0\x07\x1b[0n\x1b_Gi=33;OK\x1b\\";
        stream.write(other, false);
        assert_eq!(stream.input, other);
        assert!(stream.reply.is_empty());
        assert_eq!(stream.filter.take_background(), None);
    }

    #[test]
    fn other_sequences_are_restored_and_lone_escape_can_expire() {
        let mut stream = Stream::default();
        let other =
            b"\x1b_Gi=34;OK\x1b\\\x1b_Gi=31;other\x1b\\\x1b[6;18;9;8t\x1b[?2;3h\x1b[A\x1b[<0;2;3M";
        stream.write(other, true);
        stream.write(b"\x1b\x1b[0n\x1b", true);
        assert!(stream.filter.escape_pending());
        stream.filter.finish(&mut stream.input);
        assert_eq!(stream.input, [other.as_slice(), b"\x1b\x1b"].concat());
        assert_eq!(stream.reply, b"\x1b[0n");
        assert!(!stream.filter.escape_pending());
        stream.write(b"\x1b]", false);
        assert!(stream.filter.escape_pending());
        stream.write(b"11;rgb:1", false);
        assert!(!stream.filter.escape_pending());
        stream.filter.finish(&mut stream.input);
        assert!(stream.input.ends_with(b"\x1b]11;rgb:1"));
    }

    #[test]
    fn oversized_strings_and_pastes_do_not_grow_the_reply_buffer() {
        let mut stream = Stream::default();
        let data = vec![b'X'; MAX_REPLY * 8];
        let packet = [b"\x1b_Gi=31;".as_slice(), &data, b"\x1b\\"].concat();
        stream.write(&packet, true);
        stream.write(b"\x1b[200~", true);
        stream.write(&data, true);
        stream.write(b"\x1b[201~\x1b[0n", true);
        assert_eq!(
            stream.input,
            [packet.as_slice(), b"\x1b[200~", &data, b"\x1b[201~"].concat()
        );
        assert_eq!(stream.reply, b"\x1b[0n");
        assert!(stream.filter.pending.capacity() <= MAX_REPLY);
    }
}
