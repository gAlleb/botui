//! Keep a spread and its neighbours; the next request always favours visible pages.
use crate::{
    cover::{Crop, Picture},
    shelf::background::{Ticket, Worker},
};
use botui_store::ScanAppearance;
use image::DynamicImage;
use lru::LruCache;
use std::{io, num::NonZeroUsize, sync::mpsc::TryRecvError};
type Key = (u32, u16);
use super::paper::Paper;
struct Lane {
    renderer: std::sync::Arc<std::sync::Mutex<botui_fixed::PageReader>>,
    worker: Worker,
    job: Option<(Key, Ticket<io::Result<Picture>>)>,
}
#[derive(Default)]
struct CachedPage {
    // Keep one raster per page, including while a sharper version is rendering.
    // Downscaling reuses it; a failed refinement must not erase readable pixels.
    rendered: Option<(u16, Picture)>,
    failure: Option<(u16, String)>,
}
impl CachedPage {
    fn satisfies(&self, side: u16) -> bool {
        self.rendered.as_ref().is_some_and(|(s, _)| *s >= side)
            || self.failure.as_ref().is_some_and(|(s, _)| *s == side)
    }
    fn finish(&mut self, side: u16, result: Result<Picture, String>) {
        match result {
            Ok(picture) => {
                if self.rendered.as_ref().is_none_or(|(s, _)| side >= *s) {
                    self.rendered = Some((side, picture));
                }
                self.failure = None;
            }
            Err(error) => self.failure = Some((side, error)),
        }
    }
}
pub(super) struct Pages {
    appearance: ScanAppearance,
    colors: Option<Paper>,
    lanes: [Lane; 2],
    last: u32,
    backward: bool,
    cache: LruCache<u32, CachedPage>,
}
impl Pages {
    pub fn new(appearance: ScanAppearance, renderer: impl Fn() -> botui_fixed::PageReader) -> Self {
        Self {
            appearance,
            colors: None,
            lanes: std::array::from_fn(|_| Lane {
                renderer: std::sync::Arc::new(std::sync::Mutex::new(renderer())),
                worker: Worker::new(),
                job: None,
            }),
            last: 0,
            backward: false,
            cache: LruCache::new(NonZeroUsize::new(8).unwrap()),
        }
    }
    pub fn configure(&mut self, appearance: ScanAppearance, colors: Option<Paper>) {
        if self.appearance == appearance && self.colors == colors {
            return;
        }
        self.appearance = appearance;
        self.colors = colors;
        for lane in &mut self.lanes {
            lane.job = None;
        }
        self.cache.clear();
    }
    pub fn busy(&self) -> bool {
        self.lanes.iter().any(|l| l.job.is_some())
    }
    pub fn get(&self, page: u32) -> Option<&Picture> {
        self.cache.peek(&page)?.rendered.as_ref().map(|(_, p)| p)
    }
    pub fn error(&self, page: u32, side: u16) -> Option<&str> {
        self.cache
            .peek(&page)?
            .failure
            .as_ref()
            .filter(|(s, _)| *s == side)
            .map(|(_, e)| e.as_str())
    }
    pub fn poll(&mut self) {
        for lane in &mut self.lanes {
            let Some((k, t)) = &lane.job else { continue };
            let result = match t.receiver.try_recv() {
                Ok(result) => Some(result.map_err(|error| error.to_string())),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("Page rendering interrupted".into())),
            };
            if let Some(result) = result {
                self.cache
                    .get_or_insert_mut(k.0, CachedPage::default)
                    .finish(k.1, result);
                lane.job = None;
            }
        }
    }
    pub fn update(
        &mut self,
        first: u32,
        count: u32,
        cols: u16,
        side: u16,
        strip: Option<(&ratatui_image::picker::Picker, u16, Crop)>,
    ) {
        if first != self.last {
            self.backward = first < self.last;
            self.last = first;
        }
        let end = first.saturating_add(u32::from(cols) - 1).min(count);
        let mut wanted: Vec<u32> = (first..=end).collect();
        // Limit speculative work on very large rasters, never evict a visible slide.
        let radius = if side > 1600 { 2 } else { 3 };
        for d in 1..=radius {
            let left = first.checked_sub(d).filter(|p| *p > 0);
            let right = end.checked_add(d).filter(|p| *p <= count);
            wanted.extend(
                if self.backward {
                    [left, right]
                } else {
                    [right, left]
                }
                .into_iter()
                .flatten(),
            );
        }
        for &i in wanted.iter().rev() {
            self.cache.promote(&i);
        }
        self.cache
            .resize(NonZeroUsize::new(wanted.len() + 1).unwrap());
        let missing: Vec<_> = wanted
            .iter()
            .copied()
            .filter(|n| self.cache.peek(n).is_none_or(|p| !p.satisfies(side)))
            .take(self.lanes.len())
            .collect();
        for lane in &mut self.lanes {
            if lane
                .job
                .as_ref()
                .is_some_and(|((p, s), _)| *s < side || !missing.contains(p))
            {
                lane.job = None;
            }
        }
        for i in missing {
            if self.lanes.iter().any(|l| {
                l.job
                    .as_ref()
                    .is_some_and(|((p, s), _)| *p == i && *s >= side)
            }) {
                continue;
            }
            let lane = self.lanes.iter_mut().find(|l| l.job.is_none()).unwrap();
            let renderer = lane.renderer.clone();
            let appearance = self.appearance;
            let colors = self.colors;
            let strip = strip.map(|(picker, width, crop)| (picker.clone(), width, crop));
            lane.job = Some((
                (i, side),
                lane.worker.start(move |cancel| {
                    let image = renderer
                        .lock()
                        .map_err(|_| io::Error::other("Page worker interrupted"))?
                        .render(i, side, cancel)?;
                    let picture = Picture::new(apply_appearance(image, appearance, colors));
                    if let Some((picker, width, crop)) = strip {
                        picture.prepare_scan_strip(&picker, width, crop);
                    }
                    Ok(picture)
                }),
            ));
        }
    }
}

