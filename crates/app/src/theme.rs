use ratatui::style::{Color, Style};

// Foreground, background and link colors from BookOrbit's EPUB reader themes.ts.
#[derive(Clone, Copy)]
struct Colors(u32, u32, u32);

// Heading and formula ink on light paper, followed by the pair on dark paper.
#[derive(Clone, Copy)]
struct Accents(u32, u32, u32, u32);

struct Palette {
    id: &'static str,
    name: &'static str,
    light: Colors,
    dark: Colors,
    accents: Accents,
}

const fn palette(
    id: &'static str,
    name: &'static str,
    light: Colors,
    dark: Colors,
    accents: Accents,
) -> Palette {
    Palette {
        id,
        name,
        light,
        dark,
        accents,
    }
}

#[rustfmt::skip]
const PALETTES: &[Palette] = &[
    palette("default", "Default", Colors(0x000000, 0xffffff, 0x0066cc), Colors(0xe0e0e0, 0x222222, 0x77bbee), Accents(0x204884, 0x00674f, 0xf1ce91, 0x95d9ca)),
    palette("gray", "Gray", Colors(0x222222, 0xe0e0e0, 0x4488cc), Colors(0xc6c6c6, 0x444444, 0x88ccee), Accents(0x393939, 0x575757, 0xf0f0f0, 0xb7d9da)),
    palette("sepia", "Sepia", Colors(0x5b4636, 0xf1e8d0, 0x008b8b), Colors(0xffd595, 0x342e25, 0x48d1cc), Accents(0x783f19, 0x39623e, 0xffc27c, 0xabd6a1)),
    palette("crimson", "Crimson", Colors(0x2f1f25, 0xfdf1f4, 0xdd0031), Colors(0xf3dbe2, 0x3a252d, 0xff5a86), Accents(0x832c50, 0x624174, 0xffabbe, 0xdcb5eb)),
    palette("meadow", "Meadow", Colors(0x232c16, 0xd7dbbd, 0x177b4d), Colors(0xd8deba, 0x333627, 0xa6d608), Accents(0x42531c, 0x1d604b, 0xedd592, 0xa8d7ad)),
    palette("rosewood", "Rosewood", Colors(0x4e1609, 0xf0d1d5, 0xde3838), Colors(0xe5c4c8, 0x462f32, 0xff646e), Accents(0x843d37, 0x623753, 0xffada0, 0xe5b1d8)),
    palette("azure", "Azure", Colors(0x262d48, 0xcedef5, 0x2d53e5), Colors(0xbabee1, 0x282e47, 0xff646e), Accents(0x234d83, 0x454a85, 0x9ecbff, 0xcdbafa)),
    palette("dawnlight", "Dawnlight", Colors(0x586e75, 0xfdf6e3, 0x268bd2), Colors(0x93a1a1, 0x002b36, 0x268bd2), Accents(0x684d24, 0x08695e, 0xeabc83, 0x82d0c1)),
    palette("ember", "Ember", Colors(0x3c3836, 0xfbf1c7, 0x076678), Colors(0xebdbb2, 0x282828, 0x83a598), Accents(0x774217, 0x356451, 0xf5b879, 0xb7d29b)),
    palette("aurora", "Aurora", Colors(0x2e3440, 0xeceff4, 0x5e81ac), Colors(0xd8dee9, 0x2e3440, 0x88c0d0), Accents(0x354d73, 0x32606c, 0xaacaf3, 0xa3d6bf)),
    palette("ocean", "Ocean", Colors(0x0a4d4d, 0xe0f7fa, 0x00838f), Colors(0xb2dfdb, 0x263238, 0x4dd0e1), Accents(0x20596a, 0x38614c, 0x8cdaef, 0xb6dcc0)),
    palette("mist", "Mist", Colors(0x4a148c, 0xf3e5f5, 0x7b1fa2), Colors(0xc7b6dd, 0x3a3150, 0xb39ddb), Accents(0x633376, 0x493c76, 0xe8b6ee, 0xbdc4f3)),
    palette("parchment", "Parchment", Colors(0x493e2f, 0xf4ecd8, 0x80521c), Colors(0xe7dac1, 0x29241d, 0xd9b878), Accents(0x774c1e, 0x4e603b, 0xf0bc79, 0xb5d09c)),
    palette("sage", "Sage", Colors(0x293e33, 0xe5ede2, 0x2a6854), Colors(0xd6e2d2, 0x1e2b24, 0x8fc7a5), Accents(0x385537, 0x215759, 0xb9d99a, 0x94d5ca)),
    palette("dusk", "Dusk", Colors(0x453449, 0xf0e7ef, 0x805184), Colors(0xe3d3e4, 0x2a2130, 0xd4a6dc), Accents(0x703d65, 0x425276, 0xedb2d8, 0xb6c7ee)),
    palette("slate", "Slate", Colors(0x2e3945, 0xe7edf2, 0x315f8c), Colors(0xd3dce6, 0x1c2530, 0x91b7dd), Accents(0x2c5077, 0x365967, 0xa6ccfa, 0xa1d5cc)),
    palette("amoled", "AMOLED", Colors(0x000000, 0xffffff, 0x0066cc), Colors(0xffffff, 0x000000, 0x77bbee), Accents(0x303030, 0x5b5b5b, 0xdedede, 0xb3b3b3)),
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

    pub(crate) fn heading(self) -> Color {
        self.palette().map_or(Color::Yellow, |p| {
            color(if self.dark { p.accents.2 } else { p.accents.0 })
        })
    }
    pub(crate) fn formula(self) -> Color {
        self.palette().map_or(Color::Cyan, |p| {
            color(if self.dark { p.accents.3 } else { p.accents.1 })
        })
    }

    pub(crate) fn frame_style(self, focused: bool) -> Style {
        let Some(Colors(ink, paper, _)) = self.colors() else {
            let native = Style::new().fg(Color::Reset);
            return if focused { native.bold() } else { native.dim() };
        };
        let ink_percent = if focused { 85 } else { 45 };
        let blend = |ink: u8, paper: u8| {
            ((u16::from(ink) * ink_percent + u16::from(paper) * (100 - ink_percent) + 50) / 100)
                as u8
        };
        let [_, r, g, b] = ink.to_be_bytes();
        let [_, pr, pg, pb] = paper.to_be_bytes();
        Style::new().fg(Color::Rgb(blend(r, pr), blend(g, pg), blend(b, pb)))
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
        assert_eq!(choices.len(), 1 + 2 * PALETTES.len());
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

    #[test]
    fn focused_and_inactive_frames_have_distinct_contrast_on_every_book_paper() {
        fn luminance(color: Color) -> f64 {
            let Color::Rgb(r, g, b) = color else {
                panic!("expected an RGB reading theme")
            };
            let linear = |c: u8| {
                let c = f64::from(c) / 255.0;
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
        }
        let contrast = |a: f64, b: f64| (a.max(b) + 0.05) / (a.min(b) + 0.05);
        for row in 1..Theme::COUNT {
            for dark in [false, true] {
                let theme = Theme::at(row, dark);
                let paper = luminance(theme.style().bg.unwrap());
                for accent in [theme.heading(), theme.formula()] {
                    assert!(contrast(luminance(accent), paper) >= 4.5, "{}", theme.id());
                    assert_ne!(Some(accent), theme.style().fg);
                }
                assert_ne!(theme.heading(), theme.formula());
                let active = luminance(theme.frame_style(true).fg.unwrap());
                let inactive = luminance(theme.frame_style(false).fg.unwrap());
                assert!(contrast(active, paper) >= 3.0, "{}", theme.id());
                assert!(contrast(inactive, paper) >= 1.8, "{}", theme.id());
                assert!(contrast(active, inactive) >= 1.8, "{}", theme.id());
            }
        }
    }

    #[test]
    fn frames_follow_the_books_ink_and_paper_and_terminal_frames_use_native_ink() {
        for (id, active, inactive) in [
            ("amoled-dark", 0xd9d9d9, 0x737373),
            ("amoled-light", 0x262626, 0x8c8c8c),
            ("azure-dark", 0xa4a8ca, 0x6a6f8c),
            ("azure-light", 0x3f4862, 0x828ea7),
        ] {
            let theme = Theme::from_id(id);
            assert_eq!(theme.frame_style(true), Style::new().fg(color(active)));
            assert_eq!(theme.frame_style(false), Style::new().fg(color(inactive)));
        }
        let native = Style::new().fg(Color::Reset);
        assert_eq!(Theme::TERMINAL.frame_style(true), native.bold());
        assert_eq!(Theme::TERMINAL.frame_style(false), native.dim());
    }
}
