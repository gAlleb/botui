//! Terminal image rendering shared by the shelf and fixed-page reader.
use image::{DynamicImage, ImageReader, Limits};
use ratatui::{
    Frame,
    layout::{Rect, Size},
};
use ratatui_image::sliced::{SlicedImage, SlicedProtocol};
use ratatui_image::{
    Image, Resize,
    picker::{Picker, ProtocolType, cap_parser::QueryStdioOptions},
    protocol::Protocol,
};
use std::{
    cell::{Cell, RefCell},
    io::Cursor,
};

pub(crate) fn graphics() -> Picker {
    thread_local! { static PICKER: Picker = detect(); }
    PICKER.with(Clone::clone)
}
fn detect() -> Picker {
    let term = std::env::var("TERM").unwrap_or_default();
    let brand = std::env::var("TERM_PROGRAM").unwrap_or_default();
    // The image picker wraps capability queries and graphics for tmux.
    let tmux = term.starts_with("tmux") || brand == "tmux";
    let candidate =
        tmux || matches!(
            term.as_str(),
            "xterm-kitty" | "xterm-ghostty" | "foot" | "foot-extra"
        ) || matches!(brand.as_str(), "WezTerm" | "iTerm.app" | "ghostty");
    if !monochrome()
        && std::env::var_os("STY").is_none()
        && candidate
        && let Ok(p) = Picker::from_query_stdio_with_options(graphics_options())
        && p.protocol_type() != ProtocolType::Halfblocks
    {
        return p;
    }
    Picker::halfblocks()
}

fn graphics_options() -> QueryStdioOptions {
    QueryStdioOptions {
        // Enable the fast path only when the emulator can open a probe on the
        // same host. Its successful reply selects SHM for subsequent images.
        #[cfg(unix)]
        kitty_shared_memory_object: if std::env::var_os("SSH_CONNECTION").is_none()
            && std::env::var_os("SSH_TTY").is_none()
        {
            QueryStdioOptions::probe_kitty_smo()
        } else {
            None
        },
        ..Default::default()
    }
}

fn monochrome() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|s| !s.is_empty())
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct Crop {
    pub percent: u16,
    pub x: u8,
    pub y: u8,
}
impl Default for Crop {
    fn default() -> Self {
        Self {
            percent: 100,
            x: 128,
            y: 0,
        }
    }
}
impl Crop {
    pub fn fit(self, area: Rect, both: bool) -> Rect {
        let shrink = |n: u16| (u32::from(n) * u32::from(self.percent.min(100)) / 100).max(1) as u16;
        crate::centered(
            area,
            shrink(area.width),
            if both {
                shrink(area.height)
            } else {
                area.height
            },
        )
    }
    pub fn step(&mut self, more: bool) {
        const STEPS: &[u16] = &[
            25, 40, 50, 60, 70, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400,
        ];
        self.percent = if more {
            STEPS
                .iter()
                .copied()
                .find(|p| *p > self.percent)
                .unwrap_or(400)
        } else {
            STEPS
                .iter()
                .rev()
                .copied()
                .find(|p| *p < self.percent)
                .unwrap_or(25)
        };
    }
    fn span(self, n: u32) -> u32 {
        (u64::from(n) * 100 / u64::from(self.percent.max(100))).max(1) as u32
    }
}
#[derive(Copy, Clone, PartialEq, Eq)]
struct StripKey {
    skip: u32,
    rows: u32,
    area: Size,
    crop: Crop,
}
#[derive(Copy, Clone, PartialEq, Eq)]
struct CellStrip {
    width: u16,
    rows: u32,
    x: u8,
    zoom: u16,
}
// A terminal may evict non-visible Kitty uploads while a large PDF is open.
thread_local! { static COVER_EPOCH: Cell<u64> = const {Cell::new(0)}; }
pub(crate) struct ScanGraphics;
impl Drop for ScanGraphics {
    fn drop(&mut self) {
        COVER_EPOCH.set(COVER_EPOCH.get().wrapping_add(1));
    }
}
pub(crate) struct Picture {
    epoch: Cell<u64>,
    sliced: RefCell<Option<(CellStrip, SlicedProtocol)>>,
    source: DynamicImage,
    is_cover: bool,
    strip: RefCell<Option<(StripKey, Box<Picture>)>>,
    strip_cells: RefCell<Option<(CellStrip, image::RgbImage)>>,
    ready: RefCell<Vec<(Size, Crop, bool, Protocol)>>,
}

impl Picture {
    pub fn new(source: DynamicImage) -> Self {
        Self {
            epoch: Cell::default(),
            sliced: Default::default(),
            source,
            is_cover: false,
            strip: Default::default(),
            strip_cells: Default::default(),
            ready: RefCell::new(Vec::new()),
        }
    }

    pub fn generated_cover(source: DynamicImage) -> Self {
        let mut picture = Self::new(source);
        picture.is_cover = true;
        picture
    }

