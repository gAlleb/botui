//! Match the reading surface. Unpainted paper can then show the compositor through Kitty.
use image::{DynamicImage, Rgba};
use ratatui::{Frame, layout::Rect, style::Color};
use ratatui_image::picker::ProtocolType;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Paper {
    bg: [u8; 3],
    ink: [u8; 3],
    alpha: bool,
    pub cell_background: bool,
}
impl Paper {
    pub fn for_reader(protocol: ProtocolType) -> Self {
        let (bg, ink) = crate::settings::scan_colors();
        let clear = !crate::settings::opaque();
        Self {
            bg,
            ink,
            alpha: clear && matches!(protocol, ProtocolType::Kitty | ProtocolType::Iterm2),
            cell_background: clear && protocol == ProtocolType::Halfblocks,
        }
    }
    pub fn apply(self, image: DynamicImage) -> DynamicImage {
        // A neutral source pixel is ink coverage on white paper. Saturated colors
        // stay opaque; between those cases blend in premultiplied space.
        let neutral: [Rgba<u8>; 256] = std::array::from_fn(|i| {
            let c = if self.alpha {
                self.ink
            } else {
                std::array::from_fn(|k| {
                    ((u32::from(self.ink[k]) * (255 - i as u32)
                        + u32::from(self.bg[k]) * i as u32
                        + 127)
                        / 255) as u8
                })
            };
            Rgba([
                c[0],
                c[1],
                c[2],
                if self.alpha { 255 - i as u8 } else { 255 },
            ])
        });
        let mut rgba = image.into_rgba8();
        for pixel in rgba.pixels_mut() {
            let [r, g, b, _] = pixel.0;
            let saturation = r.max(g).max(b) - r.min(g).min(b);
            if saturation >= 48 {
                continue;
            }
            let grey =
                (54 * usize::from(r) + 183 * usize::from(g) + 19 * usize::from(b) + 128) / 256;
            let base = neutral[grey];
            let keep = u32::from(saturation.saturating_sub(12));
            if keep == 0 {
                *pixel = base;
                continue;
            }
            let ink_weight = u32::from(base[3]) * (36 - keep);
            let original_weight = 255 * keep;
            let total = ink_weight + original_weight;
            for k in 0..3 {
                pixel[k] = ((u32::from(base[k]) * ink_weight
                    + u32::from(pixel[k]) * original_weight
                    + total / 2)
                    / total) as u8;
            }
            pixel[3] = ((total + 18) / 36) as u8;
        }
        DynamicImage::ImageRgba8(rgba)
    }
    pub fn cell_padding(self, protocol: ProtocolType) -> Option<Rgba<u8>> {
        let [r, g, b] = self.bg;
        (protocol == ProtocolType::Halfblocks).then_some(Rgba([r, g, b, 255]))
    }
    // A half-block has no alpha channel. Use the terminal's default background
    // for bare paper; flip the glyph when only its top half is bare.
    pub fn clear_cells(self, f: &mut Frame, area: Rect) {
        let [r, g, b] = self.bg;
        let paper = Color::Rgb(r, g, b);
        for pos in area.positions() {
            let c = &mut f.buffer_mut()[pos];
            if c.bg == paper {
                c.bg = Color::Reset;
                if c.fg == paper {
                    c.set_char(' ');
                }
            } else if c.fg == paper && matches!(c.symbol(), "▀" | "▄") {
                let other = if c.symbol() == "▀" { '▄' } else { '▀' };
                c.fg = c.bg;
                c.bg = Color::Reset;
                c.set_char(other);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compositing_clear_paper_gives_the_same_ink_as_opaque_paper() {
        let surface = Paper {
            bg: [25, 23, 36],
            ink: [224, 222, 244],
            alpha: false,
            cell_background: false,
        };
        let input = image::RgbImage::from_raw(
            4,
            1,
            vec![255, 255, 255, 64, 64, 64, 110, 125, 145, 30, 135, 244],
        )
        .unwrap();
        let render = |p: Paper| p.apply(input.clone().into()).into_rgba8();
        let opaque = render(surface);
        let clear = render(Paper {
            alpha: true,
            ..surface
        });
        assert_eq!(clear[(0, 0)][3], 0);
        assert_eq!(opaque[(0, 0)], Rgba([25, 23, 36, 255]));
        assert_eq!(clear[(3, 0)], Rgba([30, 135, 244, 255]));
        for (a, b) in clear.pixels().zip(opaque.pixels()) {
            for k in 0..3 {
                let v = (u32::from(a[k]) * u32::from(a[3])
                    + u32::from(surface.bg[k]) * u32::from(255 - a[3])
                    + 127)
                    / 255;
                assert!(b[k].abs_diff(v as u8) <= 1);
            }
        }
    }
}
