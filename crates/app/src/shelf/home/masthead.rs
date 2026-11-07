//! Stationary, cell-sized art; never take space from the source list in a short window.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    widgets::Paragraph,
};

const OWL: &[&str] = &[
    r"       ,_,   ,_,",
    r"      /   '-'   \",
    r"     / .-.   .-. \",
    r"    | ( o ) ( o ) |",
    r"    |  '-' V '-'  |",
    r"    | \   ___   / |",
    r"     \ '.     .' /",
    r"      \  '---'  /",
    r"    ___\/_____\/___",
    r"   /  ___   |  ___  \",
    r"  /  _____  | _____  \",
    r"  \_________|________/",
];
const SMALL: &[&str] = &[
    r"     /\_ /\",
    r"    ( o o  )",
    r"    /  V   \",
    r"    \     /",
    r"   ___v_v___",
    r"  / ___|___ \",
    r"  \____|____/",
];
const WORD: &[&str] = &[
    "██████╗   ██████╗  ████████╗ ██╗   ██╗ ██╗",
    "██╔══██╗ ██╔═══██╗ ╚══██╔══╝ ██║   ██║ ██║",
    "██████╔╝ ██║   ██║    ██║    ██║   ██║ ██║",
    "██╔══██╗ ██║   ██║    ██║    ██║   ██║ ██║",
    "██████╔╝ ╚██████╔╝    ██║    ╚██████╔╝ ██║",
    "╚═════╝   ╚═════╝     ╚═╝     ╚═════╝  ╚═╝",
];
pub(super) fn draw(f: &mut Frame, area: Rect, canvas: Rect) -> u16 {
    let plain = std::env::var_os("NO_COLOR").is_some();
    let (owl, book, word, star) = if plain {
        (Color::Reset, Color::Reset, Color::Reset, Color::Reset)
    } else if let Some((ink, _, accent)) = crate::settings::shell_colors() {
        (accent, accent, ink, accent)
    } else {
        (Color::Yellow, Color::Cyan, Color::Reset, Color::Cyan)
    };
    let big = canvas.width >= 70 && canvas.height >= 30;
    let small = canvas.width >= 64 && canvas.height >= 22;
    let (art, book_at) = if big { (OWL, 8) } else { (SMALL, 4) };
    let h = if small { art.len() as u16 } else { 1 };
    let left = art.iter().map(|s| s.len()).max().unwrap_or(0) as u16;
    let w = if small {
        left + 4 + WORD.iter().map(|s| s.chars().count()).max().unwrap_or(0) as u16
    } else {
        5
    };
    let mark = crate::centered(Rect::new(canvas.x, canvas.y, canvas.width, h), w, h);
    if area.width >= 68 && area.height >= 20 {
        const STARS: &[(u16, u16, &str)] = &[
            (4, 11, "·"),
            (13, 25, "✦"),
            (5, 48, "·"),
            (10, 73, "+"),
            (18, 89, "·"),
            (27, 5, "·"),
            (42, 9, "+"),
            (64, 3, "·"),
            (79, 13, "·"),
            (92, 18, "+"),
            (97, 43, "·"),
            (88, 61, "✦"),
            (96, 79, "·"),
            (77, 94, "·"),
        ];
        for &(u, v, s) in STARS {
            let x =
                area.x + ((u32::from(area.width.saturating_sub(1)) * u32::from(u)) / 100) as u16;
            let y =
                area.y + ((u32::from(area.height.saturating_sub(1)) * u32::from(v)) / 100) as u16;
            if !canvas.contains((x, y).into())
                || (y < mark.bottom()
                    && (x.saturating_add(2) < mark.x || x >= mark.right().saturating_add(2)))
            {
                f.render_widget(
                    Paragraph::new(s).style(Style::new().fg(star).dim()),
                    Rect::new(x, y, 1, 1),
                );
            }
        }
    }
    if !small {
        f.render_widget(Paragraph::new("BOTUI").fg(word).bold(), mark);
        return 1;
    }
    for (i, s) in art.iter().enumerate() {
        f.render_widget(
            Paragraph::new(*s).fg(if i < book_at { owl } else { book }),
            Rect::new(mark.x, mark.y + i as u16, left, 1),
        );
    }
    let offset = (h - WORD.len() as u16) / 2;
    for (i, s) in WORD.iter().enumerate() {
        f.render_widget(
            Paragraph::new(*s).fg(word).bold(),
            Rect::new(
                mark.x + left + 4,
                mark.y + offset + i as u16,
                mark.width - left - 4,
                1,
            ),
        );
    }
    h + 1
}
