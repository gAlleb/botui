//! An optional publisher-layout preview. Its PDF page numbers are not EPUB CFIs.
use crate::AppTerminal;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use std::{
    fs, io,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;

#[derive(Default)]
pub(super) struct Preview(Option<TempDir>);

impl Preview {
    pub(super) fn open(
        &mut self,
        term: &mut AppTerminal,
        path: &Path,
        notices: &mut Vec<String>,
    ) -> io::Result<()> {
        use botui_formats::FileKind;
        if FileKind::from_path(path) != Some(FileKind::Epub) {
            return crate::prompt::message(
                term,
                "Publisher layout",
                "Publisher layout is available for EPUB books.",
            );
        }
        if self.0.is_none() {
            term.draw(|frame| {
                frame.render_widget(
                    ratatui::widgets::Paragraph::new("Preparing publisher layout…   Esc Cancel"),
                    frame.area(),
                )
            })?;
            let dir = tempfile::tempdir()?;
            // Serve only this book. The source directory may contain unrelated private files.
            let input = dir.path().join("book.epub");
            if fs::hard_link(path.canonicalize()?, &input).is_err() {
                fs::copy(path, &input)?;
            }
            let log = dir.path().join("render.log");
            let mut command = Command::new("vivliostyle");
            command.current_dir(dir.path()).args([
                "build",
                "book.epub",
                "--size",
                "A5",
                "--no-vite-config-file",
                "--viewer-param",
                "allowScripts=false",
                "--host",
                "127.0.0.1",
                "--timeout",
                "120",
                "-o",
                "preview.pdf",
            ]);
            if let Some(browser) = std::env::var_os("BOTUI_PUBLISHER_BROWSER") {
                command.arg("--executable-browser").arg(browser);
            }
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
            let mut child = match command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(fs::File::create(&log)?)
                .spawn()
            {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return crate::prompt::message(
                        term,
                        "Publisher layout",
                        "Install @vivliostyle/cli to enable EPUB publisher layout.",
                    );
                }
                other => other?,
            };
            let status = loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                let event = crate::settings::read_event_for(Some(Duration::from_millis(100)));
                if event.is_err()
                    || matches!(event, Ok(Some(Event::Key(k))) if k.kind != KeyEventKind::Release && k.code == KeyCode::Esc)
                {
                    #[cfg(unix)]
                    {
                        // Stop the converter and its headless browser together.
                        let _ = Command::new("kill")
                            .args(["-KILL", "--"])
                            .arg(format!("-{}", child.id()))
                            .status();
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    return event.map(|_| ());
                }
            };
            if !status.success() {
                let log = fs::read_to_string(log).unwrap_or_default();
                let reason = log
                    .lines()
                    .find(|s| s.contains("ERROR"))
                    .unwrap_or("Check the Vivliostyle CLI and its browser installation.");
                return crate::prompt::message(
                    term,
                    "Publisher layout",
                    &format!(
                        "Could not render EPUB. {}",
                        reason
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(250)
                            .collect::<String>()
                    ),
                );
            }
            self.0 = Some(dir);
        }
        let pdf = self.0.as_ref().unwrap().path().join("preview.pdf");
        crate::scan::read_server(term, &pdf, FileKind::Pdf, notices, None)?;
        Ok(())
    }
}
