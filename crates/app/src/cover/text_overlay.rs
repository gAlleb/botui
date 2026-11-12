//! Small raster patches keep PDF highlights out of the page renderer and its cache.
use super::*;
impl Picture {
    pub(crate) fn window_encoder(
        &self,
        size: Size,
        picker: Picker,
        window: [f32; 4],
    ) -> impl FnOnce() -> Result<Protocol, String> + Send + 'static {
        let source = self.source.clone();
        move || {
            let patch = Picture::new(source).text_patch(window, size, &picker, &[]);
            let mut protocol = picker
                .new_protocol(
                    patch.source,
                    size,
                    Resize::Scale(Some(image::imageops::FilterType::Triangle)),
                )
                .map_err(|e| e.to_string())?;
            if let Protocol::Sixel(sixel) = &mut protocol {
                keep_sixel_in_viewport(sixel);
            }
            Ok(protocol)
        }
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

fn keep_sixel_in_viewport(sixel: &mut ratatui_image::protocol::sixel::Sixel) {
    use std::fmt::Write;
    let (start, escape, _) =
        ratatui_image::picker::cap_parser::Parser::tmux_start_escape_end(sixel.is_tmux);
    let (width, height) = (sixel.size.width, sixel.size.height);
    if height > 1 {
        // The library clears h rows using h cursor-down commands. At the
        // bottom edge the last move clamps, so moving h rows back lands one
        // row too high. Only travel between the rows that actually exist.
        let mut old = String::new();
        let mut clear = String::new();
        for row in 0..height {
            write!(old, "{escape}[{width}X{escape}[1B").unwrap();
            write!(clear, "{escape}[{width}X").unwrap();
            if row + 1 < height {
                write!(clear, "{escape}[1B").unwrap();
            }
        }
        write!(old, "{escape}[{height}A").unwrap();
        write!(clear, "{escape}[{}A", height - 1).unwrap();
        if sixel.data[start.len()..].starts_with(&old) {
            sixel
                .data
                .replace_range(start.len()..start.len() + old.len(), &clear);
        }
    }
    // The encoder ends its last band with "$-". Both commands are unnecessary.
    // In Foot, the carriage return leaves the column at zero and disables
    // trimming of the unused pixels in the final six-pixel band. Rounding the
    // image up by 1–5 pixels then scrolls a full-height viewport by one cell.
    if let Some(end) = sixel.data.rfind(&format!("{escape}\\")) {
        let content = &sixel.data[..end];
        let trimmed = content.trim_end_matches(['$', '-']).len();
        sixel.data.replace_range(trimmed..end, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_height_sixel_does_not_advance_below_its_last_row() {
        for tmux in [false, true] {
            for height in [1, 2, 7] {
                let mut sixel = ratatui_image::protocol::sixel::Sixel::new(
                    DynamicImage::new_rgb8(40, u32::from(height) * 19),
                    Size::new(4, height),
                    tmux,
                )
                .unwrap();
                let (start, escape, _) =
                    ratatui_image::picker::cap_parser::Parser::tmux_start_escape_end(tmux);
                let begin = sixel.data[start.len()..]
                    .find(&format!("{escape}P"))
                    .unwrap()
                    + start.len();
                let end = sixel.data.rfind(&format!("{escape}\\")).unwrap();
                let pixels = sixel.data[begin..end]
                    .strip_suffix("$-")
                    .unwrap()
                    .to_owned();
                keep_sixel_in_viewport(&mut sixel);
                let begin = sixel.data[start.len()..]
                    .find(&format!("{escape}P"))
                    .unwrap()
                    + start.len();
                let end = sixel.data.rfind(&format!("{escape}\\")).unwrap();
                assert_eq!(
                    sixel.data[begin..end],
                    pixels,
                    "Encoded pixels must stay unchanged"
                );
                let clear = &sixel.data[start.len()..begin];
                assert_eq!(
                    clear.matches(&format!("{escape}[4X")).count(),
                    usize::from(height)
                );
                assert_eq!(
                    clear.matches(&format!("{escape}[1B")).count(),
                    usize::from(height - 1)
                );
            }
        }
    }
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
