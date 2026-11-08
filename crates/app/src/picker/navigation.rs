//! A lazy folder tree. Opening a node reads only that folder's direct children.
use super::{Entry, LocalView};
use crate::splitter::{Pane, Splitter};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    widgets::Paragraph,
};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Node {
    Collections,
    Folders,
    Folder(PathBuf),
    Reading,
    Recent,
}

pub(super) struct Navigation {
    pub splitter: Splitter,
    anchor: PathBuf,
    open: HashMap<PathBuf, Vec<PathBuf>>,
    show_tree: bool,
    rows: Vec<(Node, usize)>,
    active: Option<Node>,
    cursor: usize,
    offset: usize,
    visible: Vec<usize>,
    focused: bool,
    viewport: Rect,
}

impl Navigation {
    pub fn new(width: Option<u16>) -> Self {
        Self {
            splitter: Splitter::new(Pane::Library, width),
            anchor: PathBuf::new(),
            open: HashMap::new(),
            show_tree: true,
            rows: Vec::new(),
            active: None,
            cursor: 0,
            offset: 0,
            visible: Vec::new(),
            focused: false,
            viewport: Rect::default(),
        }
    }

    pub fn visit(&mut self, root: &Path, dir: &Path, entries: &[Entry]) {
        // Browsing Up can leave the saved search root. Show their common parent
        // without changing the user's saved library or recursively scanning disk.
        let anchor = dir.ancestors().find(|p| root.starts_with(p)).unwrap_or(dir);
        if anchor != self.anchor {
            self.open.retain(|p, _| p.starts_with(anchor));
            self.anchor = anchor.to_owned();
        }
        for parent in dir
            .ancestors()
            .skip(1)
            .take_while(|p| p.starts_with(anchor))
        {
            if !self.open.contains_key(parent)
                && let Ok(children) = folders(parent)
            {
                self.open.insert(parent.to_owned(), children);
            }
        }
        self.open.insert(
            dir.to_owned(),
            entries
                .iter()
                .take_while(|e| e.kind.is_none())
                .map(|e| e.path.clone())
                .collect(),
        );
        self.show_tree = true;
        self.active = Some(Node::Folder(dir.to_owned()));
        self.rebuild(self.active.clone());
    }

    pub fn show_shelf(&mut self, view: &LocalView) {
        self.active = match view {
            LocalView::Reading => Some(Node::Reading),
            LocalView::Collection(_) => Some(Node::Collections),
            LocalView::Recent => Some(Node::Recent),
            _ => None,
        };
        if let Some(i) = self
            .rows
            .iter()
            .position(|(n, _)| Some(n) == self.active.as_ref())
        {
            self.cursor = i;
        }
    }

    fn rebuild(&mut self, select: Option<Node>) {
        self.rows.clear();
        self.rows.push((Node::Folders, 0));
        if self.show_tree {
            let mut pending = vec![(self.anchor.clone(), 1)];
            while let Some((path, depth)) = pending.pop() {
                if let Some(children) = self.open.get(&path) {
                    pending.extend(children.iter().rev().map(|p| (p.clone(), depth + 1)));
                }
                self.rows.push((Node::Folder(path), depth));
            }
        }
        self.rows.extend([
            (Node::Reading, 0),
            (Node::Recent, 0),
            (Node::Collections, 0),
        ]);
        let index = self
            .rows
            .iter()
            .position(|(node, _)| Some(node) == select.as_ref());
        self.cursor = index.unwrap_or(0);
    }

    pub fn visible_panel(&self) -> bool {
        !self.viewport.is_empty()
    }

    pub fn layout(&mut self, frame: &mut Frame, area: Rect) -> Rect {
        let (nav, main) = self.splitter.layout(area, area.height >= 16);
        if nav.is_empty() {
            self.viewport = Rect::default();
            self.focused = false;
            return main;
        }
        let mut block = crate::appearance::block()
            .title(if nav.width >= 18 {
                " Navigation · F6 "
            } else {
                " F6 Browse "
            })
            .border_style(crate::appearance::border_style(self.focused));
        if self.focused {
            block = block.title_bottom(" ←/→ Fold · ↵ Open ");
        }
        self.viewport = block.inner(nav);
        frame.render_widget(block, nav);
        // Pin the section shortcuts while only the folder rows scroll.
        let folders = self.rows.len().saturating_sub(4);
        let capacity = usize::from(self.viewport.height.saturating_sub(4));
        self.offset = self.offset.min(folders.saturating_sub(capacity));
        if (1..=folders).contains(&self.cursor) {
            self.offset = self
                .offset
                .min(self.cursor - 1)
                .max(self.cursor.saturating_sub(capacity));
        }
        self.visible.clear();
        self.visible.push(0);
        self.visible
            .extend((1 + self.offset..=folders).take(capacity));
        self.visible.extend([folders + 1, folders + 2, folders + 3]);
        for (line, &index) in self.visible.iter().enumerate() {
            let (node, depth) = &self.rows[index];
            let (marker, label) = match node {
                Node::Folders => (if self.show_tree { "▾" } else { "▸" }, "1 Folders".into()),
                Node::Collections => (" ", "4 Collections".into()),
                Node::Reading => (
                    " ",
                    if self.viewport.width >= 21 {
                        "2 Currently reading"
                    } else {
                        "2 Reading"
                    }
                    .into(),
                ),
                Node::Recent => (
                    " ",
                    if self.viewport.width >= 18 {
                        "3 Recently added"
                    } else {
                        "3 Recent"
                    }
                    .into(),
                ),
                Node::Folder(path) => {
                    let marker = match self.open.get(path) {
                        Some(children) if children.is_empty() => "·",
                        Some(_) => "▾",
                        None => "▸",
                    };
                    (
                        marker,
                        path.file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy(),
                    )
                }
            };
            let indent = (depth * 2).min(usize::from(self.viewport.width.saturating_sub(6)));
            let mut style = Style::new();
            if Some(node) == self.active.as_ref()
                || (node == &Node::Folders
                    && !self.show_tree
                    && matches!(self.active, Some(Node::Folder(_))))
            {
                style = style.cyan().bold();
            }
            if self.focused && index == self.cursor {
                style = style.reversed();
            }
            let row = Rect::new(
                self.viewport.x,
                self.viewport.y + line as u16,
                self.viewport.width,
                1,
            );
            let gap = if self.viewport.width <= 10 { "" } else { " " };
            frame.render_widget(
                Paragraph::new(format!("{:indent$}{marker}{gap}{label}", "")).style(style),
                row,
            );
        }
        main
    }

