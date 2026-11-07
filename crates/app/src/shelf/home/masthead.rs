//! Stationary, cell-sized art; never take space from the source list in a short window.
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};

mod mark;
struct Ink {
    paper: Color,
    feathers: Color,
    amber: Color,
    aqua: Color,
    violet: Color,
    shade: Color,
    pupil: Color,
    glint: Color,
}

impl Ink {
    fn current() -> Self {
        if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            return Self {
                paper: Color::Reset,
                feathers: Color::Reset,
                amber: Color::Reset,
                aqua: Color::Reset,
                violet: Color::Reset,
                shade: Color::Reset,
                pupil: Color::Reset,
                glint: Color::Reset,
            };
        }
        let palette = crate::settings::interface_palette();
        let (feathers, amber, aqua, violet): (u32, u32, u32, u32) = match palette.0 {
            "gruvbox-dark" => (0xfe8019, 0xfabd2f, 0x8ec07c, 0xd3869b),
            "gruvbox-light" => (0xaf3a03, 0xb57614, 0x427b58, 0x8f3f71),
            "catppuccin-latte" => (0xfe640b, 0xdf8e1d, 0x179299, 0x8839ef),
            "catppuccin-frappe" => (0xef9f76, 0xe5c890, 0x81c8be, 0xca9ee6),
            "catppuccin-macchiato" => (0xf5a97f, 0xeed49f, 0x8bd5ca, 0xc6a0f6),
            "catppuccin-mocha" => (0xfab387, 0xf9e2af, 0x94e2d5, 0xcba6f7),
            "nord" => (0xd08770, 0xebcb8b, 0x8fbcbb, 0xb48ead),
            "dracula" => (0xffb86c, 0xf1fa8c, 0x8be9fd, 0xbd93f9),
            "tokyo-night" => (0xff9e64, 0xe0af68, 0x7dcfff, 0xbb9af7),
            "solarized-dark" | "solarized-light" => (0xcb4b16, 0xb58900, 0x2aa198, 0x6c71c4),
            "one-dark" => (0xd19a66, 0xe5c07b, 0x56b6c2, 0xc678dd),
            "kanagawa" => (0xffa066, 0xe6c384, 0x7fb4ca, 0x957fb8),
            "rose-pine" => (0xebbcba, 0xf6c177, 0x9ccfd8, 0xc4a7e7),
            "rose-pine-dawn" => (0xb4637a, 0xea9d34, 0x56949f, 0x907aa9),
            "everforest" => (0xe69875, 0xdbbc7f, 0x83c092, 0xd699b6),
            _ => {
                return Self {
                    paper: Color::Gray,
                    feathers: Color::Red,
                    amber: Color::Yellow,
                    aqua: Color::Cyan,
                    violet: Color::Magenta,
                    shade: Color::Blue,
                    pupil: Color::Black,
                    glint: Color::White,
                };
            }
        };
        let rgb = |v: u32| {
            let [_, r, g, b] = v.to_be_bytes();
            Color::Rgb(r, g, b)
        };
        let (fg, bg, _) = palette.colors();
        let shade = match (fg, bg) {
            (Color::Rgb(r, g, b), Color::Rgb(x, y, z)) => {
                Color::Rgb(r.midpoint(x), g.midpoint(y), b.midpoint(z))
            }
            _ => fg,
        };
        Self {
            paper: fg,
            feathers: rgb(feathers),
            amber: rgb(amber),
            aqua: rgb(aqua),
            violet: rgb(violet),
            shade,
            pupil: if palette.0.contains("light")
                || matches!(palette.0, "catppuccin-latte" | "rose-pine-dawn")
            {
                fg
            } else {
                bg
            },
            glint: rgb(0xfffaf0),
        }
    }
}

fn draw_glyphs(f: &mut Frame, x: u16, y: u16, line: &str, style: impl Fn(usize, char) -> Style) {
    for (column, glyph) in line.chars().enumerate().filter(|(_, ch)| *ch != ' ') {
        if let Some(cell) = f.buffer_mut().cell_mut(Position::new(x + column as u16, y)) {
            cell.set_char(glyph).set_style(style(column, glyph));
        }
    }
}

pub(super) fn illustrated(canvas: Rect) -> bool {
    canvas.width >= mark::MIN && canvas.height >= 22
}

pub(super) fn draw(f: &mut Frame, area: Rect, canvas: Rect) -> u16 {
    let ink = Ink::current();
    let illustrated = illustrated(canvas);
    let (w, h) = if illustrated {
        (mark::width(canvas.width), mark::H)
    } else {
        (5, 1)
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
                    Paragraph::new(s).style(Style::new().fg(ink.shade).dim()),
                    Rect::new(x, y, 1, 1),
                );
            }
        }
    }
    if !illustrated {
        let name = &"BOTUI"[..usize::from(mark.width.min(5))];
        draw_glyphs(f, mark.x, mark.y, name, |_, _| {
            Style::new().fg(ink.violet).bold()
        });
        return 1;
    }
    mark::draw(f, mark, &ink);
    let caption = Rect::new(
        mark.x + mark::TYPE_X,
        mark.bottom() - 3,
        mark.width - mark::TYPE_X,
        1,
    );
    f.render_widget(Paragraph::new("Your TUI book reader."), caption);
    f.render_widget(
        Paragraph::new("EPUB  ·  FB2  ·  PDF  ·  DjVu").style(Style::new().fg(ink.shade)),
        Rect {
            y: caption.y + 1,
            ..caption
        },
    );
    h
}
