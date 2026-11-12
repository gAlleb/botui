//! Small reader dialogs are drawn after the page, in the page pane.
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
    widgets::{Clear, Paragraph},
};

pub(super) enum Dialog {
    Page { value: String, invalid: bool },
    Width,
    Search(crate::prompt::LineInput),
}
#[derive(Clone, Copy)]
pub(super) enum Command {
    None,
    Close,
    Find,
    Go(u32),
    Zoom(bool),
    Fit,
    Gap(bool),
}
pub(super) type Hits = Vec<(Rect, Command)>;
impl Dialog {
    pub fn page() -> Self {
        Self::Page {
            value: String::new(),
            invalid: false,
        }
    }
    pub fn draw(
        &self,
        f: &mut Frame,
        pane: Rect,
        total: u32,
        percent: u16,
        layout: (bool, u16),
        hits: &mut Hits,
    ) {
        let (scrolling, gap) = layout;
        let fit = if scrolling { "Fit width" } else { "Fit page" };
        let width = match self {
            Self::Page { .. } => 34,
            Self::Width => 44,
            Self::Search(_) => 54,
        };
        let outer = crate::centered(pane, width, if matches!(self, Self::Width) { 7 } else { 5 });
        let (title, line, buttons) = match self {
            Self::Search(input) => (
                " Find in text layer ".into(),
                input.display(outer.width.saturating_sub(4), false),
                vec![
                    ("[ Search ↵ ]", Command::Find),
                    ("[ Cancel ]", Command::Close),
                ],
            ),
            Self::Page { value, invalid } => (
                format!(" Go to page · 1–{total} "),
                if *invalid {
                    format!("Enter 1–{total}: {value}▏")
                } else {
                    format!("{value}▏")
                },
                vec![("[ Go ↵ ]", Command::Go(0)), ("[ Cancel ]", Command::Close)],
            ),
            Self::Width => (
                " Page width / zoom ".into(),
                format!("{percent}%  ·  100% = {fit}"),
                vec![
                    ("[ − ]", Command::Zoom(false)),
                    (
                        if scrolling {
                            "[ Fit width ]"
                        } else {
                            "[ Fit page ]"
                        },
                        Command::Fit,
                    ),
                    ("[ + ]", Command::Zoom(true)),
                    ("[ Done ]", Command::Close),
                ],
            ),
        };
        let block = crate::appearance::block().title(title);
        let inner = block.inner(outer);
        crate::overlay::paint(f, outer, |f| {
            f.render_widget(Clear, outer);
            f.render_widget(block, outer);
            if inner.is_empty() {
                return;
            }
            let left = inner.x + u16::from(inner.width > 4);
            f.render_widget(
                Paragraph::new(line),
                Rect::new(left, inner.y, inner.right() - left, 1),
            );
            if matches!(self, Self::Width) {
                let label = format!("Spread gap: {gap} cells");
                f.render_widget(
                    Paragraph::new(label),
                    Rect::new(left, inner.y + 2, inner.width.saturating_sub(2), 1),
                );
                let mut x = left + 22;
                for (label, action) in [
                    ("[ < ]", Command::Gap(false)),
                    ("[ > ]", Command::Gap(true)),
                ] {
                    let width = 7.min(inner.right().saturating_sub(x));
                    let rect = Rect::new(x, inner.y + 2, width, 1);
                    f.render_widget(Paragraph::new(label).cyan(), rect);
                    hits.push((rect, action));
                    x += 8;
                }
            }
            let mut x = left;
            for (label, command) in buttons {
                let w = (label.chars().count() as u16).min(inner.right().saturating_sub(x));
                if w == 0 {
                    break;
                }
                let rect = Rect::new(x, inner.bottom() - 1, w, 1);
                f.render_widget(Paragraph::new(label).cyan(), rect);
                hits.push((rect, command));
                x += w + 1;
            }
        });
    }
    pub fn input(&mut self, e: &Event, hits: &Hits, total: u32) -> Command {
        if let Self::Search(input) = self {
            input.edit(e);
        }
        let action = match e {
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => hits
                .iter()
                .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
                .map_or(Command::None, |h| h.1),
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => Command::Close,
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => Command::Close,
                _ => match self {
                    Self::Search(_) if k.code == KeyCode::Enter => Command::Find,
                    Self::Search(_) => Command::None,
                    Self::Width => match k.code {
                        KeyCode::Char('+' | '=') | KeyCode::Right => Command::Zoom(true),
                        KeyCode::Char('-' | '−') | KeyCode::Left => Command::Zoom(false),
                        KeyCode::Char('0') => Command::Fit,
                        KeyCode::Char('[') => Command::Gap(false),
                        KeyCode::Char(']') => Command::Gap(true),
                        KeyCode::Enter | KeyCode::Char('q' | 'Q' | 'w' | 'W') => Command::Close,
                        _ => Command::None,
                    },
                    Self::Page { value, invalid } => {
                        *invalid = false;
                        match k.code {
                            KeyCode::Enter => Command::Go(0),
                            KeyCode::Backspace => {
                                value.pop();
                                Command::None
                            }
                            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                                value.clear();
                                Command::None
                            }
                            KeyCode::Char(c) if c.is_ascii_digit() && value.len() < 10 => {
                                value.push(c);
                                Command::None
                            }
                            _ => Command::None,
                        }
                    }
                },
            },
            Event::Paste(s) => {
                if let Self::Page { value, invalid } = self
                    && s.trim().bytes().all(|c| c.is_ascii_digit())
                {
                    value.extend(s.trim().chars().take(10 - value.len()));
                    *invalid = false;
                }
                Command::None
            }
            _ => Command::None,
        };
        if let Command::Go(_) = action
            && let Self::Page { value, invalid } = self
        {
            return match value.parse::<u32>() {
                Ok(p) if (1..=total).contains(&p) => Command::Go(p),
                _ => {
                    *invalid = true;
                    Command::None
                }
            };
        }
        action
    }
}
