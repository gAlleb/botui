//! Page windows and highlights use the same source-pixel coordinates.
use super::*;
impl Picture {
    pub(crate) fn scaled_window(&self, window: [f32; 4], size: Size, picker: &Picker) -> Self {
        let (w, h) = self.dimensions();
        let x = (window[0] * w as f32).floor().max(0.) as u32;
        let y = (window[1] * h as f32).floor().max(0.) as u32;
        let right = (window[2] * w as f32).ceil().min(w as f32) as u32;
        let bottom = (window[3] * h as f32).ceil().min(h as f32) as u32;
        let font = picker.font_size();
        let width = u32::from(size.width) * u32::from(font.width.max(1));
        let height = u32::from(size.height) * u32::from(font.height.max(1));
        let image = self
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
            );
        Self::new(image)
    }

    /// Tint in page coordinates before the normal scroll, zoom or spread transform.
    pub(crate) fn marked(&self, marks: &[([f32; 4], [u8; 3])]) -> Self {
        let mut image = self.source.to_rgba8();
        let (w, h) = image.dimensions();
        for (r, color) in marks {
            let x = |v: f32| (v.clamp(0., 1.) * w as f32).round() as u32;
            let y = |v: f32| (v.clamp(0., 1.) * h as f32).round() as u32;
            for yy in y(r[1])..y(r[3]) {
                for xx in x(r[0])..x(r[2]) {
                    let p = image.get_pixel_mut(xx, yy);
                    let ink = u32::from(p[3]) * 3;
                    let alpha = ink + 510;
                    for c in 0..3 {
                        p[c] = ((u32::from(p[c]) * ink + u32::from(color[c]) * 510) / alpha) as u8;
                    }
                    p[3] = (alpha / 5) as u8;
                }
            }
        }
        Self::new(DynamicImage::ImageRgba8(image))
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
        let patch = original.marked(&[([0., 0., 0.5, 0.5], [255, 200, 0])]);
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
    #[test]
    fn selecting_a_cropped_page_keeps_every_ink_edge_at_the_same_pixel() {
        let raster = image::RgbImage::from_fn(503, 709, |x, y| {
            let shade = if x % 11 < 3 || y % 17 < 4 { 0 } else { 255 };
            image::Rgb([shade; 3])
        });
        let original = Picture::new(DynamicImage::ImageRgb8(raster));
        let marked = original.marked(&[([0.25, 0.25, 0.75, 0.75], [0; 3])]);
        let picker = Picker::halfblocks();
        for window in [[0., 0., 1., 1.], [0.11, 0.17, 0.89, 0.73]] {
            let size = Size::new(37, 19);
            let view = |p: &Picture| p.scaled_window(window, size, &picker).source.to_rgb8();
            let normal = view(&original);
            let selected = view(&marked);
            assert_eq!(normal.dimensions(), selected.dimensions());
            for (x, y, sample) in normal.enumerate_pixels() {
                let u = window[0] + (window[2] - window[0]) * x as f32 / normal.width() as f32;
                let v = window[1] + (window[3] - window[1]) * y as f32 / normal.height() as f32;
                let after = selected.get_pixel(x, y)[0];
                // Allow for the resize filter straddling the selection rectangle.
                if (0.27..0.73).contains(&u) && (0.27..0.73).contains(&v) {
                    let tinted = (u16::from(sample[0]) * 3 / 5) as u8;
                    assert!(tinted.abs_diff(after) <= 2, "shifted ink at ({x}, {y})");
                } else if !(0.23..0.77).contains(&u) || !(0.23..0.77).contains(&v) {
                    assert_eq!(sample[0], after);
                }
            }
        }
    }
}
