use crate::centered;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    widgets::{Block, Clear, Paragraph},
};

pub(super) enum Command {
    Submit,
    Back,
}

pub(super) struct Login {
    pub opds: bool,
    pub server: String,
    pub username: String,
    pub password: String,
    focus: usize,
    hits: [Rect; 4],
    back: Rect,
}

impl Login {
    pub fn new(server: String, username: String) -> Self {
        Self {
            opds: false,
            server,
            username,
            password: String::new(),
            focus: 0,
            hits: [Rect::default(); 4],
            back: Rect::default(),
        }
    }

    pub fn focus_password(&mut self) {
        self.focus = 2;
    }

    pub fn draw(&mut self, f: &mut Frame, notice: Option<&str>) {
        let box_area = centered(f.area(), 72, 15);
        let panel = Block::bordered().title(if self.opds {
            " Каталог OPDS · логин и пароль необязательны "
        } else {
            " Вход в BookOrbit "
        });
        let area = panel.inner(box_area);
        f.render_widget(Clear, box_area);
        f.render_widget(panel, box_area);
        self.hits = [Rect::default(); 4];
        self.back = Rect::default();
        let mask = if self.password.is_empty() {
            ""
        } else {
            "••••••••"
        };
        let values = [self.server.as_str(), self.username.as_str(), mask];
        // In compact terminals keep the focused field and submit action visible.
        let compact = area.height < 10;
        let (step, pad) = if compact { (1, 0) } else { (2, 1) };
        let available = area.height.saturating_sub(pad + 1) as usize / step as usize;
        let start = self
            .focus
            .saturating_add(1)
            .saturating_sub(available.max(1));
        for (row, index) in (start..4).enumerate() {
            let y = area.y + pad + row as u16 * step;
            if y >= area.bottom() {
                break;
            }
            let rect = Rect::new(area.x.saturating_add(1), y, area.width.saturating_sub(2), 1);
            self.hits[index] = rect;
            let line = if index == 3 {
                "[ Войти ]".to_owned()
            } else {
                let label = ["Адрес", "Логин", "Пароль"][index];
                format!("{label}   {}", values[index])
            };
            let style = if self.focus == index {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            };
            f.render_widget(Paragraph::new(line).style(style), rect);
        }
        if area.height >= 6 {
            let bottom = area.bottom() - 1;
            self.back = Rect::new(area.x, bottom, area.width.min(10), 1);
            f.render_widget(Paragraph::new("[ Назад ]"), self.back);
            let tail = Rect::new(
                self.back.right(),
                bottom,
                area.width.saturating_sub(self.back.width),
                1,
            );
            f.render_widget(Paragraph::new("  Tab — поле   ·   Enter — далее"), tail);
            if let Some(message) = notice {
                let rect = Rect::new(area.x, bottom.saturating_sub(2), area.width, 2);
                f.render_widget(
                    Paragraph::new(message).wrap(ratatui::widgets::Wrap { trim: true }),
                    rect,
                );
            }
        }
    }

    fn field(&mut self) -> Option<&mut String> {
        match self.focus {
            0 => Some(&mut self.server),
            1 => Some(&mut self.username),
            2 => Some(&mut self.password),
            _ => None,
        }
    }

    pub fn input(&mut self, e: Event) -> Option<Command> {
        match e {
            Event::Paste(value) => {
                if let Some(field) = self.field() {
                    field.extend(
                        value
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(4096usize.saturating_sub(field.chars().count())),
                    );
                }
            }
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => return Some(Command::Back),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Some(Command::Back);
                }
                KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % 4,
                KeyCode::BackTab | KeyCode::Up => self.focus = (self.focus + 3) % 4,
                KeyCode::Enter if self.focus >= 2 => return Some(Command::Submit),
                KeyCode::Enter => self.focus += 1,
                KeyCode::Backspace => {
                    if let Some(v) = self.field() {
                        v.pop();
                    }
                }
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(v) = self.field() {
                        v.clear();
                    }
                }
                KeyCode::Char(c)
                    if !c.is_control()
                        && !k
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    if let Some(v) = self.field()
                        && v.len() < 16384
                    {
                        v.push(c);
                    }
                }
                _ => {}
            },
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                let point = Position::new(m.column, m.row);
                if self.back.contains(point) {
                    return Some(Command::Back);
                }
                if let Some(index) = self.hits.iter().position(|r| r.contains(point)) {
                    self.focus = index;
                    if index == 3 {
                        return Some(Command::Submit);
                    }
                }
            }
            _ => {}
        }
        None
    }
}
