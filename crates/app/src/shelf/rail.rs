//! A wide terminal can keep navigation in sight while browsing book jackets.
use super::{
    Command, Scope, Section, Session, Tab,
    background::{Ticket, Worker},
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::{
    sync::{Arc, mpsc::TryRecvError},
    time::{Duration, Instant},
};
#[derive(Default)]
pub(super) struct Rail {
    groups: Option<Vec<Scope>>,
    work: Option<Ticket<Vec<Scope>>>,
    last: Option<Instant>,
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
                Ok(groups) => {
                    self.groups = Some(groups);
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
    pub fn draw(
        &mut self,
        f: &mut Frame,
        area: Rect,
        session: Arc<Session>,
        active: Tab,
        scope: Option<&Scope>,
    ) {
        if self.groups.is_none()
            && self.work.is_none()
            && self
                .last
                .is_none_or(|i| i.elapsed() > Duration::from_secs(30))
        {
            self.last = Some(Instant::now());
            self.work = Some(Worker::new().start(move |_| {
                let mut groups = vec![];
                if let Ok(l) = session.with_token(|a, t| a.libraries(t)) {
                    groups.extend(l.into_iter().map(Scope::Library));
                }
                if let Ok(c) = session.with_token(|a, t| a.collections(t)) {
                    groups.extend(c.into_iter().map(Scope::Collection));
                }
                groups
            }));
        }
        f.render_widget(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::new().dim()),
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
        draw(
            "1 All books".into(),
            active == Tab::All,
            true,
            Command::OpenTab(Tab::All),
        );
        draw(
            "4 Currently reading".into(),
            active == Tab::Reading,
            true,
            Command::OpenTab(Tab::Reading),
        );
        draw(
            "5 Recently added".into(),
            active == Tab::Recent,
            true,
            Command::OpenTab(Tab::Recent),
        );
        for (tab, name) in [
            (Tab::Libraries, "2 Libraries"),
            (Tab::Collections, "3 Collections"),
        ] {
            draw(
                name.into(),
                active == tab && scope.is_none(),
                true,
                Command::OpenTab(tab),
            );
            if let Some(groups) = &self.groups {
                let max = if tab == Tab::Libraries {
                    area.height.saturating_sub(7) as usize
                } else {
                    area.height as usize
                };
                for group in groups.iter().filter(|g| g.tab() == tab).take(max) {
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
