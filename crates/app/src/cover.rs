//! Terminal image rendering shared by the shelf and fixed-page reader.
use image::{DynamicImage, ImageReader, Limits};
use ratatui::{
    Frame,
    layout::{Rect, Size},
};
use ratatui_image::{
    Image, Resize,
    picker::{Picker, ProtocolType},
    protocol::Protocol,
};
use std::{cell::RefCell, io::Cursor};

pub(crate) fn graphics() -> Picker {
    thread_local! { static PICKER: Picker = detect(); }
    PICKER.with(Clone::clone)
}
fn detect() -> Picker {
    // Multiplexers may inherit KITTY_WINDOW_ID without passing graphics through.
    let direct = std::env::var_os("TMUX").is_none() && std::env::var_os("STY").is_none();
    let term = std::env::var("TERM").unwrap_or_default();
    let brand = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let candidate = matches!(
        term.as_str(),
        "xterm-kitty" | "xterm-ghostty" | "foot" | "foot-extra"
    ) || matches!(brand.as_str(), "WezTerm" | "iTerm.app" | "ghostty");
    if !monochrome()
        && direct
        && candidate
        && let Ok(p) = Picker::from_query_stdio()
        && p.protocol_type() != ProtocolType::Halfblocks
    {
        return p;
    }
    Picker::halfblocks()
}

fn monochrome() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|s| !s.is_empty())
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(crate) struct Crop {
    pub level: u8,
    pub x: u8,
    pub y: u8,
}
pub(crate) struct Picture {
    source: DynamicImage,
    ready: RefCell<Option<(Size, Crop, Protocol)>>,
}

impl Picture {
    pub fn new(source: DynamicImage) -> Self {
        Self {
            source,
            ready: RefCell::new(None),
        }
    }

    pub fn cover(bytes: Vec<u8>) -> Option<Self> {
        Self::decode(bytes, 240, 360)
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

    pub fn draw(&self, frame: &mut Frame, area: Rect, picker: &Picker) {
        self.draw_region(frame, area, picker, Crop::default());
    }
    pub fn draw_region(&self, frame: &mut Frame, area: Rect, picker: &Picker, crop: Crop) {
        if area.is_empty() {
            return;
        }
        let source = || {
            if crop.level == 0 {
                return self.source.clone();
            }
            let scale = 1u32 << crop.level.min(2);
            let w = (self.source.width() / scale).max(1);
            let h = (self.source.height() / scale).max(1);
            self.source.crop_imm(
                (self.source.width() - w) * u32::from(crop.x) / u32::from(u8::MAX),
                (self.source.height() - h) * u32::from(crop.y) / u32::from(u8::MAX),
                w,
                h,
            )
        };
        if monochrome() {
            let raster = source()
                .thumbnail(u32::from(area.width), u32::from(area.height) * 2)
                .into_luma8();
            let h = (raster.height() as u16).div_ceil(2);
            let r = crate::centered(area, raster.width() as u16, h);
            const SHADE: &[u8] = b" .:-=+*#%@";
            for y in 0..r.height {
                for x in 0..r.width {
                    let lum = raster.get_pixel(x.into(), u32::from(y) * 2)[0];
                    frame.buffer_mut()[(r.x + x, r.y + y)]
                        .set_char(SHADE[usize::from(lum) * (SHADE.len() - 1) / 255] as char);
                }
            }
            return;
        }
        let size = area.as_size();
        let mut ready = self.ready.borrow_mut();
        if ready
            .as_ref()
            .is_none_or(|(old, view, _)| *old != size || *view != crop)
        {
            *ready = picker
                .new_protocol(
                    source(),
                    size,
                    Resize::Scale(Some(image::imageops::FilterType::Triangle)),
                )
                .ok()
                .map(|p| (size, crop, p));
        }
        if let Some((_, _, p)) = &*ready {
            let size = p.size();
            frame.render_widget(
                Image::new(p),
                crate::centered(area, size.width, size.height),
            );
        }
    }
}
