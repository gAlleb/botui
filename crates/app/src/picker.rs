mod info;
use botui_formats::FileKind;
pub(super) use info::show_info;

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{AppTerminal, centered};

struct Entry {
    path: PathBuf,
    name: String,
    kind: Option<FileKind>,
}

pub struct Picker {
    dir: PathBuf,
    entries: Vec<Entry>,
    list: ListState,
    rows: Rect,
    exit: Rect,
    pub error: Option<String>,
}

impl Picker {
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        let mut picker = Self {
            dir: dir.clone(),
            entries: Vec::new(),
            list: ListState::default(),
            rows: Rect::default(),
            exit: Rect::default(),
            error: None,
        };
        picker.load(&dir)?;
        Ok(picker)
    }

    fn load(&mut self, dir: &Path) -> io::Result<()> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = path.is_dir();
            let kind = (!is_dir).then(|| FileKind::from_path(&path)).flatten();
            if !name.starts_with('.') && (is_dir || (path.is_file() && kind.is_some())) {
                entries.push(Entry { path, name, kind });
            }
        }
        entries.sort_by_cached_key(|e| (e.kind.is_some(), e.name.to_lowercase()));
        if let Some(parent) = dir.parent() {
            entries.insert(
                0,
                Entry {
                    path: parent.to_owned(),
                    name: "..".into(),
                    kind: None,
                },
            );
        }
        self.list = ListState::default().with_selected((!entries.is_empty()).then_some(0));
        self.entries = entries;
        self.dir = dir.to_owned();
        self.error = None;
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let panel = centered(area, 80, 30);
        frame.render_widget(Clear, area);
        let block = Block::bordered().title(" Открыть книгу ");
        let inner = block.inner(panel);
        frame.render_widget(block, panel);
        self.exit = Rect::default();
        self.rows = Rect::default();
        if inner.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(self.dir.display().to_string()).bold(),
            Rect { height: 1, ..inner },
        );
        let bottom = u16::from(inner.height >= 4);
        self.rows = Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(1 + bottom),
        );
        let items = self.entries.iter().map(|e| {
            ListItem::new(match e.kind {
                Some(kind) => format!(" [{}] {}", kind.label(), e.name),
                None => format!(" {}/", e.name),
            })
        });
        frame.render_stateful_widget(
            List::new(items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
            self.rows,
            &mut self.list,
        );
        if self.entries.is_empty() {
            frame.render_widget(Paragraph::new("Нет книг и вложенных папок"), self.rows);
        }
        if bottom > 0 {
            let footer = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
            self.exit = Rect::new(footer.x, footer.y, footer.width.min(11), 1);
            frame.render_widget(Paragraph::new("[ Выход Q ]"), self.exit);
            let message = Rect::new(
                self.exit.right(),
                footer.y,
                footer.width.saturating_sub(self.exit.width),
                1,
            );
            match self.error.as_deref() {
                Some(error) => frame.render_widget(Paragraph::new(error).fg(Color::Red), message),
                None => frame.render_widget(Paragraph::new("   Enter / щелчок — открыть"), message),
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let Some(i) = self.list.selected() else {
            return;
        };
        self.list.select(Some(
            i.saturating_add_signed(delta).min(self.entries.len() - 1),
        ));
    }

    pub fn choose(&mut self, term: &mut AppTerminal) -> io::Result<Option<PathBuf>> {
        loop {
            term.draw(|frame| self.draw(frame))?;
            let open = match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    KeyCode::Enter | KeyCode::Right => self.list.selected(),
                    KeyCode::Backspace | KeyCode::Left if self.dir.parent().is_some() => Some(0),
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.move_selection(-1);
                        None
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.move_selection(1);
                        None
                    }
                    KeyCode::PageUp => {
                        self.move_selection(-(self.rows.height as isize));
                        None
                    }
                    KeyCode::PageDown => {
                        self.move_selection(self.rows.height as isize);
                        None
                    }
                    _ => None,
                },
                Event::Mouse(mouse)
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                        && self.exit.contains(Position::new(mouse.column, mouse.row)) =>
                {
                    return Ok(None);
                }
                Event::Mouse(mouse)
                    if self.rows.contains(Position::new(mouse.column, mouse.row)) =>
                {
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            let i = self.list.offset() + usize::from(mouse.row - self.rows.y);
                            if i < self.entries.len() {
                                self.list.select(Some(i));
                                Some(i)
                            } else {
                                None
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            self.move_selection(1);
                            None
                        }
                        MouseEventKind::ScrollUp => {
                            self.move_selection(-1);
                            None
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(entry) = open.and_then(|i| self.entries.get(i)) {
                let path = entry.path.clone();
                if let Some(kind) = entry.kind {
                    self.error = None;
                    if !kind.readable()
                        && crate::cover::graphics().protocol_type()
                            == ratatui_image::picker::ProtocolType::Halfblocks
                        && !show_info(term, &path, kind)?
                    {
                        continue;
                    }
                    return Ok(Some(path));
                }
                let previous = self.dir.clone();
                if let Err(e) = self.load(&path) {
                    self.error = Some(format!("{} — {e}", path.display()));
                } else if let Some(i) = self.entries.iter().position(|e| e.path == previous) {
                    self.list.select(Some(i));
                }
            }
        }
    }
}
