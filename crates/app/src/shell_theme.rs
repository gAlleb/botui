//! UI ink is independent of the book's paper and of embedded artwork.
use ratatui::style::Color;
#[derive(Clone, Copy)]
pub struct Palette(pub &'static str, pub &'static str, u32, u32, u32);
pub const ALL: &[Palette] = &[
    Palette("terminal", "Terminal", 0, 0, 0),
    Palette("gruvbox-dark", "Gruvbox Dark", 0xebdbb2, 0x282828, 0xfabd2f),
    Palette(
        "gruvbox-light",
        "Gruvbox Light",
        0x3c3836,
        0xfbf1c7,
        0x8f3f71,
    ),
    Palette(
        "catppuccin-latte",
        "Catppuccin Latte",
        0x4c4f69,
        0xeff1f5,
        0x8839ef,
    ),
    Palette(
        "catppuccin-frappe",
        "Catppuccin Frappé",
        0xc6d0f5,
        0x303446,
        0xca9ee6,
    ),
    Palette(
        "catppuccin-macchiato",
        "Catppuccin Macchiato",
        0xcad3f5,
        0x24273a,
        0xc6a0f6,
    ),
    Palette(
        "catppuccin-mocha",
        "Catppuccin Mocha",
        0xcdd6f4,
        0x1e1e2e,
        0xcba6f7,
    ),
    Palette("nord", "Nord", 0xeceff4, 0x2e3440, 0x88c0d0),
    Palette("dracula", "Dracula", 0xf8f8f2, 0x282a36, 0xbd93f9),
    Palette("tokyo-night", "Tokyo Night", 0xc0caf5, 0x1a1b26, 0x7aa2f7),
    Palette(
        "solarized-dark",
        "Solarized Dark",
        0x93a1a1,
        0x002b36,
        0x2aa198,
    ),
    Palette(
        "solarized-light",
        "Solarized Light",
        0x586e75,
        0xfdf6e3,
        0x268bd2,
    ),
    Palette("one-dark", "One Dark", 0xabb2bf, 0x282c34, 0x61afef),
    Palette("kanagawa", "Kanagawa Wave", 0xdcd7ba, 0x1f1f28, 0x98bb6c),
    Palette("rose-pine", "Rosé Pine", 0xe0def4, 0x191724, 0xebbcba),
    Palette(
        "rose-pine-dawn",
        "Rosé Pine Dawn",
        0x575279,
        0xfaf4ed,
        0x907aa9,
    ),
    Palette(
        "everforest",
        "Everforest Dark",
        0xd3c6aa,
        0x2d353b,
        0xa7c080,
    ),
];
pub fn index(id: Option<&str>) -> usize {
    let id = id.unwrap_or("catppuccin-mocha");
    ALL.iter().position(|p| p.0 == id).unwrap_or(0)
}
fn rgb(n: u32) -> Color {
    let [_, r, g, b] = n.to_be_bytes();
    Color::Rgb(r, g, b)
}
impl Palette {
    pub fn colors(self) -> (Color, Color, Color) {
        (rgb(self.2), rgb(self.3), rgb(self.4))
    }
}

#[derive(Clone, Copy, Default)]
pub struct TerminalPaper {
    pub bg: Option<Color>,
    kitty: bool,
}
impl TerminalPaper {
    pub fn detect() -> Self {
        use terminal_colorsaurus::{QueryOptions, background_color};
        let options = QueryOptions::default();
        let mut result = Self {
            kitty: std::env::var_os("KITTY_WINDOW_ID").is_some()
                || std::env::var("TERM").is_ok_and(|v| v == "xterm-kitty"),
            ..Default::default()
        };
        if let Ok(answer) = background_color(options) {
            let (r, g, b) = answer.scale_to_8bit();
            result.bg = Some(Color::Rgb(r, g, b));
        }
        result
    }
    pub fn solid(self, color: Color) -> Color {
        // Kitty makes even an explicit RGB match of its default background translucent.
        if self.kitty
            && Some(color) == self.bg
            && let Color::Rgb(r, g, b) = color
        {
            Color::Rgb(r, g, b ^ 1)
        } else {
            color
        }
    }
}
