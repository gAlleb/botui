//! Print the complete document independently of reader zoom, theme and position.
use std::{fs, io, path::Path, process::Command};

pub fn shortcut(event: &crossterm::event::Event) -> bool {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    matches!(event, Event::Key(k) if k.kind != KeyEventKind::Release
        && k.code == KeyCode::Char('p') && k.modifiers == KeyModifiers::CONTROL)
}

pub fn document(path: &Path, image_pages: Option<&[String]>) -> io::Result<()> {
    let mut command = Command::new("python3");
    command
        .args(["-I", "-c", include_str!("print.py")])
        .arg(path.canonicalize()?);
    // Keep the reader's page order, including RAR member IDs and fixed-layout EPUBs.
    // Generated numeric names prevent archive paths from becoming filesystem paths.
    let spool;
    if let Some(pages) = image_pages {
        spool = tempfile::tempdir()?;
        for (index, member) in pages.iter().enumerate() {
            let bytes = botui_formats::illustration(path, member)
                .map_err(|error| io::Error::other(error.to_string()))?;
            fs::write(spool.path().join(index.to_string()), bytes)?;
        }
        command.arg(spool.path()).arg(pages.len().to_string());
    }
    let result = command.output()?;
    if !result.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&result.stderr).trim().to_owned(),
        ));
    }
    Ok(())
}