    fn selected(&self) -> Option<Node> {
        self.rows.get(self.cursor).map(|(n, _)| n.clone())
    }

    fn move_by(&mut self, delta: isize) {
        self.cursor = self
            .cursor
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
    }

    fn fold(&mut self, node: Node, expand: bool) -> io::Result<()> {
        let mut next = node.clone();
        match &node {
            Node::Folders => self.show_tree = expand,
            Node::Folder(path) if expand => {
                if !self.open.contains_key(path) {
                    self.open.insert(path.clone(), folders(path)?);
                } else if let Some(child) = self.open[path].first() {
                    next = Node::Folder(child.clone());
                }
            }
            Node::Folder(path) => {
                if self.open.remove(path).is_none() {
                    next = if path == &self.anchor {
                        Node::Folders
                    } else {
                        path.parent()
                            .map(|p| Node::Folder(p.to_owned()))
                            .unwrap_or(Node::Folders)
                    };
                }
            }
            _ => return Ok(()),
        }
        self.rebuild(Some(next));
        Ok(())
    }

    fn toggle(&mut self, node: Node) -> io::Result<()> {
        let expanded = match &node {
            Node::Folders => self.show_tree,
            Node::Folder(path) => self.open.contains_key(path),
            _ => return Ok(()),
        };
        self.fold(node, !expanded)
    }

    fn activate(&mut self, node: Node) -> Option<io::Result<Option<Node>>> {
        self.focused = false;
        Some(Ok(Some(node)))
    }

    pub fn input(&mut self, event: &Event) -> Option<io::Result<Option<Node>>> {
        if self.viewport.is_empty() {
            return None;
        }
        let bottom = self.rows.len().saturating_sub(1);
        match event {
            Event::Key(k) if k.kind != KeyEventKind::Release && k.code == KeyCode::F(6) => {
                self.focused = !self.focused;
            }
            Event::Key(k) if self.focused && k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => self.focused = false,
                KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = bottom,
                KeyCode::PageUp => self.move_by(-(self.viewport.height as isize)),
                KeyCode::PageDown => self.move_by(self.viewport.height as isize),
                KeyCode::Enter => return self.activate(self.selected()?),
                KeyCode::Char(' ') => return Some(self.toggle(self.selected()?).map(|_| None)),
                KeyCode::Left | KeyCode::Right => {
                    return Some(
                        self.fold(self.selected()?, k.code == KeyCode::Right)
                            .map(|_| None),
                    );
                }
                _ => {
                    self.focused = false;
                    return None;
                }
            },
            Event::Mouse(m) => {
                let p = Position::new(m.column, m.row);
                if !self.viewport.contains(p) {
                    if m.kind == MouseEventKind::Down(MouseButton::Left) {
                        self.focused = false;
                    }
                    return None;
                }
                match m.kind {
                    MouseEventKind::ScrollUp => {
                        self.focused = true;
                        self.move_by(-3);
                    }
                    MouseEventKind::ScrollDown => {
                        self.focused = true;
                        self.move_by(3);
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some(&index) = self.visible.get(usize::from(m.row - self.viewport.y))
                        else {
                            return Some(Ok(None));
                        };
                        let Some((node, depth)) = self.rows.get(index) else {
                            return Some(Ok(None));
                        };
                        let node = node.clone();
                        let marker =
                            (*depth * 2).min(usize::from(self.viewport.width.saturating_sub(6)));
                        self.cursor = index;
                        self.focused = true;
                        if matches!(node, Node::Folders | Node::Folder(_))
                            && usize::from(p.x - self.viewport.x) == marker
                        {
                            return Some(self.toggle(node).map(|_| None));
                        }
                        return self.activate(node);
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
        Some(Ok(None))
    }
}

fn folders(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with('.') && entry.path().is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort_by_cached_key(|p| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    Ok(paths)
}
