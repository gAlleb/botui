//! Semantic Markdown colors follow the reading palette, including live previews.
use crate::theme::Theme;
use botui_core::publisher::Face;
use ratatui::style::{Color, Style};

fn dark(theme: Theme) -> bool {
    theme.dark() || theme == Theme::TERMINAL
}
fn accent(role: u8, theme: Theme) -> Option<Color> {
    let role = match role {
        12 => 1,
        13 => 2,
        14 => 4,
        15 => 3,
        16 => 6,
        other => other,
    };
    let rgb = match (role, dark(theme)) {
        (1, true) => (140, 185, 255),
        (1, false) => (25, 70, 160),
        (2, true) => (150, 210, 155),
        (2, false) => (20, 105, 45),
        (3, true) => (235, 180, 120),
        (3, false) => (145, 65, 15),
        (4, true) => (205, 170, 240),
        (4, false) => (115, 55, 155),
        (5, true) => (120, 205, 210),
        (5, false) => (15, 100, 110),
        (6, true) => (230, 165, 185),
        (6, false) => (145, 45, 85),
        (7 | 9, _) => return Some(theme.link()),
        (8, true) => (165, 180, 195),
        (8, false) => (85, 100, 115),
        _ => return None,
    };
    Some(Color::Rgb(rgb.0, rgb.1, rgb.2))
}
pub(super) fn background(role: u8, theme: Theme) -> Color {
    let fallback = if dark(theme) {
        (30, 30, 36)
    } else {
        (250, 250, 250)
    };
    let (r, g, b) = match theme.style().bg {
        Some(Color::Rgb(r, g, b)) => (r, g, b),
        _ => fallback,
    };
    let tint = accent(role, theme).unwrap_or(if dark(theme) {
        Color::Rgb(220, 225, 235)
    } else {
        Color::Rgb(40, 45, 55)
    });
    let Color::Rgb(tr, tg, tb) = tint else {
        return Color::DarkGray;
    };
    let blend = |a: u8, b: u8| ((u16::from(a) * 7 + u16::from(b)) / 8) as u8;
    Color::Rgb(blend(r, tr), blend(g, tg), blend(b, tb))
}
pub(super) fn face(face: Face, theme: Theme) -> Style {
    let color = match (face.syntax, dark(theme)) {
        (1, true) => Some(Color::Rgb(140, 185, 255)),
        (1, false) => Some(Color::Rgb(25, 70, 160)),
        (2, true) => Some(Color::Rgb(150, 210, 155)),
        (2, false) => Some(Color::Rgb(20, 105, 45)),
        (3, true) => Some(Color::Rgb(160, 165, 175)),
        (3, false) => Some(Color::Rgb(85, 90, 100)),
        (4, true) => Some(Color::Rgb(235, 180, 120)),
        (4, false) => Some(Color::Rgb(145, 65, 15)),
        _ => accent(face.role, theme),
    };
    let mut style = Style::new();
    if let Some(color) = color {
        style = style.fg(color);
    }
    if face.role == 10 {
        style = style.bg(background(10, theme));
    }
    style
}
