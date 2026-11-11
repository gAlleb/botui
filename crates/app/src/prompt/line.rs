//! Single-line editing shared by modal prompts and dialogs drawn inside a reader.
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
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

    pub fn display(&self, width: u16, secret: bool) -> String {
        let (before, after) = self.value.split_at(self.cursor);
        let (before, after) = if secret {
            (
                "•".repeat(before.chars().count()),
                "•".repeat(after.chars().count()),
            )
        } else {
            (before.to_owned(), after.to_owned())
        };
        let (visible, _) = before.unicode_truncate_start(usize::from(width.saturating_sub(1)));
        format!("{visible}▏{after}")
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
