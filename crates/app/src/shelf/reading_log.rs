use super::{Command, Loaded, Pane, Row, Section, Shelf};
use botui_bookorbit::reading_log::{ReadingLog, ReadingSession};
use std::collections::HashSet;

fn duration(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
    }
}

fn timestamp(s: &str) -> String {
    if let Some((day, time)) = s.strip_suffix('Z').and_then(|v| v.split_once('T'))
        && let Some(minute) = time.get(..5)
    {
        return format!("{day} {minute} UTC");
    }
    s.to_owned()
}

pub(super) fn rows(items: Vec<ReadingSession>, seen: &mut HashSet<u64>) -> Vec<Row> {
    let mut rows = Vec::new();
    for session in items {
        if !seen.insert(session.id) {
            continue;
        }
        let date = timestamp(&session.started_at);
        let end = timestamp(&session.ended_at);
        let elapsed = duration(session.duration_seconds);
        let format = session.format.as_deref().unwrap_or("—").to_uppercase();
        let source = session.source.as_deref().unwrap_or("—");
        let progress = session
            .end_progress
            .map_or_else(|| "—".into(), |v| format!("{v:.0}%"));
        let label = format!("{date}   {elapsed:>8}   {format:<5}   {progress:>4}   {source}");
        let mut detail = format!(
            "Reading session\n\nStarted   {date}\nEnded     {end}\nDuration  {elapsed}\nFormat    {format}\nProgress  {progress}\nSource    {source}"
        );
        if let Some(mode) = session.session_type {
            detail.push_str(&format!("\nMode      {mode}"));
        }
        rows.push(Row::new(label, Command::SessionDetail(detail)));
    }
    rows
}

impl Shelf {
    pub(super) fn reading_log(&mut self, book: u64) {
        if let Some(session) = self.session.clone() {
            self.fetch("Loading reading log…", move |_| {
                session
                    .with_token(|api, t| api.reading_log(t, book, 1))
                    .map(|log| Loaded::ReadingLog(book, log))
            });
        }
    }
    pub(super) fn show_reading_log(&mut self, book: u64, log: ReadingLog) {
        let title = format!(
            "Reading log · {} sessions · {} · {}",
            log.total,
            duration(log.stats.total_seconds),
            self.current().title
        );
        let next = log.next();
        let mut seen = HashSet::new();
        let rows = rows(log.items, &mut seen);
        self.stack.push(Pane::new(
            Section::ReadingLog { book, next, seen },
            title,
            rows,
        ));
    }
}
