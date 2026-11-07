//! The web catalog uses Oklch palettes. Convert to sRGB before encoding the
//! vector art so a terminal and the SVG renderer agree on the same ink.
use ratatui::style::Color;
use std::fmt::Write;

#[derive(Clone, Copy)]
pub(super) struct Ink([u8; 3]);
impl Ink {
    pub(super) fn hex(self) -> String {
        let [r, g, b] = self.0;
        format!("#{r:02x}{g:02x}{b:02x}")
    }
    pub(super) fn color(self) -> Color {
        let [r, g, b] = self.0;
        Color::Rgb(r, g, b)
    }
}
fn ink(l: f64, c: f64, h: f64) -> Ink {
    let h = h.to_radians();
    let (a, b) = (c * h.cos(), c * h.sin());
    let lms = [
        (l + 0.3963377774 * a + 0.2158037573 * b).powi(3),
        (l - 0.1055613458 * a - 0.0638541728 * b).powi(3),
        (l - 0.0894841775 * a - 1.291485548 * b).powi(3),
    ];
    let [x, y, z] = lms;
    let linear = [
        4.0767416621 * x - 3.3077115913 * y + 0.2309699292 * z,
        -1.2684380046 * x + 2.6097574011 * y - 0.3413193965 * z,
        -0.0041960863 * x - 0.7034186147 * y + 1.707614701 * z,
    ];
    Ink(linear.map(|v| {
        let v = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        };
        (255. * v.clamp(0., 1.)).round() as u8
    }))
}
struct Random(u32);
impl Random {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t));
        f64::from(t ^ (t >> 14)) / 4294967296.
    }
}
pub(super) struct Palette {
    pub bg: Ink,
    pub text: Ink,
    colors: [Ink; 3],
}
pub(super) fn palette(group: u32) -> Palette {
    const HUES: [[u16; 3]; 12] = [
        [27, 255, 90],
        [255, 88, 25],
        [90, 250, 20],
        [45, 200, 262],
        [195, 30, 85],
        [150, 350, 262],
        [295, 85, 190],
        [350, 150, 250],
        [265, 50, 160],
        [115, 15, 240],
        [15, 230, 75],
        [175, 330, 90],
    ];
    let [a, b, c] = HUES[group as usize % 12].map(f64::from);
    let warm = a > 70. && a < 115.;
    let mut colors = [
        ink(
            if warm { 0.8 } else { 0.62 },
            if warm { 0.16 } else { 0.19 },
            a,
        ),
        ink(0.8, 0.14, b),
        ink(0.46, 0.14, c),
    ];
    let dark = ink(0.19, 0.014, a);
    let paper = ink(0.955, 0.014, 85.);
    let roll = Random(group).next();
    let (bg, text) = if roll < 0.4 {
        (paper, dark)
    } else if roll < 0.75 {
        (dark, paper)
    } else {
        let pair = (colors[0], if warm { dark } else { paper });
        colors[0] = pair.1;
        colors[2] = dark;
        pair
    };
    Palette { bg, text, colors }
}
pub(super) fn motif(item: u32, palette: &Palette, index: Option<&str>) -> String {
    let bg = palette.bg.hex();
    let fg = palette.text.hex();
    let [a, b, c] = palette.colors.map(Ink::hex);
    let mut rng = Random(item);
    let mut art =
        String::from(r#"<svg x="0" y="185" width="240" height="175" viewBox="0 0 240 175">"#);
    if let Some(n) = index.filter(|s| !s.is_empty()) {
        // Use vector digits so a cover's volume is recognizable without a system font.
        let n = n
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .take(5)
            .collect::<Vec<_>>();
        let _ = write!(art, r#"<circle cx="52" cy="76" r="57" fill="{b}"/>"#);
        let unit = (208. / (n.len().max(1) as f64 * 6.)).min(11.);
        for (i, d) in n.iter().enumerate() {
            let x = 225. - ((n.len() - i) * 6) as f64 * unit;
            if let Some(d) = d.to_digit(10) {
                let mask = [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f][d as usize];
                for (j, (dx, dy, w, h)) in [
                    (0, 0, 5, 1),
                    (4, 0, 1, 5),
                    (4, 4, 1, 5),
                    (0, 8, 5, 1),
                    (0, 4, 1, 5),
                    (0, 0, 1, 5),
                    (0, 4, 5, 1),
                ]
                .into_iter()
                .enumerate()
                {
                    if mask & (1 << j) != 0 {
                        let _ = write!(
                            art,
                            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{a}"/>"#,
                            x + dx as f64 * unit,
                            170. - (9 - dy) as f64 * unit,
                            w as f64 * unit,
                            h as f64 * unit
                        );
                    }
                }
            } else {
                let _ = write!(art, r#"<circle cx="{x}" cy="162" r="5" fill="{a}"/>"#);
            }
        }
    } else {
        match item % 7 {
            0 => {
                let x = 72. + rng.next() * 96.;
                let moon = if x > 120. { 38 } else { 202 };
                let _ = write!(
                    art,
                    r#"<circle cx="{x}" cy="175" r="105" fill="{a}"/><circle cx="{moon}" cy="42" r="19" fill="{b}"/>"#
                );
                for i in 0..4 {
                    let _ = write!(
                        art,
                        r#"<rect y="{}" width="240" height="{}" fill="{bg}"/>"#,
                        162 - i * 17,
                        2 + 2 * i
                    );
                }
            }
            1 => {
                for y in [145, 85, 25] {
                    for x in [0, 60, 120, 180] {
                        let r = rng.next();
                        let col = [&a, &b, &c, &fg][(rng.next() * 4.) as usize];
                        if r < 0.15 {
                            continue;
                        }
                        if r < 0.28 {
                            let _ = write!(
                                art,
                                r#"<rect x="{x}" y="{y}" width="60" height="60" fill="{col}"/>"#
                            );
                        } else {
                            let rot = (rng.next() * 4.) as i32 * 90;
                            let _ = write!(
                                art,
                                r#"<path transform="translate({x} {y}) rotate({rot} 30 30)" d="M0 0h60a60 60 0 0 1-60 60Z" fill="{col}"/>"#
                            );
                        }
                    }
                }
            }
            2 => {
                let angle = 35. + 20. * rng.next();
                let _ = write!(art, r#"<g transform="rotate({angle:.2} 120 88)">"#);
                for i in 0..24 {
                    let col = [&a, &b, &c][i % 3];
                    let _ = write!(
                        art,
                        r#"<rect x="{}" y="-180" width="15" height="535" fill="{col}"/>"#,
                        i as i64 * 27 - 180
                    );
                }
                let _ = write!(
                    art,
                    r#"</g><circle cx="{}" cy="86" r="32" fill="{bg}" stroke="{fg}" stroke-width="5"/>"#,
                    60. + rng.next() * 120.
                );
            }
            3 => {
                let x = [120, 72, 168][(rng.next() * 3.) as usize];
                let outer: f64 = 148.8;
                for (i, col) in [&a, &bg, &b, &bg, &c, &bg, &fg].into_iter().enumerate() {
                    let rad = outer * (1. - i as f64 / 6.2);
                    let _ = write!(
                        art,
                        r#"<circle cx="{x}" cy="175" r="{rad:.2}" fill="{col}"/>"#
                    );
                }
            }
            4 => {
                let x = 82. + 48. * rng.next();
                let y = 67. + 42. * rng.next();
                let _ = write!(
                    art,
                    r#"<rect width="{x}" height="{y}" fill="{a}"/><rect x="{x}" y="{y}" width="180" height="120" fill="{b}"/><rect y="{y}" width="{}" height="120" fill="{c}"/><path d="M{x} 0 V175 M0 {y} H240" stroke="{fg}" stroke-width="5"/>"#,
                    x / 2.
                );
            }
            5 => {
                let big = (rng.next() * 15.) as i32;
                for i in 0..15 {
                    let x = 24 + i % 5 * 48;
                    let y = 151 - i / 5 * 48;
                    let (radius, col) = if i == big {
                        (43, &a)
                    } else {
                        (
                            if rng.next() < 0.2 { 15 } else { 8 },
                            if rng.next() < 0.2 { &b } else { &fg },
                        )
                    };
                    let _ = write!(
                        art,
                        r#"<circle cx="{x}" cy="{y}" r="{radius}" fill="{col}"/>"#
                    );
                }
            }
            _ => {
                for (i, col) in [&a, &b, &c, &fg].into_iter().enumerate() {
                    let sign = if i % 2 == 0 { -16 } else { 16 };
                    let y = 25 + i * 40;
                    let _ = write!(
                        art,
                        r#"<path d="M-10 {y} q18 {sign} 36 0 t36 0 t36 0 t36 0 t36 0 t36 0 t36 0" fill="none" stroke="{col}" stroke-width="10"/>"#
                    );
                }
            }
        }
    }
    art.push_str("</svg>");
    art
}
