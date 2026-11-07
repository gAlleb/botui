//! A wide terminal can keep navigation in sight while browsing book jackets.
use super::{
    Command, Directory, Scope, Section, Session, Tab,
    background::{Ticket, Worker},
};
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
#[derive(Default)]
pub(super) struct Rail {
    data: Sidebar,
    work: Option<Ticket<Sidebar>>,
    hits: Vec<(Rect, Command)>,
}
impl Rail {
    pub fn clear_hits(&mut self) {
        self.hits.clear();
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
    pub fn hit(&self, p: Position) -> Option<Command> {
        self.hits
            .iter()
            .find(|(a, _)| a.contains(p))
            .map(|(_, c)| c.clone())
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
        f.render_widget(
            crate::appearance::divider().border_style(Style::new().dim()),
            area,
        );
        let mut y = area.y;
        let mut draw = |label: String, on: bool, heading: bool, cmd: Command| {
            if y >= area.bottom() {
                return;
            }
            let r = Rect::new(area.x, y, area.width.saturating_sub(1), 1);
            let style = if on {
                Style::new().fg(Color::Cyan).bold()
            } else if heading {
                Style::new().bold()
            } else {
                Style::default()
            };
            let count = match &cmd {
                Command::Books(Scope::Library(l), _) => l.book_count,
                Command::Books(Scope::Collection(c), _) => Some(c.book_count),
                Command::OpenTab(Tab::Groups(Directory::Authors)) => self.data.authors,
                Command::OpenTab(Tab::Groups(Directory::Series)) => self.data.series,
                _ => None,
            }
            .map(|n| n.to_string());
            let cap = count
                .as_ref()
                .map_or(0, |n| n.len() as u16 + 2)
                .min(r.width);
            f.render_widget(
                Paragraph::new(format!(
                    "{}{}",
                    if heading {
                        if on { "▸ " } else { "  " }
                    } else if on {
                        "  ▸ "
                    } else {
                        "    "
                    },
                    super::gallery::caption(
                        &super::single_line(label),
                        r.width.saturating_sub(cap + if heading { 2 } else { 4 }),
                        1
                    )
                ))
                .style(style),
                r,
            );
            if let Some(n) = count {
                f.render_widget(
                    Paragraph::new(n).right_aligned().style(Style::new().dim()),
                    Rect::new(r.right() - cap, r.y, cap.saturating_sub(1), 1),
                );
            }
            self.hits.push((r, cmd));
            y += 1;
        };
        let mut room = (area.height as usize).saturating_sub(Tab::ALL.len());
        for (index, tab) in Tab::ALL.into_iter().enumerate() {
            let group = matches!(tab, Tab::Libraries | Tab::Collections);
            draw(
                format!("{} {}", index, tab.titles()[0]),
                active == tab && (!group || scope.is_none()),
                true,
                Command::OpenTab(tab),
            );
            if !group {
                continue;
            }
            {
                for group in self
                    .data
                    .groups
                    .iter()
                    .filter(|g| g.tab() == tab)
                    .take(room)
                {
                    room -= 1;
                    draw(
                        format!("  {}", group.name()),
                        Some(group) == scope,
                        false,
                        Command::Books(group.clone(), 0),
                    );
                }
            }
        }
    }
}
pub(super) fn current_scope(section: &Section) -> Option<&Scope> {
    match section {
        Section::Books { scope, .. } => Some(scope),
        _ => None,
    }
}
