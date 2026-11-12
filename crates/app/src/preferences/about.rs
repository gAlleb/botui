//! Notices are available inside the installed binary as well as the source tree.
use ratatui::text::Line;
use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    widgets::{Paragraph, Wrap},
};
pub fn draw(f: &mut Frame, area: Rect, offset: &mut usize) -> usize {
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
            "Read EPUB, FB2, PDF, DjVu, CBZ and CBR on a single page, in a two-page spread or with continuous scrolling.",
        ),
        Line::default(),
        Line::from(
            "Jump to a chapter, search the text or bookmark your place with the keyboard or mouse.",
        ),
    ];
    render(f, area, offset, text)
}

pub fn legal(f: &mut Frame, area: Rect, offset: &mut usize) -> usize {
    render(
        f,
        area,
        offset,
        vec![
            Line::from("BOTUI · GNU GPL version 3 or later").bold(),
            Line::from("Read the full license with botui --license."),
            Line::from("Built-in RAR decoding: rars (Apache-2.0). No external unpacker required."),
        ],
    )
}

fn render(f: &mut Frame, area: Rect, offset: &mut usize, text: Vec<Line<'_>>) -> usize {
    let p = Paragraph::new(text).wrap(Wrap { trim: true });
    let max = p
        .line_count(area.width)
        .saturating_sub(usize::from(area.height));
    *offset = (*offset).min(max);
    f.render_widget(p.scroll((*offset as u16, 0)), area);
    max
}
