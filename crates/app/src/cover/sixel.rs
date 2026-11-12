//! Prepare a page palette once, then encode visible windows without requantizing.
use super::*;
use lru::LruCache;
use quantette::{ImageRef, PaletteSize, Pipeline, deps::palette::Srgb, dither::FloydSteinberg};
use std::num::NonZeroUsize;
use std::{
    fmt::Write,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    image: u64,
    width: u32,
    height: u32,
    window: [u32; 4],
}
#[derive(Clone, Copy, Default)]
pub(crate) struct View {
    pub canvas: Option<(u32, u32, u32, u32)>,
    pub paper: Option<[u8; 3]>,
    pub default_background: bool,
}
pub(crate) struct Cache(LruCache<Key, Arc<Raster>>);
impl Cache {
    pub fn new() -> Self {
        Self(LruCache::new(NonZeroUsize::new(8).unwrap()))
    }
    fn put(&mut self, key: Key, raster: Arc<Raster>) {
        // Bound the sum as well as the number of retained zoom levels/pages.
        while self
            .0
            .iter()
            .map(|(_, r)| r.indices.len() * 2)
            .sum::<usize>()
            + raster.indices.len() * 2
            > 64 * 1024 * 1024
        {
            if self.0.pop_lru().is_none() {
                break;
            }
        }
        self.0.put(key, raster);
    }
}
struct Raster {
    width: usize,
    height: usize,
    palette: Vec<[u8; 3]>, // Sixel percentages, not RGB bytes.
    indices: Vec<u16>,
    background: [u8; 3],
}
impl Raster {
    fn new(image: DynamicImage, paper: Option<[u8; 3]>) -> Result<Self, String> {
        let rgb = image.into_rgb8();
        let (width, height) = (rgb.width() as usize, rgb.height() as usize);
        let pixels: Vec<_> = rgb.pixels().map(|p| Srgb::new(p[0], p[1], p[2])).collect();
        let input =
            ImageRef::new(width as u32, height as u32, &pixels).map_err(|e| e.to_string())?;
        // Same Wu palette and Floyd-Steinberg diffusion as ratatui-image/icy_sixel.
        let indexed = Pipeline::new()
            .palette_size(PaletteSize::MAX)
            .ditherer(FloydSteinberg::default())
            .input_image(input)
            .output_srgb8_indexed_image();
        let mut palette = Vec::new();
        let remap: Vec<u16> = indexed
            .palette()
            .iter()
            .map(|c| {
                let percent = [c.red, c.green, c.blue].map(|v| (u16::from(v) * 100 / 255) as u8);
                // Several palette entries often produce exactly the same Sixel color.
                // Coalesce them without changing a single displayed pixel.
                let n = palette
                    .iter()
                    .position(|p| *p == percent)
                    .unwrap_or_else(|| {
                        palette.push(percent);
                        palette.len() - 1
                    });
                n as u16
            })
            .collect();
        let mut indices: Vec<_> = indexed
            .indices()
            .iter()
            .zip(rgb.pixels())
            .map(|(&i, p)| {
                if paper == Some(p.0) {
                    256
                } else {
                    remap[usize::from(i)]
                }
            })
            .collect();
        let background = if let Some(paper) = paper {
            paper
        } else {
            let mut counts = [0usize; 256];
            for &i in &indices {
                counts[usize::from(i)] += 1;
            }
            let base = (0..palette.len()).max_by_key(|&i| counts[i]).unwrap();
            for i in &mut indices {
                if usize::from(*i) == base {
                    *i = 256;
                }
            }
            // Foot expands Sixel percentages with integer division.
            palette[base].map(|v| (u16::from(v) * 255 / 100) as u8)
        };
        Ok(Self {
            width,
            height,
            palette,
            indices,
            background,
        })
    }
    fn encode(
        &self,
        offset: (u32, u32),
        pixels: (u32, u32),
        size: Size,
        tmux: bool,
        default_background: bool,
    ) -> Protocol {
        let (width, height) = (pixels.0 as usize, pixels.1 as usize);
        let (x0, y0) = (offset.0 as usize, offset.1 as usize);
        let (start, escape, end) =
            ratatui_image::picker::cap_parser::Parser::tmux_start_escape_end(tmux);
        let mut data = start.to_owned();
        let [r, g, b] = self.background;
        if default_background {
            write!(data, "{escape}[49m").unwrap();
        } else {
            write!(data, "{escape}[48;2;{r};{g};{b}m").unwrap();
        }
        for row in 0..size.height {
            write!(data, "{escape}[{}X", size.width).unwrap();
            if row + 1 < size.height {
                write!(data, "{escape}[1B").unwrap();
            }
        }
        if size.height > 1 {
            write!(data, "{escape}[{}A", size.height - 1).unwrap();
        }
        write!(data, "{escape}P0;1;0q\"1;1;{width};{height}").unwrap();
        for (i, &[r, g, b]) in self.palette.iter().enumerate() {
            write!(data, "#{i};2;{r};{g};{b}").unwrap();
        }
        let mut bands = vec![0u8; self.palette.len() * width];
        let mut used = vec![false; self.palette.len()];
        for y in (0..height).step_by(6) {
            if y > 0 {
                data.push('-');
            }
            for (i, used) in used.iter_mut().enumerate() {
                if *used {
                    bands[i * width..(i + 1) * width].fill(0);
                    *used = false;
                }
            }
            for row in 0..6.min(height - y) {
                let yy = (y0 + y + row).min(self.height - 1);
                for x in 0..width {
                    let xx = (x0 + x).min(self.width - 1);
                    let color = usize::from(self.indices[yy * self.width + xx]);
                    if color < 256 {
                        bands[color * width + x] |= 1 << row;
                        used[color] = true;
                    }
                }
            }
            let mut first = true;
            for (color, &used) in used.iter().enumerate() {
                if !used {
                    continue;
                }
                if !first {
                    data.push('$');
                }
                first = false;
                write!(data, "#{color}").unwrap();
                let row = &bands[color * width..(color + 1) * width];
                let right = row.iter().rposition(|&v| v != 0).map_or(0, |x| x + 1);
                let mut x = 0;
                while x < right {
                    let value = row[x];
                    let mut next = x + 1;
                    while next < right && row[next] == value {
                        next += 1;
                    }
                    let count = next - x;
                    if count > 3 {
                        write!(data, "!{count}{}", char::from(63 + value)).unwrap();
                    } else {
                        for _ in 0..count {
                            data.push(char::from(63 + value));
                        }
                    }
                    x = next;
                }
            }
        }
        // Empty final bands must still end at the declared pixel height without
        // leaving Foot at column zero (which rounds the image up to six pixels).
        if data.ends_with('-') {
            data.push('?');
        }
        write!(data, "{escape}\\{escape}[0m{end}").unwrap();
        Protocol::Sixel(ratatui_image::protocol::sixel::Sixel {
            data,
            size,
            is_tmux: tmux,
        })
    }
}

