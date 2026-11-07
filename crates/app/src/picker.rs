mod gallery;
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
    widgets::{Clear, List, ListItem, ListState, Paragraph},
};

use crate::AppTerminal;

#[derive(Clone, PartialEq, Eq)]
enum LocalView {
    Directory,
    Search(String),
    Reading,
    Recent,
}
struct Entry {
    percent: Option<u8>,
    path: PathBuf,
    name: String,
    kind: Option<FileKind>,
}

pub struct Picker {
    dir: PathBuf,
    root: PathBuf,
    grid_view: bool,
    grid: Option<crate::shelf::gallery::Grid>,
    covers: crate::shelf::covers::Covers,
    graphics: ratatui_image::picker::Picker,
    view_button: Rect,
    view: LocalView,
    shelves: [Rect; 3],
    search_button: Rect,
    root_button: Rect,
    path_button: Rect,
    bg_button: Rect,
    borders_button: Rect,
    up_button: Rect,
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
            grid_view: crate::settings::Settings::load()?.local_grid,
            grid: None,
            covers: crate::shelf::covers::Covers::new(),
            graphics: crate::cover::graphics(),
            view_button: Rect::default(),
            root: dir.clone(),
            view: LocalView::Directory,
            shelves: [Rect::default(); 3],
            search_button: Rect::default(),
            root_button: Rect::default(),
            path_button: Rect::default(),
            bg_button: Rect::default(),
            borders_button: Rect::default(),
            up_button: Rect::default(),
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
                entries.push(Entry {
                    path,
                    name,
                    kind,
                    percent: None,
                });
            }
        }
        entries.sort_by_cached_key(|e| (e.kind.is_some(), e.name.to_lowercase()));
        self.list = ListState::default().with_selected((!entries.is_empty()).then_some(0));
        self.view = LocalView::Directory;
        self.entries = entries;
        self.dir = dir.to_owned();
        self.error = None;
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let panel = area;
        frame.render_widget(Clear, area);
        let block = crate::appearance::block().title(" Local library ");
        let inner = block.inner(panel);
        frame.render_widget(block, panel);
        self.exit = Rect::default();
        self.rows = Rect::default();
        if inner.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(match &self.view {
                LocalView::Reading => "Currently reading".into(),
                LocalView::Recent => "Recently added".into(),
                LocalView::Search(q) => format!("Search · {} · {} results", q, self.entries.len()),
                LocalView::Directory => self.dir.display().to_string(),
            })
            .bold(),
            Rect { height: 1, ..inner },
        );
        let label = if self.grid_view {
            "[V List]"
        } else {
            "[V Grid]"
        };
        self.view_button = Rect::new(
            inner.right().saturating_sub(8).max(inner.x),
            inner.y,
            inner.width.min(8),
            1,
        );
        frame.render_widget(Paragraph::new(label).cyan(), self.view_button);
        let mut x = inner.x;
        for (i, label) in ["1 Folders", "2 Currently reading", "3 Recently added"]
            .iter()
            .enumerate()
        {
            let rect = Rect::new(
                x,
                inner.y + 1,
                (label.len() as u16 + 2).min(inner.right().saturating_sub(x)),
                inner.height.saturating_sub(1).min(1),
            );
            self.shelves[i] = rect;
            x = rect.right();
            let selected = matches!(
                (&self.view, i),
                (LocalView::Directory, 0) | (LocalView::Reading, 1) | (LocalView::Recent, 2)
            );
            frame.render_widget(
                Paragraph::new(format!(" {label} ")).style(if selected {
                    Style::new().reversed()
                } else {
                    Style::new().fg(Color::Cyan)
                }),
                rect,
            );
        }
        let bottom = u16::from(inner.height >= 4);
        self.rows = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2 + bottom),
        );
        self.grid = None;
        if self.grid_view
            && self.rows.height >= 5
            && self.rows.width >= 18
            && !self.entries.is_empty()
        {
            self.gallery(frame);
        } else {
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
        }
        if self.entries.is_empty() {
            frame.render_widget(Paragraph::new("No books or folders here."), self.rows);
        }
        if bottom > 0 {
            let footer = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
            self.exit = Rect::new(footer.x, footer.y, footer.width.min(11), 1);
            frame.render_widget(Paragraph::new("[ Q Back ]"), self.exit);
            let message = Rect::new(
                self.exit.right(),
                footer.y,
                footer.width.saturating_sub(self.exit.width),
                1,
            );
            let mut x = message.x;
            let mut button = |label: &str| {
                let r = Rect::new(
                    x,
                    message.y,
                    (label.chars().count() as u16 + 1).min(message.right().saturating_sub(x)),
                    1,
                );
                frame.render_widget(Paragraph::new(label).cyan(), r);
                x = r.right();
                r
            };
            self.up_button = if self.view == LocalView::Directory && self.dir.parent().is_some() {
                button("[↑ Up]")
            } else {
                Rect::default()
            };
            self.path_button = button("[P Folder…]");
            self.search_button = button("[/ Search]");
            self.root_button = button("[R Set library here]");
            self.bg_button = button("[F Background]");
            self.borders_button = button("[U Borders]");
            if let Some(error) = &self.error {
                frame.render_widget(
                    Paragraph::new(error.as_str()).fg(Color::Yellow),
                    Rect::new(inner.x, inner.y, inner.width, 1),
                );
            }
        }
    }

    fn browse(&mut self, view: LocalView) -> io::Result<()> {
        if matches!(&view, LocalView::Directory)
            || matches!(&view,LocalView::Search(q) if q.is_empty())
        {
            return self.load(&self.dir.clone());
        }
        if view == LocalView::Reading {
            self.entries = botui_store::HistoryEntry::for_root(&self.root)?
                .into_iter()
                .filter_map(|entry| {
                    let kind = FileKind::from_path(&entry.path)?;
                    let name = format!(
                        "{:>3}%   {}",
                        entry.percent,
                        entry.path.file_name()?.to_string_lossy()
                    );
                    Some(Entry {
                        percent: Some(entry.percent),
                        path: entry.path,
                        name,
                        kind: Some(kind),
                    })
                })
                .collect();
            self.list = ListState::default().with_selected((!self.entries.is_empty()).then_some(0));
            self.view = view;
            self.error = None;
            return Ok(());
        }
        let needle = match &view {
            LocalView::Search(q) => q.to_lowercase(),
            _ => String::new(),
        };
        let mut dirs = vec![self.root.clone()];
        let mut entries = Vec::new();
        while let Some(dir) = dirs.pop() {
            let items = match fs::read_dir(&dir) {
                Ok(i) => i,
                Err(_) if dir != self.root => continue,
                Err(e) => return Err(e),
            };
            for item in items.flatten() {
                if item.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let path = item.path();
                let Ok(t) = item.file_type() else { continue };
                if t.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let name = path
                    .strip_prefix(&self.root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                if let Some(kind) = FileKind::from_path(&path)
                    && name.to_lowercase().contains(&needle)
                    && path.is_file()
                {
                    entries.push(Entry {
                        percent: None,
                        path,
                        name,
                        kind: Some(kind),
                    });
                }
            }
        }
        entries.sort_by_cached_key(|e| e.name.to_lowercase());
        self.list = ListState::default().with_selected((!entries.is_empty()).then_some(0));
        if view == LocalView::Recent {
            entries.sort_by_cached_key(|e| {
                std::cmp::Reverse(
                    e.path
                        .metadata()
                        .ok()
                        .and_then(|m| m.created().or_else(|_| m.modified()).ok()),
                )
            });
        }
        self.view = view;
        self.entries = entries;
        self.error = None;
        Ok(())
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
        if self.view == LocalView::Reading {
            let previous = self
                .list
                .selected()
                .and_then(|i| self.entries.get(i))
                .map(|e| e.path.clone());
            self.browse(LocalView::Reading)?;
            if let Some(i) =
                previous.and_then(|path| self.entries.iter().position(|e| e.path == path))
            {
                self.list.select(Some(i));
            }
        }
        loop {
            term.draw(|frame| {
                self.draw(frame);
                crate::settings::paint(frame)
            })?;
            if self.covers.busy() && !event::poll(std::time::Duration::from_millis(60))? {
                self.covers.collect();
                continue;
            }
            self.covers.collect();
            let e = event::read()?;
            let click = match &e {
                Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                    Some(Position::new(m.column, m.row))
                }
                _ => None,
            };
            let key = match &e {
                Event::Key(k) if k.kind != KeyEventKind::Release => Some(k.code),
                _ => None,
            };
            if matches!(key, Some(KeyCode::Char('p' | 'P')))
                || click.is_some_and(|p| self.path_button.contains(p))
            {
                if let Some(value) = crate::prompt::edit(
                    term,
                    "Open library folder · paste or type a path",
                    "[ Open ↵ ]",
                    "",
                    |f| self.draw(f),
                )? && !value.is_empty()
                {
                    let path = Path::new(&value);
                    let path = if let Ok(tail) = path.strip_prefix("~") {
                        std::env::var_os("HOME")
                            .map(PathBuf::from)
                            .unwrap_or_default()
                            .join(tail)
                    } else {
                        self.dir.join(path)
                    };
                    let result = (|| -> io::Result<()> {
                        let path = path.canonicalize()?;
                        self.load(&path)?;
                        crate::settings::Settings::update(|s| s.library_root = Some(path.clone()))?;
                        self.root = path;
                        Ok(())
                    })();
                    if let Err(e) = result {
                        self.error = Some(format!("{} · {e}", path.display()));
                    }
                }
                continue;
            }
            if matches!(key, Some(KeyCode::Char('u' | 'U')))
                || click.is_some_and(|p| self.borders_button.contains(p))
            {
                if let Err(e) = crate::appearance::cycle() {
                    self.error = Some(e.to_string());
                }
                continue;
            }
            if matches!(key, Some(KeyCode::Char('f' | 'F')))
                || click.is_some_and(|p| self.bg_button.contains(p))
            {
                if let Err(e) = crate::settings::Settings::update(|s| {
                    s.opaque_background = !s.opaque_background
                }) {
                    self.error = Some(e.to_string());
                }
                continue;
            }
            if matches!(key, Some(KeyCode::Char('v' | 'V')))
                || click.is_some_and(|p| self.view_button.contains(p))
            {
                self.grid_view = !self.grid_view;
                if let Err(e) = crate::settings::Settings::update(|s| s.local_grid = self.grid_view)
                {
                    self.error = Some(e.to_string());
                }
                continue;
            }
            let step = self.grid.as_ref().map_or(1, |g| g.cols as isize);
            let page = self
                .grid
                .as_ref()
                .map_or(self.rows.height as isize, |g| g.len() as isize);
            let tab = match key {
                Some(KeyCode::Char('1')) => Some(0),
                Some(KeyCode::Char('2')) => Some(1),
                Some(KeyCode::Char('3')) => Some(2),
                _ => click.and_then(|p| self.shelves.iter().position(|r| r.contains(p))),
            };
            if let Some(i) = tab {
                if let Err(e) = self.browse(
                    [LocalView::Directory, LocalView::Reading, LocalView::Recent][i].clone(),
                ) {
                    self.error = Some(e.to_string());
                }
                continue;
            }
            if key == Some(KeyCode::Char('/'))
                || click.is_some_and(|p| self.search_button.contains(p))
            {
                let q = match &self.view {
                    LocalView::Search(q) => q.clone(),
                    _ => String::new(),
                };
                if let Some(q) =
                    crate::prompt::edit(term, "Search local library", "[ Search ↵ ]", &q, |f| {
                        self.draw(f)
                    })?
                    && let Err(e) = self.browse(LocalView::Search(q))
                {
                    self.error = Some(e.to_string());
                }
                continue;
            }
            if matches!(key, Some(KeyCode::Char('r' | 'R')))
                || click.is_some_and(|p| self.root_button.contains(p))
            {
                if self.view == LocalView::Directory {
                    match crate::settings::Settings::update(|s| {
                        s.library_root = Some(self.dir.clone())
                    }) {
                        Ok(_) => {
                            self.root = self.dir.clone();
                            self.error = Some(format!("Library saved · {}", self.root.display()));
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                continue;
            }
            if (matches!(key, Some(KeyCode::Esc | KeyCode::Backspace))
                || (key == Some(KeyCode::Left) && self.grid.is_none()))
                && self.view != LocalView::Directory
            {
                self.load(&self.dir.clone())?;
                continue;
            }
            if self.view == LocalView::Directory
                && (click.is_some_and(|p| self.up_button.contains(p))
                    || key == Some(KeyCode::Backspace)
                    || (key == Some(KeyCode::Left) && self.grid.is_none()))
            {
                let previous = self.dir.clone();
                if let Some(parent) = previous.parent() {
                    match self.load(parent) {
                        Ok(()) => {
                            if let Some(i) = self.entries.iter().position(|e| e.path == previous) {
                                self.list.select(Some(i));
                            }
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                continue;
            }
            let open = match e {
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    KeyCode::Right | KeyCode::Char('l') if self.grid.is_some() => {
                        self.move_selection(1);
                        None
                    }
                    KeyCode::Left | KeyCode::Char('h') if self.grid.is_some() => {
                        self.move_selection(-1);
                        None
                    }
                    KeyCode::Enter | KeyCode::Right => self.list.selected(),
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.move_selection(-step);
                        None
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.move_selection(step);
                        None
                    }
                    KeyCode::PageUp => {
                        self.move_selection(-page);
                        None
                    }
                    KeyCode::PageDown => {
                        self.move_selection(page);
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
                            let i = if let Some(g) = &self.grid {
                                let Some(i) = g.hit(Position::new(mouse.column, mouse.row)) else {
                                    continue;
                                };
                                i
                            } else {
                                self.list.offset() + usize::from(mouse.row - self.rows.y)
                            };
                            if i < self.entries.len() {
                                self.list.select(Some(i));
                                Some(i)
                            } else {
                                None
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            self.move_selection(step);
                            None
                        }
                        MouseEventKind::ScrollUp => {
                            self.move_selection(-step);
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
