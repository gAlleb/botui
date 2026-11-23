//! Print the original PDF independently of reader zoom, theme and position.
use std::{io, path::Path, process::Command};

pub fn shortcut(event: &crossterm::event::Event) -> bool {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    matches!(event, Event::Key(k) if k.kind != KeyEventKind::Release
        && k.code == KeyCode::Char('p') && k.modifiers == KeyModifiers::CONTROL)
}

pub fn pdf(path: &Path) -> io::Result<()> {
    let result = Command::new("python3")
        .args(["-I", "-c", include_str!("print.py")])
        .arg(path.canonicalize()?)
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&result.stderr).trim().to_owned(),
        ));
    }
    Ok(())
}
