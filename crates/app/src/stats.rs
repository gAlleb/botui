use botui_store::LocalState;
use std::{io, time::Instant};
fn elapsed(n: u64) -> String {
    if n < 60 {
        format!("{n} s")
    } else if n < 3600 {
        format!("{} min {} s", n / 60, n % 60)
    } else {
        format!("{} h {} min", n / 3600, (n % 3600) / 60)
    }
}
pub fn show(
    term: &mut crate::AppTerminal,
    store: Option<&LocalState>,
    opened: Instant,
    pct: f64,
) -> io::Result<()> {
    let prev = store
        .map(LocalState::stats)
        .transpose()?
        .unwrap_or_default();
    let now = opened.elapsed().as_secs();
    let rows = [
        format!(" Progress                 {pct:.1}%"),
        format!(
            " Sessions on this device  {}",
            prev.sessions.saturating_add(1)
        ),
        format!(" This session             {}", elapsed(now)),
        format!(
            " Total time book open     {}",
            elapsed(prev.seconds.saturating_add(now))
        ),
    ];
    crate::prompt::choose(term, "Reading statistics", &rows, 0).map(|_| ())
}
pub fn finish(store: Option<&LocalState>, opened: Instant, notices: &mut Vec<String>) {
    if let Some(store) = store
        && let Err(e) = store.finish_session(opened.elapsed().as_secs())
    {
        notices.push(format!("Could not save reading statistics: {e}"));
    }
}
