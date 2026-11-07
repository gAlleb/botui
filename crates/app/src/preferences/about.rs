//! Notices are available inside the installed binary as well as the source tree.
use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    widgets::{Paragraph, Wrap},
};
use std::{
    io,
    process::{Command, Stdio},
};
const URL: &str = "https://github.com/bookorbit/bookorbit";
pub fn draw(f: &mut Frame, area: Rect, offset: &mut usize) -> usize {
    use ratatui::text::Line;
    let text = vec![
        Line::from(concat!("BOTUI ", env!("CARGO_PKG_VERSION")))
            .cyan()
            .bold(),
        Line::from("Terminal book reader"),
        Line::default(),
        Line::from(
            "Open a book from a local folder, browse an OPDS catalog or connect your BookOrbit library.",
        ),
        Line::default(),
        Line::from(
            "Read EPUB, FB2, PDF and DjVu on a single page, in a two-page spread or with continuous scrolling.",
        ),
        Line::default(),
        Line::from(
            "Jump to a chapter, search the text or bookmark your place with the keyboard or mouse.",
        ),
    ];
    let p = Paragraph::new(text).wrap(Wrap { trim: true });
    let max = p
        .line_count(area.width)
        .saturating_sub(usize::from(area.height));
    *offset = (*offset).min(max);
    f.render_widget(p.scroll((*offset as u16, 0)), area);
    max
}

pub fn legal(f: &mut Frame, area: Rect, offset: &mut usize) -> (usize, Rect) {
    let text = concat!(
        "BOTUI ",
        env!("CARGO_PKG_VERSION"),
        " · Terminal book reader\n",
        "Original BOTUI code: GNU GPL version 3 or later.\n\n",
        "BookOrbit cover artwork · adapted 2026-11-07\n",
        "Copyright (C) 2025-2026 neon. Additional copyright is held by the respective BookOrbit contributors.\n\n",
        "Licensed under the GNU Affero General Public License, version 3 only, with additional terms. This software is provided without warranty. You may redistribute and modify it under those terms.\n\n",
        "For the adapted artwork: This is a modified version of BookOrbit and is not an official BookOrbit release.\n\n",
        "Original project: https://github.com/bookorbit/bookorbit\n",
        "Full licenses and notices: botui --license"
    );
    let book = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    let p = Paragraph::new(text).wrap(Wrap { trim: true });
    let max = p
        .line_count(book.width)
        .saturating_sub(usize::from(book.height));
    *offset = (*offset).min(max);
    f.render_widget(p.scroll((*offset as u16, 0)), book);
    let link = Rect::new(area.x, area.y, area.width.min(26), area.height.min(1));
    f.render_widget(
        Paragraph::new("↗ Powered by BookOrbit").cyan().underlined(),
        link,
    );
    (max, link)
}
pub fn open() -> io::Result<()> {
    let mut child = Command::new("xdg-open")
        .arg(URL)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