impl Picture {
    pub(crate) fn cached_sixel_encoder(
        &self,
        size: Size,
        picker: Picker,
        window: [f32; 4],
        view: View,
        cache: Arc<Mutex<Cache>>,
    ) -> impl FnOnce() -> Result<Protocol, String> + Send + 'static {
        let font = picker.font_size();
        let pixels = (
            u32::from(size.width) * u32::from(font.width),
            u32::from(size.height) * u32::from(font.height),
        );
        let (w, h, x, y) = view.canvas.unwrap_or_else(|| {
            let w = (pixels.0 as f64 / f64::from(window[2] - window[0]))
                .round()
                .max(1.) as u32;
            let h = (pixels.1 as f64 / f64::from(window[3] - window[1]))
                .round()
                .max(1.) as u32;
            (
                w,
                h,
                (f64::from(window[0]) * f64::from(w)).round() as u32,
                (f64::from(window[1]) * f64::from(h)).round() as u32,
            )
        });
        // Very tall pages/high zooms use a bounded viewport raster instead.
        let full = u64::from(w) * u64::from(h) <= 16 * 1024 * 1024;
        let bounds = if full { [0., 0., 1., 1.] } else { window };
        let key = Key {
            image: self.identity,
            width: if full { w } else { pixels.0 },
            height: if full { h } else { pixels.1 },
            window: bounds.map(f32::to_bits),
        };
        let prepared = cache.lock().unwrap().0.get(&key).cloned();
        let source = prepared.is_none().then(|| self.source.clone());
        move || {
            let raster = if let Some(raster) = prepared {
                raster
            } else {
                let source = source.unwrap();
                let (sw, sh) = (source.width(), source.height());
                let left = (bounds[0] * sw as f32).floor() as u32;
                let top = (bounds[1] * sh as f32).floor() as u32;
                let right = (bounds[2] * sw as f32).ceil().min(sw as f32) as u32;
                let bottom = (bounds[3] * sh as f32).ceil().min(sh as f32) as u32;
                let image = source
                    .crop_imm(left, top, (right - left).max(1), (bottom - top).max(1))
                    .resize_exact(key.width, key.height, image::imageops::FilterType::Triangle);
                let raster = Arc::new(Raster::new(image, view.paper)?);
                cache.lock().unwrap().put(key, raster.clone());
                raster
            };
            Ok(raster.encode(
                if full { (x, y) } else { (0, 0) },
                pixels,
                size,
                picker.tmux_detected(),
                view.default_background,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> DynamicImage {
        image::RgbImage::from_fn(71, 59, |x, y| {
            image::Rgb([(x * 3 + y) as u8, (y * 4 + x) as u8, (x * 2 + y * 2) as u8])
        })
        .into()
    }
    fn decode(protocol: &Protocol) -> icy_sixel::SixelImage {
        let Protocol::Sixel(s) = protocol else {
            panic!("Sixel expected")
        };
        icy_sixel::SixelImage::decode(s.data.as_bytes()).unwrap()
    }
    #[test]
    fn indexed_windows_keep_colors_and_crop_at_every_sixel_band_offset() {
        let source = image();
        let rgba = source.to_rgba8();
        let original = icy_sixel::SixelImage::decode(
            icy_sixel::sixel_encode(rgba.as_raw(), 71, 59, &Default::default())
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        let raster = Raster::new(source, None).unwrap();
        for top in 0..12 {
            let protocol = raster.encode((3, top), (52, 37), Size::new(13, 2), false, false);
            let cropped = decode(&protocol);
            // The decoder retains padding in the final six-pixel band.
            assert_eq!(cropped.width, 52);
            assert!(cropped.height >= 37);
            for y in 0..37 {
                for x in 0..52 {
                    let a = (y * 52 + x) * 4;
                    let b = ((y + top as usize) * 71 + x + 3) * 4;
                    for k in 0..3 {
                        let actual = if cropped.pixels[a + 3] == 0 {
                            raster.background[k]
                        } else {
                            cropped.pixels[a + k]
                        };
                        assert!(
                            actual.abs_diff(original.pixels[b + k]) <= 1,
                            "Window changed colors at {x},{y}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn scrolling_reuses_the_same_prepared_raster_and_default_paper_stays_clear() {
        let cache = Arc::new(Mutex::new(Cache::new()));
        let picture = Picture::new(image());
        let picker = Picker::halfblocks();
        let size = Size::new(4, 2);
        let window = [0., 0., 1., 1.];
        for top in [0, 20, 0] {
            let view = View {
                canvas: Some((40, 80, 0, top)),
                paper: Some([39, 50, 56]),
                default_background: true,
            };
            let protocol =
                picture.cached_sixel_encoder(size, picker.clone(), window, view, cache.clone())()
                    .unwrap();
            let Protocol::Sixel(s) = protocol else {
                unreachable!()
            };
            assert!(s.data.starts_with("\x1b[49m"));
            assert_eq!(
                cache.lock().unwrap().0.len(),
                1,
                "Scrolling prepared another page palette"
            );
        }
    }
    #[test]
    fn full_height_sixel_stays_inside_the_viewport_with_or_without_tmux() {
        for tmux in [false, true] {
            for rows in [1, 2, 7] {
                let raster = Raster::new(
                    image::RgbImage::from_fn(40, rows * 19, |x, y| {
                        image::Rgb(if x < 20 && y % 3 == 0 {
                            [10, 20, 30]
                        } else {
                            [200, 210, 220]
                        })
                    })
                    .into(),
                    None,
                )
                .unwrap();
                let Protocol::Sixel(s) = raster.encode(
                    (0, 0),
                    (40, rows * 19),
                    Size::new(4, rows as u16),
                    tmux,
                    false,
                ) else {
                    unreachable!()
                };
                let (start, esc, end) =
                    ratatui_image::picker::cap_parser::Parser::tmux_start_escape_end(tmux);
                assert!(s.data.starts_with(start) && s.data.ends_with(end));
                assert_eq!(s.data.matches(&format!("{esc}[4X")).count(), rows as usize);
                assert_eq!(
                    s.data.matches(&format!("{esc}[1B")).count(),
                    rows as usize - 1
                );
                let dcs_end = s.data.rfind(&format!("{esc}\\")).unwrap();
                assert!(!s.data[..dcs_end].ends_with(['$', '-']));
            }
        }
    }
    #[test]
    fn themed_bare_paper_uses_exact_background_instead_of_palette_approximation() {
        let paper = [39, 50, 56];
        let source = image::RgbImage::from_fn(71, 59, |x, y| {
            image::Rgb(if x > 20 && x < 40 && y > 10 && y < 50 {
                [165, 220, 222]
            } else {
                paper
            })
        });
        let raster = Raster::new(source.into(), Some(paper)).unwrap();
        let protocol = raster.encode((0, 0), (71, 59), Size::new(8, 3), false, false);
        let pixels = decode(&protocol);
        assert_eq!(raster.background, paper);
        assert_eq!(pixels.pixels[3], 0);
        assert!(pixels.pixels[(25 * 71 + 25) * 4 + 3] > 0);
        let Protocol::Sixel(s) = protocol else {
            unreachable!()
        };
        assert!(s.data.contains("\x1b[48;2;39;50;56m"));
        assert_eq!(s.data.matches("\x1b[1B").count(), 2);
        assert!(!s.data.contains("$-\x1b\\"));
    }
}
