//! Keep a spread and its neighbours; the next request always favours visible pages.
use crate::{
    cover::Picture,
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
pub(super) struct Pages {
    appearance: ScanAppearance,
    colors: Option<Paper>,
    lanes: [Lane; 2],
    last: u32,
    backward: bool,
    cache: LruCache<Key, Result<Picture, String>>,
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
    pub fn get(&self, page: u32, side: u16) -> Option<&Picture> {
        self.cache.peek(&(page, side))?.as_ref().ok()
    }
    pub fn error(&self, page: u32, side: u16) -> Option<&str> {
        self.cache
            .peek(&(page, side))?
            .as_ref()
            .err()
            .map(String::as_str)
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
                self.cache.put(*k, result);
                lane.job = None;
            }
        }
    }
    pub fn update(&mut self, first: u32, count: u32, cols: u16, side: u16) {
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
        self.cache
            .resize(NonZeroUsize::new(wanted.len() + 1).unwrap());
        for &i in wanted.iter().rev() {
            self.cache.promote(&(i, side));
        }
        let missing: Vec<_> = wanted
            .iter()
            .copied()
            .filter(|n| !self.cache.contains(&(*n, side)))
            .take(self.lanes.len())
            .collect();
        for lane in &mut self.lanes {
            if lane
                .job
                .as_ref()
                .is_some_and(|((p, s), _)| *s != side || !missing.contains(p))
            {
                lane.job = None;
            }
        }
        for i in missing {
            if self
                .lanes
                .iter()
                .any(|l| l.job.as_ref().is_some_and(|(k, _)| *k == (i, side)))
            {
                continue;
            }
            let lane = self.lanes.iter_mut().find(|l| l.job.is_none()).unwrap();
            let renderer = lane.renderer.clone();
            let appearance = self.appearance;
            let colors = self.colors;
            lane.job = Some((
                (i, side),
                lane.worker.start(move |cancel| {
                    let image = renderer
                        .lock()
                        .map_err(|_| io::Error::other("Page worker interrupted"))?
                        .render(i, side, cancel)?;
                    Ok(Picture::new(apply_appearance(image, appearance, colors)))
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
