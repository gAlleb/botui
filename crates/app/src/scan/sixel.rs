//! Sixel encoding runs off the input thread. Kitty and half-block paths do not use this cache.
use crate::{
    cover::Picture,
    shelf::background::{Ticket, Worker},
};
use lru::LruCache;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Rect, Size},
};
use ratatui_image::{Image, picker::Picker, protocol::Protocol};
use std::{num::NonZeroUsize, sync::mpsc::TryRecvError};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    image: u64,
    width: u16,
    height: u16,
    font: (u16, u16),
    window: [u32; 4],
}
struct Lane {
    worker: Worker,
    job: Option<(Key, Ticket<Result<Protocol, String>>)>,
}
pub(super) struct Cache {
    ready: LruCache<Key, Result<Protocol, String>>,
    lanes: [Lane; 2],
    requested: Vec<(Key, Rect)>,
    displayed: Option<(Vec<(Key, Rect)>, Buffer)>,
    complete: bool,
}
impl Cache {
    pub fn new() -> Self {
        Self {
            ready: LruCache::new(NonZeroUsize::new(12).unwrap()),
            lanes: std::array::from_fn(|_| Lane {
                worker: Worker::new(),
                job: None,
            }),
            requested: Vec::new(),
            displayed: None,
            complete: false,
        }
    }
    pub fn poll(&mut self) {
        for lane in &mut self.lanes {
            if let Some((key, ticket)) = &lane.job {
                let result = match ticket.receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => {
                        Some(Err("Sixel encoding interrupted".into()))
                    }
                };
                if let Some(result) = result {
                    self.ready.put(*key, result);
                    lane.job = None;
                }
            }
        }
    }
    pub fn busy(&self) -> bool {
        self.lanes.iter().any(|l| l.job.is_some())
    }
    pub fn begin_frame(&mut self) {
        self.requested.clear();
        self.complete = true;
    }
    pub fn pending(&mut self) {
        self.complete = false;
    }
    /// Keep the last complete canvas at its old coordinates until every visible
    /// slice is encoded. Never put an old protocol into a new-sized rectangle.
    /// Capture before selections and dialogs, so overlays remain independent.
    pub fn finish_frame(&mut self, f: &mut Frame, area: Rect) -> bool {
        if self.displayed.as_ref().is_some_and(|(_, b)| b.area != area) {
            self.displayed = None;
        }
        if self.complete && self.requested.is_empty() {
            self.displayed = None;
            return true;
        }
        if self.complete {
            if self
                .displayed
                .as_ref()
                .is_none_or(|(keys, _)| *keys != self.requested)
            {
                let mut canvas = Buffer::empty(area);
                for pos in area.positions() {
                    canvas[pos] = f.buffer_mut()[pos].clone();
                }
                self.displayed = Some((self.requested.clone(), canvas));
            }
            return true;
        }
        if let Some((_, canvas)) = &self.displayed {
            for pos in area.positions() {
                f.buffer_mut()[pos] = canvas[pos].clone();
            }
        }
        false
    }
    pub fn has_frame(&self) -> bool {
        self.displayed.is_some()
    }
    fn key(picture: &Picture, size: Size, picker: &Picker, window: [f32; 4]) -> Key {
        let font = picker.font_size();
        Key {
            image: picture.identity,
            width: size.width,
            height: size.height,
            font: (font.width, font.height),
            window: window.map(f32::to_bits),
        }
    }
    pub fn prepare(&mut self, picture: &Picture, size: Size, picker: &Picker, window: [f32; 4]) {
        let key = Self::key(picture, size, picker, window);
        if size.width == 0
            || size.height == 0
            || self.ready.contains(&key)
            || self
                .lanes
                .iter()
                .any(|l| l.job.as_ref().is_some_and(|(k, _)| *k == key))
        {
            return;
        }
        if let Some(lane) = self.lanes.iter_mut().find(|l| l.job.is_none()) {
            let encode = picture.window_encoder(size, picker.clone(), window);
            lane.job = Some((key, lane.worker.start(move |_| encode())));
        }
    }
    pub fn draw(
        &mut self,
        picture: &Picture,
        f: &mut Frame,
        area: Rect,
        picker: &Picker,
        window: [f32; 4],
    ) {
        self.prepare(picture, area.as_size(), picker, window);
        let key = Self::key(picture, area.as_size(), picker, window);
        self.requested.push((key, area));
        match self.ready.get(&key) {
            Some(Ok(protocol)) => f.render_widget(Image::new(protocol), area),
            Some(Err(error)) => super::pages::draw_error(f, area, 0, error),
            None => {
                self.complete = false;
                f.render_widget(
                    ratatui::widgets::Paragraph::new("Rendering page…").centered(),
                    crate::centered(area, area.width.min(24), 1),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use ratatui_image::picker::ProtocolType;

    #[test]
    fn pending_zoom_keeps_both_pages_at_the_old_coordinates_until_complete() {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Sixel);
        let pictures = [40, 180].map(|grey| {
            Picture::new(image::RgbImage::from_pixel(8, 16, image::Rgb([grey; 3])).into())
        });
        let window = [0., 0., 1., 1.];
        let old = [Rect::new(2, 1, 4, 6), Rect::new(6, 1, 4, 6)];
        let zoom = [Rect::new(0, 0, 6, 8), Rect::new(6, 0, 6, 8)];
        let mut cache = Cache::new();
        let prepare = |cache: &mut Cache, n: usize, area: Rect| {
            let key = Cache::key(&pictures[n], area.as_size(), &picker, window);
            cache.ready.put(
                key,
                pictures[n].window_encoder(area.as_size(), picker.clone(), window)(),
            );
        };
        let mut term = Terminal::new(TestBackend::new(12, 8)).unwrap();
        let draw = |cache: &mut Cache, term: &mut Terminal<TestBackend>, areas: [Rect; 2]| {
            let mut current = false;
            term.draw(|f| {
                cache.begin_frame();
                for i in 0..2 {
                    cache.draw(&pictures[i], f, areas[i], &picker, window);
                }
                current = cache.finish_frame(f, f.area());
            })
            .unwrap();
            current
        };
        for (i, area) in old.into_iter().enumerate() {
            prepare(&mut cache, i, area);
        }
        assert!(draw(&mut cache, &mut term, old));
        let previous = term.backend().buffer().clone();
        // One encoded half must not move or erase the other half of a spread.
        prepare(&mut cache, 0, zoom[0]);
        assert!(!draw(&mut cache, &mut term, zoom));
        assert_eq!(*term.backend().buffer(), previous);
        prepare(&mut cache, 1, zoom[1]);
        assert!(draw(&mut cache, &mut term, zoom));
        assert_ne!(*term.backend().buffer(), previous);
        let enlarged = term.backend().buffer().clone();
        cache.ready.clear();
        assert!(!draw(&mut cache, &mut term, old));
        assert_eq!(*term.backend().buffer(), enlarged);
        // The previous canvas must never overwrite a resized reading pane.
        term.draw(|f| {
            cache.begin_frame();
            cache.pending();
            assert!(!cache.finish_frame(f, Rect::new(1, 0, 11, 8)));
            assert!(!cache.has_frame());
        })
        .unwrap();
    }
}
