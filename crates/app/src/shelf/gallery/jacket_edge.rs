//! Keep jacket lettering as terminal text. Only the four little arcs need pixels.
use crate::appearance::BordersStyle;
use image::{DynamicImage, Rgba, RgbaImage};
use lru::LruCache;
use ratatui::{
    Frame,
    layout::{Rect, Size},
    style::{Color, Style},
    widgets::{Block, BorderType},
};
use ratatui_image::{
    Image, Resize,
    picker::{Picker, ProtocolType},
    protocol::Protocol,
};
use std::{cell::RefCell, num::NonZeroUsize};

type Ink = (u8, u8, u8);
type Key = (Ink, u16, u16, u8);
thread_local! {
    static CORNERS: RefCell<LruCache<Key, Protocol>> = RefCell::new(LruCache::new(NonZeroUsize::new(48).unwrap()));
}

fn corner((r, g, b): Ink, w: u16, h: u16, i: u8) -> DynamicImage {
    let radius = (f32::from(w.min(h)) * 0.80).max(1.0);
    DynamicImage::ImageRgba8(RgbaImage::from_fn(w.into(), h.into(), |x, y| {
        let x = if i & 1 == 0 { x } else { u32::from(w) - 1 - x };
        let y = if i & 2 == 0 { y } else { u32::from(h) - 1 - y };
        let dx = (radius - (x as f32 + 0.5)).max(0.0);
        let dy = (radius - (y as f32 + 0.5)).max(0.0);
        Rgba([
            r,
            g,
            b,
            ((radius + 0.5 - dx.hypot(dy)).clamp(0.0, 1.0) * 255.0).round() as u8,
        ])
    }))
}

pub(super) fn paper(f: &mut Frame, area: Rect, cloth: Style, ink: Ink, graphics: &Picker) -> Rect {
    if BordersStyle::current() != BordersStyle::Rounded || area.width < 4 || area.height < 3 {
        f.render_widget(Block::new().style(cloth), area);
        return area;
    }
    let size = graphics.font_size();
    if graphics.protocol_type() != ProtocolType::Halfblocks
        && (1..=128).contains(&size.width)
        && (1..=256).contains(&size.height)
    {
        // Leave the actual cell background alone under transparent edge pixels.
        f.render_widget(
            Block::new().style(cloth),
            Rect::new(area.x + 1, area.y, area.width - 2, area.height),
        );
        f.render_widget(
            Block::new().style(cloth),
            Rect::new(area.x, area.y + 1, area.width, area.height - 2),
        );
        let rendered = CORNERS.with_borrow_mut(|c| {
            for i in 0..4 {
                let key = (ink, size.width, size.height, i);
                if !c.contains(&key) {
                    let Ok(p) = graphics.new_protocol(
                        corner(ink, size.width, size.height, i),
                        Size::new(1, 1),
                        Resize::Fit(None),
                    ) else {
                        return false;
                    };
                    c.put(key, p);
                }
                let x = if i & 1 == 0 { area.x } else { area.right() - 1 };
                let y = if i & 2 == 0 {
                    area.y
                } else {
                    area.bottom() - 1
                };
                f.render_widget(Image::new(c.get(&key).unwrap()), Rect::new(x, y, 1, 1));
            }
            true
        });
        if rendered {
            return area;
        }
    }
    // A terminal font has continuous box-drawing arcs, but no filled quarter circle.
    // Frame the inset cloth all the way around rather than cutting a half-cell out of each end.
    let (r, g, b) = ink;
    let edge = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::Rgb(
            r.saturating_add(40),
            g.saturating_add(40),
            b.saturating_add(40),
        )));
    let inner = edge.inner(area);
    f.render_widget(edge, area);
    f.render_widget(Block::new().style(cloth), inner);
    inner
}
