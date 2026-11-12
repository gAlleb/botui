//! Sixel encoding runs off the input thread. Kitty and half-block paths do not use this cache.
use crate::{
    cover::Picture,
    shelf::background::{Ticket, Worker},
};
use lru::LruCache;
use ratatui::{
    Frame,
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
}
impl Cache {
    pub fn new() -> Self {
        Self {
            ready: LruCache::new(NonZeroUsize::new(12).unwrap()),
            lanes: std::array::from_fn(|_| Lane {
                worker: Worker::new(),
                job: None,
            }),
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
        match self
            .ready
            .get(&Self::key(picture, area.as_size(), picker, window))
        {
            Some(Ok(protocol)) => f.render_widget(Image::new(protocol), area),
            Some(Err(error)) => super::pages::draw_error(f, area, 0, error),
            None => f.render_widget(
                ratatui::widgets::Paragraph::new("Rendering page…").centered(),
                crate::centered(area, area.width.min(24), 1),
            ),
        }
    }
}
