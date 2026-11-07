use ratatui::style::{Color, Style};

// Foreground, background and link colors from BookOrbit's EPUB reader themes.ts.
#[derive(Clone, Copy)]
struct Colors(u32, u32, u32);

struct Palette {
    id: &'static str,
    name: &'static str,
    light: Colors,
    dark: Colors,
}

const fn palette(id: &'static str, name: &'static str, light: Colors, dark: Colors) -> Palette {
    Palette {
        id,
        name,
        light,
        dark,
    }
}

#[rustfmt::skip]
const PALETTES: &[Palette] = &[
    palette("default", "Default", Colors(0x000000, 0xffffff, 0x0066cc), Colors(0xe0e0e0, 0x222222, 0x77bbee)),
    palette("gray", "Gray", Colors(0x222222, 0xe0e0e0, 0x4488cc), Colors(0xc6c6c6, 0x444444, 0x88ccee)),
    palette("sepia", "Sepia", Colors(0x5b4636, 0xf1e8d0, 0x008b8b), Colors(0xffd595, 0x342e25, 0x48d1cc)),
    palette("crimson", "Crimson", Colors(0x2f1f25, 0xfdf1f4, 0xdd0031), Colors(0xf3dbe2, 0x3a252d, 0xff5a86)),
    palette("meadow", "Meadow", Colors(0x232c16, 0xd7dbbd, 0x177b4d), Colors(0xd8deba, 0x333627, 0xa6d608)),
    palette("rosewood", "Rosewood", Colors(0x4e1609, 0xf0d1d5, 0xde3838), Colors(0xe5c4c8, 0x462f32, 0xff646e)),
    palette("azure", "Azure", Colors(0x262d48, 0xcedef5, 0x2d53e5), Colors(0xbabee1, 0x282e47, 0xff646e)),
    palette("dawnlight", "Dawnlight", Colors(0x586e75, 0xfdf6e3, 0x268bd2), Colors(0x93a1a1, 0x002b36, 0x268bd2)),
    palette("ember", "Ember", Colors(0x3c3836, 0xfbf1c7, 0x076678), Colors(0xebdbb2, 0x282828, 0x83a598)),
    palette("aurora", "Aurora", Colors(0x2e3440, 0xeceff4, 0x5e81ac), Colors(0xd8dee9, 0x2e3440, 0x88c0d0)),
    palette("ocean", "Ocean", Colors(0x0a4d4d, 0xe0f7fa, 0x00838f), Colors(0xb2dfdb, 0x263238, 0x4dd0e1)),
    palette("mist", "Mist", Colors(0x4a148c, 0xf3e5f5, 0x7b1fa2), Colors(0xc7b6dd, 0x3a3150, 0xb39ddb)),
    palette("amoled", "AMOLED", Colors(0x000000, 0xffffff, 0x0066cc), Colors(0xffffff, 0x000000, 0x77bbee)),
];

fn color(value: u32) -> Color {
    let [_, r, g, b] = value.to_be_bytes();
    Color::Rgb(r, g, b)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Theme {
    row: usize,
    dark: bool,
}

impl Theme {
    pub(crate) const TERMINAL: Self = Self {
        row: 0,
        dark: false,
    };
    pub(crate) const COUNT: usize = PALETTES.len() + 1;

    pub(crate) fn at(row: usize, dark: bool) -> Self {
        if row == 0 || row >= Self::COUNT {
            Self::TERMINAL
        } else {
            Self { row, dark }
        }
    }
    pub(crate) fn row(self) -> usize {
        self.row
    }
    pub(crate) fn dark(self) -> bool {
        self.dark
    }
    fn palette(self) -> Option<&'static Palette> {
        self.row.checked_sub(1).and_then(|i| PALETTES.get(i))
    }
    pub(crate) fn from_id(id: &str) -> Self {
        let Some((name, mode)) = id.rsplit_once('-') else {
            return Self::TERMINAL;
        };
        let dark = match mode {
            "light" => false,
            "dark" => true,
            _ => return Self::TERMINAL,
        };
        PALETTES
            .iter()
            .position(|p| p.id == name)
            .map(|i| Self::at(i + 1, dark))
            .unwrap_or(Self::TERMINAL)
    }
    pub(crate) fn id(self) -> String {
        self.palette().map_or_else(
            || "terminal".into(),
            |p| format!("{}-{}", p.id, if self.dark { "dark" } else { "light" }),
        )
    }
    pub(crate) fn palette_name(self) -> &'static str {
        self.palette().map_or("Terminal", |p| p.name)
    }
    pub(crate) fn name(self) -> String {
        if self == Self::TERMINAL {
            return self.palette_name().into();
        }
        format!(
            "{} · {}",
            self.palette_name(),
            if self.dark { "dark" } else { "light" }
        )
    }
    fn colors(self) -> Option<Colors> {
        self.palette()
            .map(|p| if self.dark { p.dark } else { p.light })
    }
    pub(crate) fn style(self) -> Style {
        let (fg, bg) = self
            .colors()
            .map_or((Color::Reset, Color::Reset), |Colors(fg, bg, _)| {
                (color(fg), color(bg))
            });
        Style::new().fg(fg).bg(bg)
    }
    pub(crate) fn link(self) -> Color {
        self.colors()
            .map_or(Color::Blue, |Colors(_, _, v)| color(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_selectable_theme_has_a_round_trip_id_including_existing_sepia_files() {
        let mut choices = HashSet::new();
        for row in 0..Theme::COUNT {
            for dark in [false, true] {
                let theme = Theme::at(row, dark);
                assert_eq!(Theme::from_id(&theme.id()), theme);
                choices.insert(theme.id());
            }
        }
        assert_eq!(choices.len(), 27);
        assert_eq!(
            Theme::from_id("sepia-dark").style().bg,
            Some(Color::Rgb(52, 46, 37))
        );
        assert_eq!(
            Theme::from_id("sepia-light").style().fg,
            Some(Color::Rgb(91, 70, 54))
        );
        assert_eq!(
            Theme::from_id("terminal").style(),
            Style::new().fg(Color::Reset).bg(Color::Reset)
        );
    }
}
