//! A wide terminal can keep navigation in sight while browsing book jackets.
use super::{
    Command, Directory, Scope, Section, Session, Tab,
    background::{Ticket, Worker},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};
use std::sync::{Arc, mpsc::TryRecvError};
#[derive(Default)]
struct Sidebar {
    groups: Vec<Scope>,
    authors: Option<u64>,
    series: Option<u64>,
}
pub(super) struct Rail {
    pub splitter: crate::splitter::Splitter,
    data: Sidebar,
    work: Option<Ticket<Sidebar>>,
    hits: Vec<(Rect, usize)>,
    rows: Vec<Command>,
    viewport: Rect,
    focused: bool,
    cursor: usize,
    offset: usize,
    folded: [bool; 2],
}
impl Rail {
    pub fn clear_hits(&mut self) {
        self.hits.clear();
        self.viewport = Rect::default();
    }
    pub fn busy(&self) -> bool {
        self.work.is_some()
    }
    pub fn collect(&mut self) {
        if let Some(job) = &self.work {
            match job.receiver.try_recv() {
                Ok(mut groups) => {
                    groups.authors = groups.authors.or(self.data.authors);
                    groups.series = groups.series.or(self.data.series);
                    self.data = groups;
                    self.work = None;
                }
                Err(TryRecvError::Disconnected) => self.work = None,
                Err(TryRecvError::Empty) => (),
            }
        }
    }
    pub fn saved() -> Self {
        let c = crate::settings::Settings::load().unwrap_or_default();
        Self {
            splitter: crate::splitter::Splitter::new(
                crate::splitter::Pane::Library,
                c.sidebar_width,
            ),
            data: Sidebar::default(),
            work: None,
            hits: vec![],
            rows: vec![],
            viewport: Rect::default(),
            focused: false,
            cursor: 0,
            offset: 0,
            folded: [
                c.bookorbit.libraries_collapsed,
                c.bookorbit.collections_collapsed,
            ],
        }
    }
    fn slot(tab: Tab) -> Option<usize> {
        match tab {
            Tab::Libraries => Some(0),
            Tab::Collections => Some(1),
            _ => None,
        }
    }
    fn parent(&self) -> Option<Tab> {
        match self.rows.get(self.cursor)? {
            Command::OpenTab(t) => Some(*t),
            Command::Books(s, _) => Some(s.tab()),
            _ => None,
        }
    }
    fn fold(&mut self, t: Tab, value: bool) -> std::io::Result<()> {
        let Some(i) = Self::slot(t) else {
            return Ok(());
        };
        if self.folded[i] == value {
            return Ok(());
        }
        crate::settings::Settings::update(|p| {
            if i == 0 {
                p.bookorbit.libraries_collapsed = value
            } else {
                p.bookorbit.collections_collapsed = value
            }
        })?;
        self.folded[i] = value;
        self.cursor = self
            .rows
            .iter()
            .position(|c| matches!(c,Command::OpenTab(tab) if *tab==t))
            .unwrap_or(0);
        Ok(())
    }
    fn activate(&mut self) -> std::io::Result<Option<Command>> {
        let command = self.rows.get(self.cursor).cloned();
        if let Some(Command::OpenTab(t)) = &command
            && let Some(i) = Self::slot(*t)
        {
            self.fold(*t, !self.folded[i])?;
            return Ok(None);
        }
        self.focused = false;
        Ok(command)
    }
    pub fn handle(&mut self, e: &Event) -> Option<std::io::Result<Option<Command>>> {
        if self.viewport.is_empty() {
            return None;
        }
        let bottom = self.rows.len().saturating_sub(1);
        match e {
            Event::Key(k) if k.kind != KeyEventKind::Release && k.code == KeyCode::F(6) => {
                self.focused = !self.focused
            }
            Event::Key(k) if self.focused && k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc => self.focused = false,
                KeyCode::Down | KeyCode::Char('j') => self.cursor = (self.cursor + 1).min(bottom),
                KeyCode::Up | KeyCode::Char('k') => self.cursor = self.cursor.saturating_sub(1),
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = bottom,
                KeyCode::Enter | KeyCode::Char(' ') => return Some(self.activate()),
                KeyCode::Left | KeyCode::Right => {
                    if let Some(tab) = self.parent() {
                        return Some(self.fold(tab, k.code == KeyCode::Left).map(|_| None));
                    }
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
                    MouseEventKind::Down(MouseButton::Left) => {
                        let (_, i) = self.hits.iter().find(|(r, _)| r.contains(p))?;
                        self.cursor = *i;
                        self.focused = true;
                        return Some(self.activate());
                    }
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        let max = self
                            .rows
                            .len()
                            .saturating_sub(self.viewport.height as usize);
                        self.offset = self
                            .offset
                            .saturating_add_signed(if m.kind == MouseEventKind::ScrollUp {
                                -3
                            } else {
                                3
                            })
                            .min(max);
                        self.cursor = self.cursor.clamp(
                            self.offset,
                            (self.offset + self.viewport.height as usize - 1).min(bottom),
                        );
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
        Some(Ok(None))
    }
    pub fn refresh(&mut self, session: Arc<Session>) {
        self.work = Some(Worker::new().start(move |_| {
            let mut groups = vec![];
            if let Ok(l) = session.with_token(|a, t| a.libraries(t)) {
                groups.extend(l.into_iter().map(Scope::Library));
            }
            if let Ok(c) = session.with_token(|a, t| a.collections(t)) {
                groups.extend(c.into_iter().map(Scope::Collection));
            }
            let total = |d| {
                session
                    .with_token(|a, t| a.directory(t, d, "", 0, 1))
                    .ok()
                    .map(|page| page.total)
            };
            Sidebar {
                groups,
                authors: total(Directory::Authors),
                series: total(Directory::Series),
            }
        }));
    }
    pub fn set_total(&mut self, directory: Directory, total: u64) {
        match directory {
            Directory::Authors => self.data.authors = Some(total),
            Directory::Series => self.data.series = Some(total),
        }
    }
    pub fn count(&self, tab: Tab) -> Option<u64> {
        match tab {
            Tab::Groups(Directory::Authors) => self.data.authors,
            Tab::Groups(Directory::Series) => self.data.series,
            _ => None,
        }
    }
    pub fn draw(&mut self, f: &mut Frame, area: Rect, active: Tab, scope: Option<&Scope>) {
        let mut edge = crate::appearance::block()
            .title(" Navigation · F6 ")
            .border_style(if self.focused {
                Style::new().cyan().bold()
            } else {
                Style::new().dim()
            });
        if self.focused {
            edge = edge.title_bottom(" ←/→ Fold · F6 Back ");
        }
        let inner = edge.inner(area);
        f.render_widget(edge, area);
        self.viewport = inner;
        self.rows.clear();
        for tab in Tab::ALL {
            self.rows.push(Command::OpenTab(tab));
            if Self::slot(tab).is_some_and(|i| !self.folded[i]) {
                self.rows.extend(
                    self.data
                        .groups
                        .iter()
                        .filter(|g| g.tab() == tab)
                        .map(|s| Command::Books(s.clone(), 0)),
                );
            }
        }
        if inner.is_empty() {
            return;
        }
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
        self.offset = self
            .offset
            .min(self.cursor)
            .max((self.cursor + 1).saturating_sub(inner.height as usize))
            .min(self.rows.len().saturating_sub(inner.height as usize));
        for (line, (i, cmd)) in self
            .rows
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(inner.height as usize)
            .enumerate()
        {
            let (prefix, label, count, on, heading) = match cmd {
                Command::OpenTab(t) => {
                    let is_group = Self::slot(*t);
                    let on = active == *t
                        && (is_group.is_none()
                            || scope.is_none()
                            || is_group.is_some_and(|v| self.folded[v]));
                    let prefix = match is_group {
                        Some(i) if self.folded[i] => "▸ ",
                        Some(_) => "▾ ",
                        None if on => "▸ ",
                        _ => "  ",
                    };
                    let number = Tab::ALL.iter().position(|v| v == t).unwrap_or(0);
                    (
                        prefix,
                        format!("{} {}", number, t.titles()[0]),
                        self.count(*t),
                        on,
                        true,
                    )
                }
                Command::Books(group, _) => {
                    let on = scope == Some(group);
                    let count = match group {
                        Scope::Library(l) => l.book_count,
                        Scope::Collection(c) => Some(c.book_count),
                        _ => None,
                    };
                    (
                        if on { "     ▸ " } else { "       " },
                        super::single_line(group.name().to_owned()),
                        count,
                        on,
                        false,
                    )
                }
                _ => continue,
            };
            let r = Rect::new(inner.x, inner.y + line as u16, inner.width, 1);
            let n = count.map(|c| c.to_string());
            let cap = n.as_ref().map_or(0, |n| n.len() as u16 + 2).min(r.width);
            let style = if self.focused && self.cursor == i {
                Style::new().cyan().reversed()
            } else if on {
                Style::new().fg(Color::Cyan).bold()
            } else if heading {
                Style::new().bold()
            } else {
                Style::default()
            };
            let label = super::gallery::caption(
                &label,
                r.width.saturating_sub(cap + prefix.chars().count() as u16),
                1,
            );
            f.render_widget(Paragraph::new(format!("{prefix}{label}")).style(style), r);
            if let Some(n) = n {
                f.render_widget(
                    Paragraph::new(n).right_aligned().style(style.dim()),
                    Rect::new(r.right() - cap, r.y, cap.saturating_sub(1), 1),
                );
            }
            self.hits.push((r, i));
        }
    }
}
pub(super) fn current_scope(section: &Section) -> Option<&Scope> {
    match section {
        Section::Books { scope, .. } => Some(scope),
        _ => None,
    }
}
