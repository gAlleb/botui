//! Input grammar shared by text and fixed-page readers. No layout or I/O here.
use crossterm::event::KeyCode;

pub mod document;
pub mod text;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Down,
    Up,
    Word,
    BackWord,
    EndWord,
    BigWord,
    BackBigWord,
    EndBigWord,
    Find {
        ch: char,
        forward: bool,
        till: bool,
        repeat: bool,
    },
    Start,
    First,
    End,
    NextParagraph,
    PreviousParagraph,
    ScreenDown,
    ScreenUp,
    ScreenStart,
    ScreenEnd,
    Top,
    Bottom,
    InnerWord,
    AroundWord,
    InnerBigWord,
    AroundBigWord,
    Delimited {
        open: char,
        close: char,
        around: bool,
    },
    Paragraph,
    Line,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Wait,
    Cancel,
    Move(Motion, usize),
    Yank(Motion, usize),
    Mark(char),
    Jump(char),
    Visual,
    VisualLine,
    Reselect,
    Search(bool),
    Next(bool),
}
#[derive(Default)]
pub struct Keys {
    count: usize,
    operator: bool,
    operator_count: usize,
    prefix: Option<char>,
    last_find: Option<(char, bool, bool)>,
}
impl Keys {
    pub fn reset(&mut self) {
        *self = Self {
            last_find: self.last_find,
            ..Self::default()
        };
    }
    pub fn expects_literal(&self) -> bool {
        matches!(self.prefix, Some('m' | '\'' | '`' | 'f' | 'F' | 't' | 'T'))
    }
    pub fn pending(&self) -> bool {
        self.operator || self.prefix.is_some() || self.count > 0
    }
    pub fn input(&mut self, code: KeyCode, visual: bool) -> Option<Edit> {
        if code == KeyCode::Esc {
            self.reset();
            return Some(Edit::Cancel);
        }
        let c = match code {
            KeyCode::Char(c) => c,
            KeyCode::Left => 'h',
            KeyCode::Right => 'l',
            KeyCode::Down => 'j',
            KeyCode::Up => 'k',
            KeyCode::Home => '0',
            KeyCode::End => '$',
            _ => return None,
        };
        if matches!(self.prefix, Some('m' | '\'' | '`')) {
            let prefix = self.prefix.take().unwrap();
            self.reset();
            return Some(if c.is_ascii_alphabetic() {
                if prefix == 'm' {
                    Edit::Mark(c)
                } else {
                    Edit::Jump(c)
                }
            } else {
                Edit::Cancel
            });
        }
        if c.is_ascii_digit() && (c != '0' || self.count > 0) && self.prefix.is_none() {
            self.count = (self.count * 10 + c.to_digit(10).unwrap() as usize).min(9999);
            return Some(Edit::Wait);
        }
        let motion = if let Some(prefix) = self.prefix.take() {
            match (prefix, c) {
                ('g', 'g') => Some(Motion::Top),
                ('g', 'j') => Some(Motion::ScreenDown),
                ('g', 'k') => Some(Motion::ScreenUp),
                ('g', '0') => Some(Motion::ScreenStart),
                ('g', '$') => Some(Motion::ScreenEnd),
                ('g', 'v') if !self.operator => {
                    self.reset();
                    return Some(Edit::Reselect);
                }
                ('i', 'w') => Some(Motion::InnerWord),
                ('a', 'w') => Some(Motion::AroundWord),
                ('i', 'W') => Some(Motion::InnerBigWord),
                ('a', 'W') => Some(Motion::AroundBigWord),
                ('i' | 'a', 'p') => Some(Motion::Paragraph),
                (
                    'i' | 'a',
                    '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | 'b' | 'B',
                ) => {
                    let (open, close) = match c {
                        '(' | ')' | 'b' => ('(', ')'),
                        '[' | ']' => ('[', ']'),
                        '{' | '}' | 'B' => ('{', '}'),
                        '<' | '>' => ('<', '>'),
                        c => (c, c),
                    };
                    Some(Motion::Delimited {
                        open,
                        close,
                        around: prefix == 'a',
                    })
                }
                ('f' | 'F' | 't' | 'T', _) if matches!(code, KeyCode::Char(_)) => {
                    let forward = matches!(prefix, 'f' | 't');
                    let till = matches!(prefix, 't' | 'T');
                    self.last_find = Some((c, forward, till));
                    Some(Motion::Find {
                        ch: c,
                        forward,
                        till,
                        repeat: false,
                    })
                }
                _ => {
                    self.reset();
                    return Some(Edit::Cancel);
                }
            }
        } else {
            match c {
                'y' if visual => Some(Motion::Line),
                'y' if self.operator => Some(Motion::Line),
                'y' => {
                    self.operator = true;
                    self.operator_count = self.count.max(1);
                    self.count = 0;
                    return Some(Edit::Wait);
                }
                'g' => {
                    self.prefix = Some(c);
                    return Some(Edit::Wait);
                }
                'f' | 'F' | 't' | 'T' => {
                    self.prefix = Some(c);
                    return Some(Edit::Wait);
                }
                ';' | ',' => self
                    .last_find
                    .map(|(ch, forward, till)| Motion::Find {
                        ch,
                        forward: if c == ';' { forward } else { !forward },
                        till,
                        repeat: true,
                    })
                    .or_else(|| {
                        self.reset();
                        None
                    }),
                'i' | 'a' if self.operator || visual => {
                    self.prefix = Some(c);
                    return Some(Edit::Wait);
                }
                'm' | '\'' | '`' if !self.operator => {
                    self.prefix = Some(c);
                    return Some(Edit::Wait);
                }
                'h' => Some(Motion::Left),
                'l' => Some(Motion::Right),
                'j' => Some(Motion::Down),
                'k' => Some(Motion::Up),
                '}' => Some(Motion::NextParagraph),
                '{' => Some(Motion::PreviousParagraph),
                'w' => Some(Motion::Word),
                'b' => Some(Motion::BackWord),
                'e' => Some(Motion::EndWord),
                'W' => Some(Motion::BigWord),
                'B' => Some(Motion::BackBigWord),
                'E' => Some(Motion::EndBigWord),
                '0' => Some(Motion::Start),
                '^' => Some(Motion::First),
                '$' => Some(Motion::End),
                'G' => Some(Motion::Bottom),
                'v' if !self.operator => {
                    self.reset();
                    return Some(Edit::Visual);
                }
                'V' if !self.operator => {
                    self.reset();
                    return Some(Edit::VisualLine);
                }
                '/' | '?' if !self.operator => {
                    self.reset();
                    return Some(Edit::Search(c == '/'));
                }
                'n' | 'N' if !self.operator => {
                    self.reset();
                    return Some(Edit::Next(c == 'n'));
                }
                _ => {
                    if self.pending() {
                        self.reset();
                        return Some(Edit::Cancel);
                    }
                    return None;
                }
            }
        }?;
        let count = self.count.max(1) * self.operator_count.max(1);
        let yank = self.operator || (visual && c == 'y' && motion == Motion::Line);
        self.reset();
        Some(if yank {
            Edit::Yank(motion, count.min(9999))
        } else {
            Edit::Move(motion, count.min(9999))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(s: &str) -> Edit {
        let mut k = Keys::default();
        s.chars()
            .filter_map(|c| k.input(KeyCode::Char(c), false))
            .last()
            .unwrap()
    }
    #[test]
    fn normal_operators() {
        assert_eq!(parse("yiw"), Edit::Yank(Motion::InnerWord, 1));
        assert_eq!(parse("yip"), Edit::Yank(Motion::Paragraph, 1));
        assert_eq!(parse("3y2w"), Edit::Yank(Motion::Word, 6));
        assert_eq!(parse("yy"), Edit::Yank(Motion::Line, 1));
        assert_eq!(parse("y$"), Edit::Yank(Motion::End, 1));
        assert_eq!(parse("2j"), Edit::Move(Motion::Down, 2));
    }
    #[test]
    fn cancel_never_replays_suffix() {
        assert_eq!(parse("yq"), Edit::Cancel);
        assert_eq!(parse("gq"), Edit::Cancel);
    }
    #[test]
    fn find_targets_are_literal_and_counts_compose_with_yank() {
        for ch in [' ', '0', 'y', '?', 'Я'] {
            let mut keys = Keys::default();
            assert_eq!(keys.input(KeyCode::Char('f'), false), Some(Edit::Wait));
            assert!(keys.expects_literal());
            assert_eq!(
                keys.input(KeyCode::Char(ch), false),
                Some(Edit::Move(
                    Motion::Find {
                        ch,
                        forward: true,
                        till: false,
                        repeat: false,
                    },
                    1
                ))
            );
            assert!(!keys.expects_literal());
        }
        assert_eq!(
            parse("2y3F,"),
            Edit::Yank(
                Motion::Find {
                    ch: ',',
                    forward: false,
                    till: false,
                    repeat: false,
                },
                6
            )
        );
        assert_eq!(
            parse("t,;"),
            Edit::Move(
                Motion::Find {
                    ch: ',',
                    forward: true,
                    till: true,
                    repeat: true,
                },
                1
            )
        );
        assert_eq!(
            parse("t,,;"),
            Edit::Move(
                Motion::Find {
                    ch: ',',
                    forward: true,
                    till: true,
                    repeat: true,
                },
                1
            ),
            "reverse repeat must not replace the remembered direction"
        );
        assert_eq!(
            parse("t,,"),
            Edit::Move(
                Motion::Find {
                    ch: ',',
                    forward: false,
                    till: true,
                    repeat: true,
                },
                1
            )
        );
    }
    #[test]
    fn big_words_and_delimiters_share_operator_and_visual_grammar() {
        assert_eq!(parse("yE"), Edit::Yank(Motion::EndBigWord, 1));
        assert_eq!(parse("2B"), Edit::Move(Motion::BackBigWord, 2));
        assert_eq!(parse("yiW"), Edit::Yank(Motion::InnerBigWord, 1));
        for command in ["yi(", "yi)", "yib"] {
            assert_eq!(
                parse(command),
                Edit::Yank(
                    Motion::Delimited {
                        open: '(',
                        close: ')',
                        around: false
                    },
                    1
                )
            );
        }
        assert_eq!(
            parse("2ya\""),
            Edit::Yank(
                Motion::Delimited {
                    open: '"',
                    close: '"',
                    around: true
                },
                2
            )
        );
        let mut keys = Keys::default();
        keys.input(KeyCode::Char('i'), true);
        assert_eq!(
            keys.input(KeyCode::Char(']'), true),
            Some(Edit::Move(
                Motion::Delimited {
                    open: '[',
                    close: ']',
                    around: false,
                },
                1
            ))
        );
    }
}