    pub fn cover(bytes: Vec<u8>) -> Option<Self> {
        let mut picture = Self::decode(bytes, 240, 360)?;
        picture.is_cover = true;
        Some(picture)
    }
    pub fn inline(bytes: Vec<u8>) -> Option<Self> {
        Self::decode(bytes, 720, 540)
    }
    pub fn illustration(bytes: Vec<u8>) -> Option<Self> {
        Self::decode(bytes, 2048, 2048)
    }
    fn decode(bytes: Vec<u8>, width: u32, height: u32) -> Option<Self> {
        let mut decoder = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(32 << 20);
        decoder.limits(limits);
        Some(Self::new(decoder.decode().ok()?.thumbnail(width, height)))
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.source.width(), self.source.height())
    }
    pub fn draw_strip(
        &self,
        f: &mut Frame,
        area: Rect,
        picker: &Picker,
        crop: Crop,
        skip: u32,
        rows: u32,
    ) {
        if area.is_empty() || rows == 0 {
            return;
        }
        if picker.protocol_type() == ProtocolType::Halfblocks || monochrome() {
            let key = CellStrip {
                width: area.width,
                rows,
                x: crop.x,
                zoom: crop.percent,
            };
            let mut cache = self.strip_cells.borrow_mut();
            if cache.as_ref().is_none_or(|(k, _)| *k != key) {
                let (w, h) = self.dimensions();
                let cw = crop.span(w);
                let x = (w - cw) * u32::from(crop.x) / 255;
                let width = u32::from(area.width);
                let height = (rows * 2).min((2_000_000 / width).max(1));
                let pix = self
                    .source
                    .crop_imm(x, 0, cw, h)
                    .resize_exact(width, height, image::imageops::FilterType::Triangle)
                    .into_rgb8();
                *cache = Some((key, pix));
            }
            if let Some((_, im)) = &*cache {
                let mono = monochrome();
                let color = |p: &image::Rgb<u8>| ratatui::style::Color::Rgb(p[0], p[1], p[2]);
                for y in 0..area.height {
                    let row = (skip + u32::from(y)) * 2;
                    let pixel = |dy| {
                        ((u64::from(row + dy) * u64::from(im.height()) / u64::from(rows * 2))
                            as u32)
                            .min(im.height() - 1)
                    };
                    for x in 0..area.width {
                        let a = im.get_pixel(x.into(), pixel(0));
                        let b = im.get_pixel(x.into(), pixel(1));
                        let cell = &mut f.buffer_mut()[(area.x + x, area.y + y)];
                        if mono {
                            const SHADE: &[u8] = b" .:-=+*#%@";
                            let l = (299 * usize::from(a[0])
                                + 587 * usize::from(a[1])
                                + 114 * usize::from(a[2]))
                                / 1000;
                            cell.set_char(SHADE[l * (SHADE.len() - 1) / 255] as char);
                        } else {
                            cell.set_char('▀').set_fg(color(a)).set_bg(color(b));
                        }
                    }
                }
            }
            return;
        }
        // Kitty can display another row of the same upload. Bound the virtual
        // sheet (including row diacritics) before asking the terminal to scale it.
        let font = picker.font_size();
        if picker.protocol_type() == ProtocolType::Kitty
            && rows <= 280
            && area.width <= 280
            && u64::from(area.width)
                * u64::from(font.width)
                * u64::from(rows)
                * u64::from(font.height)
                <= 4_000_000
        {
            let key = CellStrip {
                width: area.width,
                rows,
                x: crop.x,
                zoom: crop.percent,
            };
            let mut cache = self.sliced.borrow_mut();
            if cache.as_ref().is_none_or(|(k, _)| *k != key) {
                let (w, h) = self.dimensions();
                let cw = crop.span(w);
                let x = (w - cw) * u32::from(crop.x) / 255;
                let image = self.source.crop_imm(x, 0, cw, h);
                *cache = SlicedProtocol::new_with_resize(
                    picker,
                    image,
                    Size::new(area.width, rows as u16),
                    Resize::Scale(Some(image::imageops::FilterType::Triangle)),
                )
                .ok()
                .map(|p| (key, p));
            }
            if let Some((_, proto)) = &*cache {
                f.render_widget(SlicedImage::new(proto, (0, -(skip as i16)).into()), area);
                return;
            }
        }
        let key = StripKey {
            skip,
            rows,
            area: area.as_size(),
            crop,
        };
        let mut cache = self.strip.borrow_mut();
        if cache.as_ref().is_none_or(|(k, _)| *k != key) {
            let (w, h) = self.dimensions();
            let cw = crop.span(w);
            let x = (w - cw) * u32::from(crop.x) / 255;
            let y = (u64::from(h) * u64::from(skip) / u64::from(rows)).min(u64::from(h - 1)) as u32;
            let end = (u64::from(h) * u64::from(skip + u32::from(area.height)))
                .div_ceil(u64::from(rows))
                .min(u64::from(h)) as u32;
            *cache = Some((
                key,
                Box::new(Self::new(self.source.crop_imm(
                    x,
                    y,
                    cw,
                    end.saturating_sub(y).max(1),
                ))),
            ));
        }
        if let Some((_, page)) = &*cache {
            page.draw(f, area, picker);
        }
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, picker: &Picker) {
        self.draw_region(frame, area, picker, Crop::default());
    }
    pub fn draw_region(&self, frame: &mut Frame, area: Rect, picker: &Picker, crop: Crop) {
        self.draw_region_mode(frame, area, picker, crop, monochrome());
    }
    fn draw_region_mode(
        &self,
        frame: &mut Frame,
        area: Rect,
        picker: &Picker,
        crop: Crop,
        monochrome: bool,
    ) {
        if area.is_empty() {
            return;
        }
        let rounded = self.is_cover
            && crate::appearance::BordersStyle::current()
                == crate::appearance::BordersStyle::Rounded;
        let source = || {
            if crop.percent <= 100 {
                return if rounded {
                    rounded_cover(&self.source)
                } else {
                    self.source.clone()
                };
            }
            let w = crop.span(self.source.width());
            let h = crop.span(self.source.height());
            self.source.crop_imm(
                (self.source.width() - w) * u32::from(crop.x) / u32::from(u8::MAX),
                (self.source.height() - h) * u32::from(crop.y) / u32::from(u8::MAX),
                w,
                h,
            )
        };
        if monochrome {
            let raster = source()
                .thumbnail(u32::from(area.width), u32::from(area.height) * 2)
                .into_luma_alpha8();
            let h = (raster.height() as u16).div_ceil(2);
            let r = crate::centered(area, raster.width() as u16, h);
            const SHADE: &[u8] = b" .:-=+*#%@";
            for y in 0..r.height {
                for x in 0..r.width {
                    let pixel = raster.get_pixel(x.into(), u32::from(y) * 2);
                    if pixel[1] < 128 {
                        continue;
                    }
                    let lum = pixel[0];
                    frame.buffer_mut()[(r.x + x, r.y + y)]
                        .set_char(SHADE[usize::from(lum) * (SHADE.len() - 1) / 255] as char);
                }
            }
            return;
        }
        let size = area.as_size();
        let mut ready = self.ready.borrow_mut();
        if self.is_cover
            && picker.protocol_type() == ProtocolType::Kitty
            && self.epoch.replace(COVER_EPOCH.get()) != COVER_EPOCH.get()
        {
            ready.clear();
        }

        let hit = ready.iter().position(|(old, view, corners, _)| {
            *old == size && *view == crop && *corners == rounded
        });
        let index = if let Some(index) = hit {
            index
        } else {
            let Ok(protocol) = picker.new_protocol(
                source(),
                size,
                Resize::Scale(Some(image::imageops::FilterType::Triangle)),
            ) else {
                return;
            };
            // One inline picture may occupy both columns at different heights.
            // Retain both small renditions instead of resizing twice per frame.
            // Large scans retain only one rendition to bound raster memory.
            let limit = if self.source.width() <= 720 && self.source.height() <= 540 {
                2
            } else {
                1
            };
            if ready.len() == limit {
                ready.remove(0);
            }
            ready.push((size, crop, rounded, protocol));
            ready.len() - 1
        };
        let p = &ready[index].3;
        let size = p.size();
        frame.render_widget(
            Image::new(p),
            crate::centered(area, size.width, size.height),
        );
    }
}

