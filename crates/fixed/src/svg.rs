//! Rasterize visible SVGs on the existing image workers. Never load files named
//! by an SVG: formats has already embedded allowed EPUB-local resources.
use image::{DynamicImage, RgbaImage};
use resvg::{tiny_skia, usvg};
use std::sync::{Arc, OnceLock};

pub fn render(bytes: &[u8], width: u32, height: u32) -> Option<DynamicImage> {
    if bytes.len() > 32 << 20 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    if !text.contains("<svg") {
        return None;
    }
    let mut options = usvg::Options {
        font_family: "DejaVu Sans".into(),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_string: Box::new(|_, _| None),
            resolve_data: Box::new(|mime, data, options| {
                if data.len() > 16 << 20 {
                    return None;
                }
                if let Ok(reader) = image::ImageReader::new(std::io::Cursor::new(data.as_slice()))
                    .with_guessed_format()
                    && reader.format().is_some()
                {
                    let (w, h) = reader.into_dimensions().ok()?;
                    if w > 4096 || h > 4096 || u64::from(w) * u64::from(h) * 4 > 32 << 20 {
                        return None;
                    }
                }
                usvg::ImageHrefResolver::default_data_resolver()(mime, data, options)
            }),
        },
        ..Default::default()
    };
    if text.contains("<text") {
        options.fontdb = fonts().clone();
        let fallback = usvg::FontResolver::default_fallback_selector();
        let attempts = std::sync::atomic::AtomicUsize::new(0);
        options.font_resolver.select_fallback = Box::new(move |ch, excluded, db| {
            if let Some(id) = fallback(ch, excluded, db) {
                return Some(id);
            }
            if attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 16 {
                return None;
            }
            if let Ok(output) = std::process::Command::new("fc-match")
                .args([
                    "-f",
                    "%{file}\n",
                    &format!("sans-serif:charset={:x}", ch as u32),
                ])
                .output()
                && output.status.success()
                && let Some(path) = String::from_utf8_lossy(&output.stdout).lines().next()
            {
                let _ = Arc::make_mut(db).load_font_file(path);
            }
            fallback(ch, excluded, db)
        });
    }
    let tree = usvg::Tree::from_data(bytes, &options).ok()?;
    let size = tree.size();
    let scale = (width as f32 / size.width()).min(height as f32 / size.height());
    let mut pixmap = tiny_skia::Pixmap::new(
        (size.width() * scale).ceil().max(1.) as u32,
        (size.height() * scale).ceil().max(1.) as u32,
    )?;
    // EPUB diagrams commonly assume paper and use black paths on an otherwise
    // transparent canvas. Keep their ink legible on dark reader/terminal themes.
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia uses premultiplied RGBA; image/terminal protocols expect straight alpha.
    let pixels: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let p = p.demultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        })
        .collect();
    RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixels).map(DynamicImage::ImageRgba8)
}
fn fonts() -> &'static Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}\n", "DejaVu Sans"])
            .output()
            && output.status.success()
        {
            for path in String::from_utf8_lossy(&output.stdout).lines().take(1) {
                let _ = db.load_font_file(path);
            }
        }
        if db.is_empty() {
            db.load_system_fonts();
        }
        db.set_sans_serif_family("DejaVu Sans");
        Arc::new(db)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vectors_keep_aspect_ratio_and_legible_translucent_ink() {
        let data=br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><rect width="100" height="50" fill="#ff0000" fill-opacity=".5"/></svg>"##;
        let image = render(data, 200, 200).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), (200, 100));
        assert_eq!(image.get_pixel(100, 50).0, [255, 127, 127, 255]);
    }
    #[test]
    fn svg_cannot_read_host_images() {
        let path = tempfile::NamedTempFile::with_suffix(".png").unwrap();
        RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]))
            .save(path.path())
            .unwrap();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><image width="10" height="10" xlink:href="{}"/></svg>"#,
            path.path().display()
        );
        assert!(
            render(svg.as_bytes(), 10, 10)
                .unwrap()
                .into_rgba8()
                .pixels()
                .all(|p| p.0 == [255, 255, 255, 255])
        );
    }
    #[test]
    fn embedded_raster_and_vector_overlay_both_render() {
        // A real 4x4 red PNG. SVG cover wrappers must not silently turn white
        // when resvg is built without its raster-images feature.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 20 10"><image width="20" height="10" preserveAspectRatio="none" xlink:href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAIAAAAmkwkpAAAAEElEQVR4nGP8z4AATAxEcQAz0QEHOoQ+uAAAAABJRU5ErkJggg=="/><path d="M10 0H20V10H10Z" fill="#0000ff"/></svg>"##;
        let pixels = render(svg, 200, 100).unwrap().into_rgba8();
        assert_eq!(pixels.get_pixel(25, 50).0, [255, 0, 0, 255]);
        assert_eq!(pixels.get_pixel(175, 50).0, [0, 0, 255, 255]);
    }
}
