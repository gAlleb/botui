//! Single-line editing shared by modal prompts and dialogs drawn inside a reader.
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    style::Stylize,
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_truncate::UnicodeTruncateStr;

pub(crate) struct LineInput {
    value: String,
    cursor: usize,
}

impl LineInput {
    pub fn split(&mut self) -> Self {
        Self {
            value: self.value.split_off(self.cursor),
            cursor: 0,
        }
    }
    pub fn into_text(self) -> String {
        self.value
    }
    pub fn at_start(&self) -> bool {
        self.cursor == 0
    }
    pub fn append(&mut self, tail: &str) {
        self.cursor = self.value.len();
        self.value.push_str(tail);
    }
    pub fn new(value: &str) -> Self {
        Self {
            value: value.into(),
            cursor: value.len(),
        }
    }

    pub fn display(&self, width: u16, secret: bool) -> Line<'static> {
        if width == 0 {
            return Line::default();
        }
        let (before, cursor, after) = if secret {
            let (before, after) = self.value.split_at(self.cursor);
            (
                "•".repeat(before.chars().count()),
                if after.is_empty() { " " } else { "•" }.to_owned(),
                "•".repeat(after.chars().count().saturating_sub(1)),
            )
        } else if let Some((start, glyph)) = self
            .value
            .grapheme_indices(true)
            .find(|(start, glyph)| start + glyph.len() > self.cursor)
        {
            // Highlight the existing glyph, including a combining mark if the
            // editing position lies inside it; never insert a printable cursor.
            (
                self.value[..start].to_owned(),
                glyph.to_owned(),
                self.value[start + glyph.len()..].to_owned(),
            )
        } else {
            (self.value.clone(), " ".to_owned(), String::new())
        };
        let width = usize::from(width);
        let cursor = if Span::raw(&cursor).width() > width {
            " "
        } else {
            &cursor
        };
        let cursor_width = Span::raw(cursor).width();
        let (before, used) = before.unicode_truncate_start(width.saturating_sub(cursor_width));
        let (after, _) = after.unicode_truncate(width.saturating_sub(used + cursor_width));
        Line::from(vec![
            Span::raw(before.to_owned()),
            Span::raw(cursor.to_owned()).reversed(),
            Span::raw(after.to_owned()),
        ])
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn finish(self, secret: bool) -> String {
        if secret {
            self.value
        } else {
            self.value.trim().to_owned()
        }
    }

    fn insert(&mut self, input: impl Iterator<Item = char>) {
        let text: String = input
            .filter(|c| !c.is_control())
            .take(2048usize.saturating_sub(self.value.chars().count()))
            .collect();
        self.value.insert_str(self.cursor, &text);
        self.cursor += text.len();
    }