fn apply_appearance(
    image: DynamicImage,
    appearance: ScanAppearance,
    colors: Option<Paper>,
) -> DynamicImage {
    let mut image = match appearance.quarter_turns % 4 {
        1 => image.rotate90(),
        2 => image.rotate180(),
        3 => image.rotate270(),
        _ => image,
    };
    if let Some(paper) = colors {
        return paper.apply(image);
    }
    if appearance.inverted {
        image.invert();
    }
    image
}

/// Draw inside the failed page's own slot. A damaged sheet must not close the book
/// or cover its readable neighbor in a spread.
pub(super) fn draw_error(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    page: u32,
    error: &str,
) {
    use ratatui::widgets::{Paragraph, Wrap};
    let block = crate::centered(area, area.width.min(64), area.height.min(5));
    f.render_widget(
        Paragraph::new(format!("Page {page} unavailable\n{error}\nP · Go to page"))
            .centered()
            .wrap(Wrap { trim: true }),
        block,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    fn pages() -> Pages {
        Pages::new(Default::default(), || {
            botui_fixed::PageReader::new(
                std::path::Path::new("unused.pdf"),
                botui_formats::FileKind::Pdf,
            )
        })
    }
    fn finish(pages: &mut Pages) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while pages.busy() {
            pages.poll();
            assert!(
                std::time::Instant::now() < deadline,
                "Worker did not finish"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn zoom_reuses_preview_then_larger_raster_without_duplicate_cache_entries() {
        let mut pages = pages();
        pages
            .cache
            .get_or_insert_mut(1, CachedPage::default)
            .finish(800, Ok(Picture::new(DynamicImage::new_rgb8(400, 800))));
        let (release, wait) = std::sync::mpsc::channel();
        pages.lanes[0].job = Some((
            (1, 1600),
            pages.lanes[0].worker.start(move |_| {
                wait.recv().unwrap();
                Ok(Picture::new(DynamicImage::new_rgb8(800, 1600)))
            }),
        ));
        // A zoom change while refining can use both the old page and the pending
        // sharper render. Neither should be discarded for an exact-size key.
        pages.update(1, 1, 1, 1200, None);
        assert_eq!(pages.get(1).unwrap().dimensions(), (400, 800));
        assert_eq!(pages.lanes[0].job.as_ref().unwrap().0, (1, 1600));
        release.send(()).unwrap();
        finish(&mut pages);
        for side in [800, 1200, 1600] {
            pages.update(1, 1, 1, side, None);
            assert!(
                !pages.busy(),
                "Cached resolution started another PDF render"
            );
            assert_eq!(pages.get(1).unwrap().dimensions(), (800, 1600));
            assert_eq!(pages.cache.len(), 1);
        }
        pages.configure(
            ScanAppearance {
                inverted: true,
                ..Default::default()
            },
            None,
        );
        assert!(
            pages.get(1).is_none(),
            "Old colors survived an appearance change"
        );
    }

    #[test]
    fn failed_refinement_keeps_readable_page_and_does_not_retry_every_frame() {
        let mut pages = pages();
        pages
            .cache
            .get_or_insert_mut(1, CachedPage::default)
            .finish(800, Ok(Picture::new(DynamicImage::new_rgb8(400, 800))));
        pages.lanes[0].job = Some((
            (1, 1600),
            pages.lanes[0]
                .worker
                .start(|_| Err(io::Error::other("Rasterizer failed"))),
        ));
        finish(&mut pages);
        pages.update(1, 1, 1, 1600, None);
        assert_eq!(pages.get(1).unwrap().dimensions(), (400, 800));
        assert_eq!(pages.error(1, 1600), Some("Rasterizer failed"));
        assert!(!pages.busy());
        pages.update(1, 1, 1, 800, None);
        assert!(!pages.busy());
        assert!(pages.error(1, 800).is_none());
    }

    #[test]
    fn strip_crosses_mixed_page_sizes_one_row_at_a_time_and_marks_the_whole_pane() {
        use crate::scan::flow::Viewport;
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};
        use ratatui_image::picker::Picker;
        let mut pages = Pages::new(Default::default(), || {
            botui_fixed::PageReader::new(
                std::path::Path::new("unused.pdf"),
                botui_formats::FileKind::Pdf,
            )
        });
        for (n, height) in [(1, 180), (2, 120), (3, 240)] {
            pages
                .cache
                .get_or_insert_mut(n, CachedPage::default)
                .finish(240, Ok(Picture::new(DynamicImage::new_rgb8(120, height))));
        }
        let picker = Picker::halfblocks();
        let view = Viewport {
            side: 240,
            width: 20,
            height: 8,
            picker: &picker,
            crop: Crop::default(),
            count: 3,
        };
        let strip = view.strip(&pages);
        let (mut n, mut fraction) = (1, 0);
        // Pages occupy 15, 10 and 20 rows, with one seam row between them.
        for absolute in 1..=30 {
            strip.shift(&mut n, &mut fraction, 1);
            let (page, skip, _, _, _) = strip.parts(n, fraction).next().unwrap();
            let start = match page {
                1 => 0,
                2 => 16,
                _ => 27,
            };
            assert_eq!(start + skip, absolute);
        }
        for absolute in (0..30).rev() {
            strip.shift(&mut n, &mut fraction, -1);
            let (page, skip, _, _, _) = strip.parts(n, fraction).next().unwrap();
            let start = match page {
                1 => 0,
                2 => 16,
                _ => 27,
            };
            assert_eq!(start + skip, absolute);
        }
        strip.shift(&mut n, &mut fraction, 10);
        let mut term = Terminal::new(TestBackend::new(34, 8)).unwrap();
        term.draw(|f| strip.draw(f, Rect::new(0, 0, 34, 8), n, fraction, None))
            .unwrap();
        for x in 0..34 {
            assert_eq!(term.backend().buffer()[(x, 5)].symbol(), "·");
        }
    }

    #[test]
    fn rotation_moves_pixels_before_inversion() {
        let pixels =
            ImageBuffer::<Luma<u8>, _>::from_raw(2, 3, vec![0, 1, 20, 21, 40, 41]).unwrap();
        let image = apply_appearance(
            DynamicImage::ImageLuma8(pixels),
            ScanAppearance {
                quarter_turns: 1,
                inverted: true,
                ..Default::default()
            },
            None,
        )
        .to_luma8();
        assert_eq!(image.dimensions(), (3, 2));
        assert_eq!(image.into_raw(), [215, 235, 255, 214, 234, 254]);
    }
}