fn rounded_cover(source: &DynamicImage) -> DynamicImage {
    let mut pixels = source.to_rgba8();
    let (w, h) = pixels.dimensions();
    // Scale the bend with the jacket so thumbnails and the detail view match.
    let radius = w.min(h) / 10;
    for y in 0..radius {
        for x in 0..radius {
            let dx = (radius - x) as f32 - 0.5;
            let dy = (radius - y) as f32 - 0.5;
            let opacity = (radius as f32 + 0.5 - dx.hypot(dy)).clamp(0., 1.);
            for (cx, cy) in [
                (x, y),
                (w - 1 - x, y),
                (x, h - 1 - y),
                (w - 1 - x, h - 1 - y),
            ] {
                let alpha = &mut pixels.get_pixel_mut(cx, cy)[3];
                *alpha = (f32::from(*alpha) * opacity).round() as u8;
            }
        }
    }
    DynamicImage::ImageRgba8(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_visible_sizes_reuse_the_same_small_picture_protocols() {
        let picture = Picture::new(DynamicImage::new_rgb8(100, 150));
        let picker = Picker::halfblocks();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(30, 20)).unwrap();
        for height in [10, 8, 10, 8] {
            terminal
                .draw(|f| {
                    picture.draw_region_mode(
                        f,
                        Rect::new(0, 0, 20, height),
                        &picker,
                        Crop::default(),
                        false,
                    )
                })
                .unwrap();
        }
        let ready = picture.ready.borrow();
        assert_eq!(ready.len(), 2);
        // Reusing a size does not discard the other side of the spread.
        assert_eq!(ready[0].0, Size::new(20, 10));
        assert_eq!(ready[1].0, Size::new(20, 8));
    }
}