    pub fn edit(&mut self, event: &Event) {
        match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = self.value.len(),
                KeyCode::Left => {
                    self.cursor = self.value[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(i, _)| i);
                }
                KeyCode::Right => {
                    self.cursor += self.value[self.cursor..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8)
                }
                KeyCode::Backspace if self.cursor > 0 => {
                    let i = self.value[..self.cursor]
                        .char_indices()
                        .next_back()
                        .unwrap()
                        .0;
                    self.value.drain(i..self.cursor);
                    self.cursor = i;
                }
                KeyCode::Delete if self.cursor < self.value.len() => {
                    self.value.remove(self.cursor);
                }
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.value.clear();
                    self.cursor = 0;
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.insert(std::iter::once(c))
                }
                _ => (),
            },
            Event::Paste(s) => self.insert(s.chars()),
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        style::Modifier,
        widgets::{Paragraph, Widget},
    };

    fn rendered(input: &LineInput, width: u16, secret: bool) -> Buffer {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);
        Paragraph::new(input.display(width, secret)).render(area, &mut buffer);
        buffer
    }

    fn assert_text(buffer: &Buffer, text: &str) {
        let mut expected = Buffer::empty(buffer.area);
        Paragraph::new(text).render(expected.area, &mut expected);
        assert_eq!(
            buffer
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<Vec<_>>(),
            expected
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn cursor_highlights_the_existing_character_without_moving_the_command() {
        let mut line = LineInput::new("nvim");
        line.edit(&Event::Key(KeyCode::Home.into()));
        for x in 0..=4 {
            let buffer = rendered(&line, 8, false);
            assert_text(&buffer, "nvim");
            assert!(buffer[(x, 0)].modifier.contains(Modifier::REVERSED));
            assert_eq!(
                buffer
                    .content
                    .iter()
                    .filter(|cell| cell.modifier.contains(Modifier::REVERSED))
                    .count(),
                1
            );
            line.edit(&Event::Key(KeyCode::Right.into()));
        }
        assert_eq!(line.finish(false), "nvim");
    }

    #[test]
    fn long_command_keeps_cursor_visible_at_start_middle_and_end() {
        let mut line = LineInput::new("0123456789abcdefgh");
        assert_text(&rendered(&line, 8, false), "bcdefgh ");
        assert!(
            rendered(&line, 8, false)[(7, 0)]
                .modifier
                .contains(Modifier::REVERSED)
        );
        line.edit(&Event::Key(KeyCode::Home.into()));
        assert_text(&rendered(&line, 8, false), "01234567");
        for _ in 0..9 {
            line.edit(&Event::Key(KeyCode::Right.into()));
        }
        let buffer = rendered(&line, 8, false);
        assert_text(&buffer, "23456789");
        assert!(buffer[(7, 0)].modifier.contains(Modifier::REVERSED));
        line.edit(&Event::Paste("X".into()));
        assert_eq!(line.finish(false), "012345678X9abcdefgh");
    }

    #[test]
    fn unicode_cursor_preserves_wide_and_combining_characters() {
        let value = "Я界e\u{301}🙂z";
        let mut line = LineInput::new(value);
        line.edit(&Event::Key(KeyCode::Home.into()));
        for x in [0, 1, 3, 3, 4, 6, 7] {
            let buffer = rendered(&line, 10, false);
            assert_text(&buffer, value);
            assert!(buffer[(x, 0)].modifier.contains(Modifier::REVERSED));
            line.edit(&Event::Key(KeyCode::Right.into()));
        }
        assert_eq!(line.finish(false), value);

        let mut line = LineInput::new("界界界界");
        assert_text(&rendered(&line, 5, false), "界界 ");
        line.edit(&Event::Key(KeyCode::Left.into()));
        let buffer = rendered(&line, 5, false);
        assert_text(&buffer, "界界");
        assert!(buffer[(2, 0)].modifier.contains(Modifier::REVERSED));
        assert!(line.display(0, false).spans.is_empty());
        let narrow = rendered(&line, 1, false);
        assert_text(&narrow, " ");
        assert!(narrow[(0, 0)].modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn password_cursor_never_displays_the_secret() {
        let secret = "пароль🙂";
        let mut line = LineInput::new(secret);
        assert_text(&rendered(&line, 12, true), "••••••• ");
        line.edit(&Event::Key(KeyCode::Home.into()));
        line.edit(&Event::Key(KeyCode::Right.into()));
        let buffer = rendered(&line, 12, true);
        assert_text(&buffer, "•••••••");
        assert!(buffer[(1, 0)].modifier.contains(Modifier::REVERSED));
        line.edit(&Event::Key(KeyCode::End.into()));
        assert_text(&rendered(&line, 4, true), "••• ");
        assert_eq!(line.finish(true), secret);
    }

    #[test]
    fn split_keeps_new_line_cursor_before_the_existing_tail() {
        let mut line = LineInput::new("Привет мир");
        for _ in 0..3 {
            line.edit(&Event::Key(KeyCode::Left.into()));
        }
        let mut tail = line.split();
        tail.edit(&Event::Paste("новый ".into()));
        assert_eq!(line.into_text(), "Привет ");
        assert_eq!(tail.into_text(), "новый мир");
    }
}
