//! Small raster patches keep PDF highlights out of the page renderer and its cache.
use super::*;
impl Picture {
    pub(crate) fn page_mapping(&self, area: Rect, crop: Crop) -> Option<(Rect, [f32; 4])> {
        let ready = self.ready.borrow();
        let (w, h) = self.dimensions();
        let cw = crop.span(w);
        let ch = crop.span(h);
        let size = if monochrome() {
            let scale = (f32::from(area.width) / cw as f32)
                .min(f32::from(area.height) * 2. / ch as f32)
                .min(1.);
            Size::new(
                (cw as f32 * scale).round() as u16,
                ((ch as f32 * scale).round() as u16).div_ceil(2),
            )
        } else {
            ready
                .iter()
                .find(|(r, _)| r.size == area.as_size() && r.crop == crop)?
                .1
                .size()
        };
        let x = (w - cw) * u32::from(crop.x) / 255;
        let y = (h - ch) * u32::from(crop.y) / 255;
        Some((
            crate::centered(area, size.width, size.height),
            [
                x as f32 / w as f32,
                y as f32 / h as f32,
                (x + cw) as f32 / w as f32,
                (y + ch) as f32 / h as f32,
            ],
        ))
    }

    pub(crate) fn text_patch(
        &self,
        window: [f32; 4],
        size: Size,
        picker: &Picker,
        marks: &[([f32; 4], [u8; 3])],
    ) -> Self {
        let (w, h) = self.dimensions();
        let x = (window[0] * w as f32).floor().max(0.) as u32;
        let y = (window[1] * h as f32).floor().max(0.) as u32;
        let right = (window[2] * w as f32).ceil().min(w as f32) as u32;
        let bottom = (window[3] * h as f32).ceil().min(h as f32) as u32;
        let font = picker.font_size();
        let width = u32::from(size.width) * u32::from(font.width.max(1));
        let height = u32::from(size.height) * u32::from(font.height.max(1));
        let mut image = self
            .source
            .crop_imm(
                x.min(w - 1),
                y.min(h - 1),
                (right - x).max(1),
                (bottom - y).max(1),
            )
            .resize_exact(
                width.max(1),
                height.max(1),
                image::imageops::FilterType::Triangle,
            )
            .into_rgb8();
        for (bounds, color) in marks {
            let px = |v: f32| {
                ((v - window[0]) / (window[2] - window[0]) * width as f32).clamp(0., width as f32)
                    as u32
            };
            let py = |v: f32| {
                ((v - window[1]) / (window[3] - window[1]) * height as f32).clamp(0., height as f32)
                    as u32
            };
            for yy in py(bounds[1])..py(bounds[3]) {
                for xx in px(bounds[0])..px(bounds[2]) {
                    let pixel = image.get_pixel_mut(xx, yy);
                    for c in 0..3 {
                        pixel[c] = ((u16::from(pixel[c]) * 3 + u16::from(color[c]) * 2) / 5) as u8;
                    }
                }
            }
        }
        Self::new(DynamicImage::ImageRgb8(image))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn highlight_changes_only_selected_pixels_and_keeps_underlying_ink() {
        let mut raster = image::RgbImage::from_pixel(100, 100, image::Rgb([255, 255, 255]));
        for y in 20..30 {
            for x in 10..40 {
                raster.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
        let original = Picture::new(DynamicImage::ImageRgb8(raster));
        let picker = Picker::halfblocks();
        let patch = original.text_patch(
            [0., 0., 1., 1.],
            Size::new(100, 50),
            &picker,
            &[([0., 0., 0.5, 0.5], [255, 200, 0])],
        );
        let pixels = patch.source.to_rgb8();
        let sample = |x: f32, y: f32| {
            pixels
                .get_pixel(
                    (x * pixels.width() as f32) as u32,
                    (y * pixels.height() as f32) as u32,
                )
                .0
        };
        assert_eq!(sample(0.8, 0.8), [255, 255, 255]);
        assert_eq!(sample(0.1, 0.1), [255, 233, 153]);
        let ink = sample(0.2, 0.25);
        assert!(ink[0] < 120 && ink[1] < 120 && ink[2] < 120);
        assert_eq!(original.source.to_rgb8().get_pixel(20, 25).0, [0, 0, 0]);
    }
}
