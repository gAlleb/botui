//! Terminal image rendering shared by the shelf and fixed-page reader.
pub(crate) mod kitty_lifetime;
pub(crate) mod sixel;
mod text_overlay;
use image::{DynamicImage, ImageReader, Limits};
use ratatui::{
    Frame,
    layout::{Rect, Size},
};
use ratatui_image::sliced::{SlicedImage, SlicedProtocol};
use ratatui_image::{
    FontSize, Image, Resize,
    picker::{Capability, Picker, ProtocolType},
    protocol::Protocol,
};
use std::{
    cell::{Cell, RefCell},
    io::Cursor,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellMetrics {
    Pixels { size: Size, exact: bool },
    GridOnly(Size),
}

thread_local! { static GRAPHICS: RefCell<Option<(Picker, Option<CellMetrics>)>> = const { RefCell::new(None) }; }

/// TIOCGWINSZ can include extra padding beyond the terminal grid.
fn cell_metrics() -> Option<CellMetrics> {
    let w = crossterm::terminal::window_size().ok()?;
    let width = w.width.checked_div(w.columns)?;
    let height = w.height.checked_div(w.rows)?;
    Some(if width == 0 || height == 0 {
        CellMetrics::GridOnly(Size::new(w.columns, w.rows))
    } else {
        CellMetrics::Pixels {
            size: Size::new(width, height),
            exact: w.width % w.columns == 0 && w.height % w.rows == 0,
        }
    })
}

pub(crate) fn graphics() -> Picker {
    GRAPHICS.with_borrow_mut(|cache| {
        cache
            .get_or_insert_with(|| {
                // Associate the reply with the grid that initiated the query.
                // A further resize can arrive while the terminal is answering.
                let previous = cell_metrics();
                (detect(), previous)
            })
            .0
            .clone()
    })
}

pub(crate) fn sixel_active() -> bool {
    GRAPHICS.with_borrow(|cache| {
        cache
            .as_ref()
            .is_some_and(|(picker, _)| picker.protocol_type() == ProtocolType::Sixel)
    })
}

pub(crate) fn resized() {
    GRAPHICS.with_borrow_mut(|cache| {
        if let Some((picker, previous)) = cache
            && picker.protocol_type() != ProtocolType::Halfblocks
            && let Some(current) = cell_metrics()
            && *previous != Some(current)
        {
            let font = picker.font_size();
            let measured = Size::new(font.width, font.height);
            if let CellMetrics::Pixels { size, exact: true } = current
                && matches!(previous, Some(CellMetrics::Pixels { size, .. }) if *size == measured)
                && !picker.tmux_detected()
                && !picker
                    .capabilities()
                    .contains(&Capability::KittyCompression)
            {
                // This tty already confirmed its pixel metrics and image protocol.
                // A full device query on each zoom blocks the next text frame.
                let protocol = picker.protocol_type();
                #[allow(deprecated)] // Keep the negotiated protocol, not the environment guess.
                let mut resized = Picker::from_fontsize(FontSize::new(size.width, size.height));
                resized.set_protocol_type(protocol);
                *cache = Some((resized, Some(current)));
            } else {
                // A multiplexer can change clients, and rounded metrics are ambiguous.
                *cache = None;
            }
        }
    });
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
        && let Some(p) = crate::terminal_input::graphics(Default::default())
        && p.protocol_type() != ProtocolType::Halfblocks
    {
        return p;
    }
    Picker::halfblocks()
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
    pub(crate) fn span(self, n: u32) -> u32 {
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
    scan: bool,
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImagePlacement {
    Fit,
    ShelfCard,
}

impl ImagePlacement {
    pub(crate) fn text_area(self, slot: Rect) -> Rect {
        if self == Self::ShelfCard && slot.height >= 6 && slot.width >= 6 {
            slot.inner(ratatui::layout::Margin::new(1, 1))
        } else {
            slot
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Rendition {
    size: Size,
    crop: Crop,
    rounded: bool,
    placement: ImagePlacement,
}

type PageWindow = (Size, [f32; 4], Box<Picture>);

// Row and column offsets must fit in the Kitty placeholder diacritic table.
const MAX_INLINE_CELLS: u16 = 280;

pub(crate) struct Picture {
    pub(crate) identity: u64,
    epoch: Cell<u64>,
    cell_pixels: Cell<Size>,
    sliced: RefCell<Option<(CellStrip, kitty_lifetime::Managed<SlicedProtocol>)>>,
    inline_kitty: RefCell<Option<kitty_lifetime::InlineImage>>,
    source: DynamicImage,
    is_cover: bool,
    strip: RefCell<Option<(StripKey, Box<Picture>)>>,
    strip_cells: RefCell<Option<(CellStrip, image::RgbImage)>>,
    ready: RefCell<Vec<(Rendition, kitty_lifetime::Managed<Protocol>)>>,
    page_window: RefCell<Option<PageWindow>>,
}

impl Picture {
    pub fn new(source: DynamicImage) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            identity: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            epoch: Cell::default(),
            cell_pixels: Cell::default(),
            sliced: Default::default(),
            inline_kitty: Default::default(),
            source,
            is_cover: false,
            strip: Default::default(),
            strip_cells: Default::default(),
            ready: RefCell::new(Vec::new()),
            page_window: RefCell::new(None),
        }
    }

    pub fn generated_cover(source: DynamicImage) -> Self {
        let mut picture = Self::new(source);
        picture.is_cover = true;
        picture
    }

    pub fn cover(bytes: Vec<u8>) -> Option<Self> {
        let mut picture = Self::decode(&bytes, 240, 360)?;
        picture.is_cover = true;
        Some(picture)
    }
    pub fn inline(bytes: Vec<u8>, picker: &Picker, bounds: Size) -> Option<Self> {
        let picture = Self::decode(&bytes, 720, 540)?;
        if !monochrome() {
            picture.refresh_resolution(picker);
            if let (_, Some(key)) = picture.inline_geometry(picker, bounds) {
                if picker.protocol_type() == ProtocolType::Kitty {
                    picture.prepare_inline_kitty(picker);
                } else {
                    picture.prepare_sliced(picker, key);
                }
            }
        }
        Some(picture)
    }
    pub fn illustration(bytes: &[u8]) -> Option<Self> {
        Self::decode(bytes, 2048, 2048)
    }
    fn decode(bytes: &[u8], width: u32, height: u32) -> Option<Self> {
        let mut decoder = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        if decoder.format().is_none() {
            return botui_fixed::svg::render(bytes, width, height).map(Self::new);
        }
        let mut limits = Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(32 << 20);
        decoder.limits(limits);
        Some(Self::new(decoder.decode().ok()?.thumbnail(width, height)))
    }

    pub fn illustration_png(&self) -> botui_bookorbit::Result<Vec<u8>> {
        crate::cover_viewer::png(&self.source)
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.source.width(), self.source.height())
    }
    fn refresh_resolution(&self, picker: &Picker) {
        let font = picker.font_size();
        let pixels = Size::new(font.width, font.height);
        if self.cell_pixels.replace(pixels) != pixels {
            self.ready.borrow_mut().clear();
            self.sliced.take();
            self.page_window.take();
        }
    }
    /// Fit the whole illustration once. Scrolling changes only which rows are painted.
    pub fn draw_inline(
        &self,
        frame: &mut Frame,
        area: Rect,
        picker: &Picker,
        first: u16,
        height: u16,
    ) -> Option<Rect> {
        if area.is_empty() || height == 0 {
            return None;
        }
        if monochrome() {
            return draw_mono(frame, &self.source, area, first, height);
        }
        self.refresh_resolution(picker);
        let (size, key) = self.inline_geometry(picker, Size::new(area.width, height));
        let (visible, skip) = visible_figure(area, first, height, size)?;
        if key.is_some() && picker.protocol_type() == ProtocolType::Kitty {
            self.prepare_inline_kitty(picker);
            if let Some(image) = self.inline_kitty.borrow_mut().as_mut() {
                image.draw(frame, visible, size, skip);
                return Some(visible);
            }
        }
        if let Some(key) = key
            && self.draw_sliced(frame, visible, picker, key, skip)
        {
            return Some(visible);
        }
        self.draw_strip(
            frame,
            visible,
            picker,
            Crop::default(),
            skip.into(),
            size.height.into(),
        );
        Some(visible)
    }

    fn prepare_inline_kitty(&self, picker: &Picker) {
        let mut cache = self.inline_kitty.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|image| image.tmux() != picker.tmux_detected())
        {
            *cache = kitty_lifetime::InlineImage::new(picker, self.source.clone()).ok();
        }
    }

    fn inline_geometry(&self, picker: &Picker, bounds: Size) -> (Size, Option<CellStrip>) {
        let font = picker.font_size();
        let size = Resize::Scale(None).size_for(&self.source, font, bounds);
        // Larger images and iTerm2 use the source-region fallback in draw_inline.
        let pixels = u64::from(size.width)
            * u64::from(font.width)
            * u64::from(size.height)
            * u64::from(font.height);
        let key = (size.width <= MAX_INLINE_CELLS
            && size.height <= MAX_INLINE_CELLS
            && (picker.protocol_type() == ProtocolType::Kitty || pixels <= 4_000_000)
            && picker.protocol_type() != ProtocolType::Iterm2)
            .then_some(CellStrip {
                scan: false,
                width: bounds.width,
                rows: bounds.height.into(),
                x: Crop::default().x,
                zoom: Crop::default().percent,
            });
        (size, key)
    }

    fn draw_sliced(
        &self,
        f: &mut Frame,
        area: Rect,
        picker: &Picker,
        key: CellStrip,
        skip: u16,
    ) -> bool {
        self.prepare_sliced(picker, key);
        let cache = self.sliced.borrow();
        if let Some((_, protocol)) = &*cache {
            f.render_widget(SlicedImage::new(protocol, (0, -(skip as i16)).into()), area);
            return true;
        }
        false
    }
    fn prepare_sliced(&self, picker: &Picker, key: CellStrip) {
        let mut cache = self.sliced.borrow_mut();
        if cache.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let (w, h) = self.dimensions();
        let cw = Crop {
            percent: key.zoom,
            ..Crop::default()
        }
        .span(w);
        let x = (w - cw) * u32::from(key.x) / 255;
        let source = self.source.crop_imm(x, 0, cw, h);
        let size = Size::new(key.width, key.rows as u16);
        let protocol = kitty_lifetime::new_sliced(picker, source, size, key.scan);
        *cache = protocol.ok().map(|p| (key, p));
    }

    /// Prepare a neighbouring scan on its render worker, before it enters the strip.
    pub(super) fn prepare_scan_strip(&self, picker: &Picker, width: u16, crop: Crop) {
        let rows = self.strip_rows(width, picker, crop);
        if picker.protocol_type() == ProtocolType::Kitty && rows <= 280 && width <= 280 {
            self.refresh_resolution(picker);
            self.prepare_sliced(
                picker,
                CellStrip {
                    scan: true,
                    width,
                    rows,
                    x: crop.x,
                    zoom: crop.percent,
                },
            );
        }
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
        self.refresh_resolution(picker);
        if picker.protocol_type() == ProtocolType::Halfblocks || monochrome() {
            let key = CellStrip {
                scan: true,
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
        // Reuse the bounded source upload while Kitty moves the virtual placement.
        if picker.protocol_type() == ProtocolType::Kitty && rows <= 280 && area.width <= 280 {
            let key = CellStrip {
                scan: true,
                width: area.width,
                rows,
                x: crop.x,
                zoom: crop.percent,
            };
            if self.draw_sliced(f, area, picker, key, skip as u16) {
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
                Box::new(self.scaled_window(
                    [
                        x as f32 / w as f32,
                        y as f32 / h as f32,
                        (x + cw) as f32 / w as f32,
                        end as f32 / h as f32,
                    ],
                    area.as_size(),
                    picker,
                )),
            ));
        }
        if let Some((_, page)) = &*cache {
            page.draw(f, area, picker);
        }
    }
    pub fn draw_window(&self, frame: &mut Frame, area: Rect, picker: &Picker, window: [f32; 4]) {
        self.refresh_resolution(picker);
        let mut cache = self.page_window.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|(size, bounds, _)| *size != area.as_size() || *bounds != window)
        {
            *cache = Some((
                area.as_size(),
                window,
                Box::new(self.scaled_window(window, area.as_size(), picker)),
            ));
        }
        if let Some((_, _, picture)) = &*cache {
            picture.draw(frame, area, picker);
        }
    }
    pub fn draw(&self, frame: &mut Frame, area: Rect, picker: &Picker) {
        self.draw_region(frame, area, picker, Crop::default());
    }
    pub fn draw_region(&self, frame: &mut Frame, area: Rect, picker: &Picker, crop: Crop) {
        self.draw_region_mode(frame, area, picker, crop, monochrome(), ImagePlacement::Fit);
    }
    pub fn draw_cover(
        &self,
        frame: &mut Frame,
        area: Rect,
        picker: &Picker,
        placement: ImagePlacement,
    ) {
        // Half blocks have no alpha channel. Leave the margin as terminal
        // cells so it keeps the current background, including transparency.
        if picker.protocol_type() == ProtocolType::Halfblocks {
            self.draw(frame, placement.text_area(area), picker);
            return;
        }
        self.draw_region_mode(
            frame,
            area,
            picker,
            Crop::default(),
            monochrome(),
            placement,
        );
    }
    fn draw_region_mode(
        &self,
        frame: &mut Frame,
        area: Rect,
        picker: &Picker,
        crop: Crop,
        monochrome: bool,
        placement: ImagePlacement,
    ) {
        if area.is_empty() {
            return;
        }
        self.refresh_resolution(picker);
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
            draw_mono(frame, &source(), area, 0, area.height);
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

        let key = Rendition {
            size,
            crop,
            rounded,
            placement,
        };
        let hit = ready.iter().position(|(entry, _)| *entry == key);
        let index = if let Some(index) = hit {
            index
        } else {
            let source = source();
            let result = match placement {
                ImagePlacement::Fit => kitty_lifetime::new_protocol(
                    picker,
                    source,
                    size,
                    Resize::Scale(Some(image::imageops::FilterType::Triangle)),
                ),
                ImagePlacement::ShelfCard => {
                    let font = picker.font_size();
                    let (image, cells) =
                        card_raster(&source, size, Size::new(font.width, font.height));
                    kitty_lifetime::new_protocol(picker, image, cells, Resize::Fit(None))
                }
            };
            let Ok(protocol) = result else {
                return;
            };
            // Keep small pictures ready for two different display sizes.
            // Large scans retain only one rendition to bound raster memory.
            let limit = if self.source.width() <= 720 && self.source.height() <= 540 {
                2
            } else {
                1
            };
            if ready.len() == limit {
                ready.remove(0);
            }
            ready.push((key, protocol));
            ready.len() - 1
        };
        let p = &ready[index].1;
        let size = p.size();
        frame.render_widget(
            Image::new(p),
            crate::centered(area, size.width, size.height),
        );
    }
}

/// Intersect the centered, fitted image with the visible rows of its layout box.
fn visible_figure(area: Rect, first: u16, height: u16, size: Size) -> Option<(Rect, u16)> {
    let top = height.saturating_sub(size.height) / 2;
    let start = first.max(top);
    let end = first
        .saturating_add(area.height)
        .min(top.saturating_add(size.height));
    if end <= start || size.width == 0 {
        return None;
    }
    let x = area.x + area.width.saturating_sub(size.width) / 2;
    Some((
        Rect::new(x, area.y + (start - first), size.width, end - start),
        start - top,
    ))
}

fn draw_mono(
    frame: &mut Frame,
    source: &DynamicImage,
    area: Rect,
    first: u16,
    height: u16,
) -> Option<Rect> {
    let raster = source
        .thumbnail(u32::from(area.width), u32::from(height) * 2)
        .into_luma_alpha8();
    let size = Size::new(raster.width() as u16, raster.height().div_ceil(2) as u16);
    let (r, skip) = visible_figure(area, first, height, size)?;
    const SHADE: &[u8] = b" .:-=+*#%@";
    for y in 0..r.height {
        for x in 0..r.width {
            let pixel = raster.get_pixel(x.into(), u32::from(skip + y) * 2);
            if pixel[1] >= 128 {
                frame.buffer_mut()[(r.x + x, r.y + y)]
                    .set_char(SHADE[usize::from(pixel[0]) * (SHADE.len() - 1) / 255] as char);
            }
        }
    }
    Some(r)
}

fn card_raster(source: &DynamicImage, slot: Size, cell: Size) -> (DynamicImage, Size) {
    let cw = u32::from(cell.width.max(1));
    let ch = u32::from(cell.height.max(1));
    let width = u32::from(slot.width) * cw;
    let height = u32::from(slot.height) * ch;
    // Reserve a third of a text line on each edge, including above the caption.
    let padding = if slot.height >= 3 {
        (2 * ch + 1) / 3
    } else {
        0
    };
    let image = source.resize(
        width.saturating_sub(padding).max(1),
        height.saturating_sub(padding).max(1),
        image::imageops::FilterType::Triangle,
    );
    // Drop equal numbers of whole blank cells from opposite edges. Keep the
    // sub-cell margins in the upload so every protocol centers the actual ink.
    let canvas_axis = |available: u32, ink: u32, cell: u32| {
        available - (available - ink) / (2 * cell) * (2 * cell)
    };
    let w = canvas_axis(width, image.width(), cw);
    let h = canvas_axis(height, image.height(), ch);
    let mut canvas = image::RgbaImage::new(w, h);
    image::imageops::replace(
        &mut canvas,
        &image,
        i64::from((w - image.width()) / 2),
        i64::from((h - image.height()) / 2),
    );
    (
        DynamicImage::ImageRgba8(canvas),
        Size::new((w / cw) as u16, (h / ch) as u16),
    )
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
    fn inline_scroll_paints_the_same_pixels_as_the_corresponding_rows_of_the_whole_figure() {
        use ratatui::{Terminal, backend::TestBackend};
        let picker = Picker::halfblocks();
        for (width, height) in [(64, 120), (160, 64)] {
            let picture = Picture::new(DynamicImage::ImageRgb8(image::RgbImage::from_fn(
                width,
                height,
                |x, y| image::Rgb([x as u8, y as u8, (x + y) as u8]),
            )));
            for mono in [false, true] {
                let render = |first, visible| {
                    let mut terminal = Terminal::new(TestBackend::new(24, visible)).unwrap();
                    terminal
                        .draw(|f| {
                            let area = f.area();
                            if mono {
                                draw_mono(f, &picture.source, area, first, 12);
                            } else {
                                picture.draw_inline(f, area, &picker, first, 12);
                            }
                        })
                        .unwrap();
                    terminal.backend().buffer().clone()
                };
                let full = render(0, 12);
                assert!(full.content.iter().any(|cell| cell.symbol() != " "));
                for (offset, visible) in [(0, 3), (3, 6), (8, 4), (11, 1)] {
                    let slice = render(offset, visible);
                    for y in 0..visible {
                        for x in 0..24 {
                            assert_eq!(slice[(x, y)], full[(x, offset + y)]);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn halfblock_card_margins_keep_the_terminal_background() {
        use ratatui::{
            style::{Color, Style},
            widgets::Block,
        };
        let picture = Picture::generated_cover(DynamicImage::new_rgb8(100, 150));
        let picker = Picker::halfblocks();
        let background = Color::Blue;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(20, 12)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                frame.render_widget(Block::new().style(Style::new().bg(background)), area);
                picture.draw_cover(frame, area, &picker, ImagePlacement::ShelfCard);
            })
            .unwrap();
        let frame = terminal.backend().buffer();
        for x in 0..20 {
            for y in [0, 11] {
                assert_eq!(frame[(x, y)].bg, background);
                assert_eq!(frame[(x, y)].symbol(), " ");
            }
        }
    }

    #[test]
    fn shelf_art_is_centered_in_pixels_and_leaves_space_above_the_caption() {
        for (source, slot, cell, expected) in [
            ((240, 360), (24, 16), (11, 23), (235, 353)),
            ((240, 360), (25, 16), (11, 23), (235, 353)),
            ((360, 240), (24, 16), (11, 23), (249, 166)),
            ((90, 360), (25, 16), (11, 23), (88, 353)),
            ((240, 360), (20, 10), (10, 20), (125, 187)),
            ((240, 360), (1, 1), (1, 2), (1, 2)),
        ] {
            let (slot, cell) = (Size::new(slot.0, slot.1), Size::new(cell.0, cell.1));
            let source = DynamicImage::new_rgb8(source.0, source.1);
            let (image, footprint) = card_raster(&source, slot, cell);
            let ink = image.as_rgba8().unwrap();
            let mut bounds = (ink.width(), ink.height(), 0, 0);
            for (x, y, pixel) in ink.enumerate_pixels().filter(|(_, _, p)| p[3] != 0) {
                assert_eq!(pixel[3], 255);
                bounds = (
                    bounds.0.min(x),
                    bounds.1.min(y),
                    bounds.2.max(x + 1),
                    bounds.3.max(y + 1),
                );
            }
            assert_eq!((bounds.2 - bounds.0, bounds.3 - bounds.1), expected);
            assert!(bounds.0.abs_diff(ink.width() - bounds.2) <= 1);
            assert!(bounds.1.abs_diff(ink.height() - bounds.3) <= 1);
            assert_eq!((slot.width - footprint.width) % 2, 0);
            assert_eq!((slot.height - footprint.height) % 2, 0);
            assert_eq!(
                (ink.width(), ink.height()),
                (
                    u32::from(footprint.width) * u32::from(cell.width),
                    u32::from(footprint.height) * u32::from(cell.height)
                )
            );
        }
    }

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
                        ImagePlacement::Fit,
                    )
                })
                .unwrap();
        }
        let ready = picture.ready.borrow();
        assert_eq!(ready.len(), 2);
        // Reusing a size does not discard the other side of the spread.
        assert_eq!(ready[0].0.size, Size::new(20, 10));
        assert_eq!(ready[1].0.size, Size::new(20, 8));
    }
}
