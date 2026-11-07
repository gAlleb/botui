//! Keep a spread and its neighbours; the next request always favours visible pages.
use crate::{
    cover::Picture,
    shelf::background::{Ticket, Worker},
};
use botui_formats::FileKind;
use botui_store::ScanAppearance;
use image::DynamicImage;
use lru::LruCache;
use std::{io, num::NonZeroUsize, path::Path, sync::mpsc::TryRecvError};
type Key = (u32, u16);
pub(super) struct Pages {
    appearance: ScanAppearance,
    worker: Worker,
    job: Option<(Key, Ticket<io::Result<Picture>>)>,
    cache: LruCache<Key, Result<Picture, String>>,
}
impl Pages {
    pub fn new(appearance: ScanAppearance) -> Self {
        Self {
            appearance,
            worker: Worker::new(),
            job: None,
            cache: LruCache::new(NonZeroUsize::new(4).unwrap()),
        }
    }
    pub fn set_appearance(&mut self, appearance: ScanAppearance) {
        self.appearance = appearance;
        self.job = None;
        self.cache.clear();
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
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
        if let Some((k, t)) = &self.job {
            let result = match t.receiver.try_recv() {
                Ok(result) => Some(result.map_err(|error| error.to_string())),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("Page rendering interrupted".into())),
            };
            if let Some(result) = result {
                self.cache.put(*k, result);
                self.job = None;
            }
        }
    }
    pub fn update(
        &mut self,
        path: &Path,
        kind: FileKind,
        first: u32,
        count: u32,
        cols: u16,
        side: u16,
    ) {
        let visible_end = first.saturating_add(u32::from(cols) - 1).min(count);
        let end = visible_end.saturating_add(2).min(count);
        let capacity = NonZeroUsize::new(usize::from(cols) + 2).unwrap();
        if self.cache.cap() != capacity {
            self.cache.resize(capacity);
        }
        let relevant = |k: Key| k.1 == side && (first..=end).contains(&k.0);
        if self.job.as_ref().is_some_and(|(k, _)| !relevant(*k)) {
            self.job = None;
        }
        for i in first..=visible_end {
            self.cache.promote(&(i, side));
        }
        if self.job.is_none()
            && let Some(i) = (first..=end).find(|i| !self.cache.contains(&(*i, side)))
        {
            let path = path.to_owned();
            let appearance = self.appearance;
            self.job = Some((
                (i, side),
                self.worker.start(move |cancel| {
                    let image = botui_fixed::page(&path, kind, i, side, cancel)?;
                    Ok(Picture::new(apply_appearance(image, appearance)))
                }),
            ));
        }
    }
}

fn apply_appearance(image: DynamicImage, appearance: ScanAppearance) -> DynamicImage {
    let mut image = match appearance.quarter_turns % 4 {
        1 => image.rotate90(),
        2 => image.rotate180(),
        3 => image.rotate270(),
        _ => image,
    };
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
            },
        )
        .to_luma8();
        assert_eq!(image.dimensions(), (3, 2));
        assert_eq!(image.into_raw(), [215, 235, 255, 214, 234, 254]);
    }
}
