//! A continuous strip of raster pages. Keep a fraction within the top page so a resize
//! does not reinterpret a row offset in the old font or viewport geometry.
use super::pages::Pages;
use crate::cover::{Crop, Picture};
use ratatui::{Frame, layout::Rect, style::Stylize, widgets::Paragraph};
use ratatui_image::picker::Picker;
#[derive(Clone, Copy)]
pub(super) struct Viewport<'a> {
    pub side: u16,
    pub width: u16,
    pub height: u16,
    pub picker: &'a Picker,
    pub crop: Crop,
    pub count: u32,
}
pub(super) struct Strip<'a> {
    pub pages: &'a Pages,
    view: Viewport<'a>,
}
impl<'a> Viewport<'a> {
    pub fn strip(self, pages: &'a Pages) -> Strip<'a> {
        Strip { view: self, pages }
    }
}
impl<'a> std::ops::Deref for Strip<'a> {
    type Target = Viewport<'a>;
    fn deref(&self) -> &Self::Target {
        &self.view
    }
}
impl Strip<'_> {
    fn rows(&self, n: u32) -> u32 {
        self.pages
            .get(n)
            .map_or(u32::from(self.height.max(1)), |p| {
                p.strip_rows(self.width, self.picker, self.crop)
            })
            .max(1)
    }
    fn row(&self, n: u32, f: u16) -> u32 {
        let rows = self.rows(n);
        let row = ((u64::from(rows) * u64::from(f) + 32767) / 65535) as u32;
        if n == self.count {
            row.min(rows.saturating_sub(self.height.into()))
        } else {
            row
        }
    }
    fn frac(&self, n: u32, r: u32) -> u16 {
        ((u64::from(r) * 65535 + u64::from(self.rows(n)) / 2) / u64::from(self.rows(n))).min(65535)
            as u16
    }
    pub fn shift(&self, n: &mut u32, f: &mut u16, delta: i32) {
        let mut row = i64::from(self.row(*n, *f)) + i64::from(delta);
        while row < 0 && *n > 1 {
            *n -= 1;
            row += i64::from(self.rows(*n)) + 1;
        }
        while *n < self.count && row > i64::from(self.rows(*n)) {
            row -= i64::from(self.rows(*n)) + 1;
            *n += 1;
        }
        let max = if *n == self.count {
            self.rows(*n).saturating_sub(self.height.into())
        } else {
            self.rows(*n)
        };
        *f = self.frac(*n, row.clamp(0, i64::from(max)) as u32);
    }
    pub fn parts(&self, n: u32, f: u16) -> impl Iterator<Item = (u32, u32, u32, u16, u16)> + '_ {
        let mut n = n;
        let mut skip = self.row(n, f).min(self.rows(n));
        let mut y = 0u16;
        std::iter::from_fn(move || {
            if y >= self.height || n > self.count {
                return None;
            }
            let rows = self.rows(n);
            let take = rows.saturating_sub(skip).min(u32::from(self.height - y)) as u16;
            let result = (n, skip, rows, y, take);
            y = y.saturating_add(take).saturating_add(1);
            n += 1;
            skip = 0;
            Some(result)
        })
    }
    pub fn end(&self, n: u32, f: u16) -> bool {
        self.parts(n, f).any(|(p, skip, rows, _, take)| {
            p == self.count && self.pages.get(p).is_some() && skip + u32::from(take) >= rows
        })
    }
    pub fn draw(
        &self,
        f: &mut Frame,
        r: Rect,
        n: u32,
        offset: u16,
        mut sixel: Option<&mut super::sixel::Cache>,
    ) {
        for (n, skip, rows, y, take) in self.parts(n, offset) {
            // A single quiet row separates pages, including when the viewport starts
            // exactly at the seam. Page pixels and scroll fractions are unchanged.
            let seam = y.saturating_add(take);
            if n < self.count && seam < self.height {
                f.render_widget(
                    Paragraph::new("·".repeat(r.width as usize)).dim(),
                    Rect::new(r.x, r.y + seam, r.width, 1),
                );
            }
            if take == 0 {
                continue;
            }
            let area = Rect::new(r.x + (r.width - self.width) / 2, r.y + y, self.width, take);
            if let Some(p) = self.pages.get(n) {
                if let Some(cache) = &mut sixel {
                    let (w, h) = p.dimensions();
                    let cw = self.crop.span(w);
                    let x = (w - cw) * u32::from(self.crop.x) / 255;
                    let top = (u64::from(h) * u64::from(skip) / u64::from(rows))
                        .min(u64::from(h - 1)) as u32;
                    let end = (u64::from(h) * u64::from(skip + u32::from(take)))
                        .div_ceil(u64::from(rows))
                        .min(u64::from(h)) as u32;
                    let font = self.picker.font_size();
                    let full_width = (u64::from(self.width) * u64::from(font.width) * u64::from(w)
                        / u64::from(cw))
                    .max(1) as u32;
                    let left = (full_width - u32::from(self.width) * u32::from(font.width))
                        * u32::from(self.crop.x)
                        / 255;
                    cache.set_canvas(Some((
                        full_width,
                        rows * u32::from(font.height),
                        left,
                        skip * u32::from(font.height),
                    )));
                    cache.draw(
                        p,
                        f,
                        area,
                        self.picker,
                        [
                            x as f32 / w as f32,
                            top as f32 / h as f32,
                            (x + cw) as f32 / w as f32,
                            end as f32 / h as f32,
                        ],
                    );
                    cache.set_canvas(None);
                } else {
                    p.draw_strip(f, area, self.picker, self.crop, skip, rows);
                }
            } else if let Some(error) = self.pages.error(n, self.side) {
                super::pages::draw_error(f, area, n, error);
            } else if let Some(cache) = &mut sixel {
                cache.pending();
            }
        }
    }
}
impl Picture {
    pub(crate) fn strip_rows(&self, width: u16, picker: &Picker, crop: Crop) -> u32 {
        let (w, h) = self.dimensions();
        let cell = picker.font_size();
        ((u64::from(h)
            * u64::from(width)
            * u64::from(cell.width.max(1))
            * u64::from(crop.percent.max(100)))
        .div_ceil(100 * u64::from(w.max(1)) * u64::from(cell.height.max(1))))
        .clamp(1, 32_000) as u32
    }
}
