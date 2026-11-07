use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

use botui_formats::FileKind;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
    text::Line,
    widgets::{Clear, Paragraph, Wrap},
};

use crate::{AppTerminal, centered};

struct FileInfo {
    name: String,
    format: String,
    size: Option<u64>,
}

impl FileInfo {
    fn load(path: &Path, kind: FileKind) -> Self {
        let file = File::open(path).ok();
        let size = file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map(|m| m.len());
        let mut header = Vec::new();
        let detected = file.and_then(|f| {
            f.take(1024).read_to_end(&mut header).ok()?;
            if header.starts_with(b"AT&TFORM")
                && matches!(header.get(12..16), Some(b"DJVU" | b"DJVM"))
            {
                Some("DjVu")
            } else if header.windows(5).any(|w| w == b"%PDF-") {
                Some("PDF")
            } else {
                None
            }
        });
        Self {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .collect(),
            format: detected
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{} (from extension)", kind.label())),
            size,
        }
    }

    fn draw(&self, frame: &mut Frame) -> Rect {
        let area = frame.area();
        let panel = centered(area, 72, 10);
        let block = crate::appearance::block().title(" Book details ");
        let inner = block.inner(panel);
        frame.render_widget(Clear, area);
        frame.render_widget(block, panel);
        let mut lines = vec![
            Line::from(self.name.as_str()).bold(),
            Line::from(format!("Format: {}", self.format)),
        ];
        if let Some(size) = self.size {
            lines.push(Line::from(format!("Size: {size} bytes")));
        }
        let body = Rect {
            height: inner.height.saturating_sub(1),
            ..inner
        };
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
        let back = Rect::new(
            inner.x,
            inner.bottom().saturating_sub(1),
            inner.width.min(15),
            inner.height.min(1),
        );
        frame.render_widget(Paragraph::new("[ Back · Esc ]").reversed(), back);
        let open = open_button(back, inner.right());
        frame.render_widget(Paragraph::new("[ P Read ]"), open);
        let external = external_button(open, inner.right());
        frame.render_widget(Paragraph::new("[ X External viewer ]"), external);
        back
    }
}

fn open_button(back: Rect, right: u16) -> Rect {
    let x = back.right().saturating_add(1);
    Rect::new(x, back.y, right.saturating_sub(x).min(14), back.height)
}
fn external_button(open: Rect, right: u16) -> Rect {
    let x = open.right().saturating_add(1);
    Rect::new(x, open.y, right.saturating_sub(x).min(24), open.height)
}
fn dismiss(input: Event, back: Rect) -> bool {
    match input {
        Event::Key(k) if k.kind != KeyEventKind::Release => {
            matches!(
                k.code,
                KeyCode::Esc
                    | KeyCode::Enter
                    | KeyCode::Backspace
                    | KeyCode::Left
                    | KeyCode::Char('q')
            ) || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL))
        }
        Event::Mouse(m) => {
            m.kind == MouseEventKind::Down(MouseButton::Left)
                && back.contains(Position::new(m.column, m.row))
        }
        _ => false,
    }
}

pub(crate) fn show_info(term: &mut AppTerminal, path: &Path, kind: FileKind) -> io::Result<bool> {
    let info = FileInfo::load(path, kind);
    loop {
        let mut back = Rect::default();
        let mut open = Rect::default();
        let mut external = Rect::default();
        term.draw(|f| {
            back = info.draw(f);
            crate::settings::paint_interface(f);
            let right = centered(f.area(), 72, 10).right().saturating_sub(1);
            open = open_button(back, right);
            external = external_button(open, right);
        })?;
        let e = crate::settings::read_event()?;
        if matches!(e,Event::Key(k) if k.kind!=KeyEventKind::Release && matches!(k.code,KeyCode::Char('p'|'P')))
            || matches!(e,Event::Mouse(m) if m.kind==MouseEventKind::Down(MouseButton::Left) && open.contains(Position::new(m.column,m.row)))
        {
            return Ok(true);
        }
        if matches!(e,Event::Key(k) if k.kind!=KeyEventKind::Release && matches!(k.code,KeyCode::Char('x'|'X')))
            || matches!(e,Event::Mouse(m) if m.kind==MouseEventKind::Down(MouseButton::Left) && external.contains(Position::new(m.column,m.row)))
        {
            let mut child = std::process::Command::new("xdg-open")
                .arg(path.canonicalize()?)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(false);
        }
        if dismiss(e, back) {
            return Ok(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, MouseEvent};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn shows_format_size_and_only_the_visible_back_button_closes_on_click() {
        let info = FileInfo {
            name: "Скан.djv".into(),
            format: "DjVu".into(),
            size: Some(100),
        };
        let mut term = Terminal::new(TestBackend::new(70, 12)).unwrap();
        let mut back = Rect::default();
        term.draw(|f| back = info.draw(f)).unwrap();
        let screen: String = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            screen.contains("Скан.djv")
                && screen.contains("Format: DjVu")
                && screen.contains("Size: 100 bytes")
        );
        let click = |row| {
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: back.x,
                row,
                modifiers: KeyModifiers::NONE,
            })
        };
        assert!(!dismiss(click(back.y - 1), back));
        assert!(dismiss(click(back.y), back));
        assert!(dismiss(Event::Key(KeyEvent::from(KeyCode::Esc)), back));
    }
}
