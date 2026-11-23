//! Retire only uploads whose last cached rendition has gone away.
use image::DynamicImage;
use ratatui::layout::Size;
use ratatui_image::{
    Resize,
    errors::Errors,
    picker::{Capability, Picker, ProtocolType, cap_parser::Parser},
    protocol::{Protocol, kitty::Kitty},
    sliced::SlicedProtocol,
};
use std::{
    collections::HashMap,
    hash::BuildHasher,
    io::{self, Write},
    ops::Deref,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone)]
pub(crate) struct Managed<T> {
    protocol: T,
    upload: Option<Arc<Upload>>,
}

impl<T> Deref for Managed<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.protocol
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Delete {
    id: u32,
    tmux: bool,
}

struct Queue {
    open: bool,
    pending: Vec<Delete>,
    live: HashMap<u32, bool>,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            open: true,
            pending: Vec::new(),
            live: HashMap::new(),
        }
    }
}

fn queue() -> &'static Arc<Mutex<Queue>> {
    static QUEUE: OnceLock<Arc<Mutex<Queue>>> = OnceLock::new();
    QUEUE.get_or_init(|| Arc::new(Mutex::new(Queue::default())))
}

struct Upload {
    delete: Delete,
    queue: Arc<Mutex<Queue>>,
}

impl Upload {
    fn new(delete: Delete, queue: Arc<Mutex<Queue>>) -> Self {
        {
            let mut state = queue.lock().unwrap_or_else(|p| p.into_inner());
            if state.open {
                state.live.insert(delete.id, delete.tmux);
            }
        }
        Self { delete, queue }
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        let mut queue = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        if queue.open && queue.live.remove(&self.delete.id).is_some() {
            queue.pending.push(self.delete);
        }
    }
}

fn kitty(picker: &Picker, image: DynamicImage, size: Size) -> Result<Managed<Kitty>, Errors> {
    // All application Kitty producers share this allocator. A random starting
    // range avoids predictable clashes with another program; never recycle an ID.
    static NEXT: OnceLock<AtomicU64> = OnceLock::new();
    let next = NEXT.get_or_init(|| {
        let seed = std::collections::hash_map::RandomState::new().hash_one(std::process::id());
        AtomicU64::new((seed & 0x00ff_ffff) + 1)
    });
    let id = u32::try_from(next.fetch_add(1, Ordering::Relaxed))
        .map_err(|_| io::Error::other("Kitty image IDs exhausted"))?;
    let protocol = Kitty::new(
        image,
        size,
        id,
        picker.tmux_detected(),
        picker
            .capabilities()
            .contains(&Capability::KittyCompression),
        // ratatui-image creates SHM during encoding, exposes no ownership guard
        // or name, and leaks it when prefetch is discarded before transmission.
        // Compressed inline transfers need no external resource to clean up.
        false,
    )?;
    Ok(Managed {
        protocol,
        upload: Some(Arc::new(Upload::new(
            Delete {
                id,
                tmux: picker.tmux_detected(),
            },
            Arc::clone(queue()),
        ))),
    })
}

pub(crate) fn new_protocol(
    picker: &Picker,
    image: DynamicImage,
    size: Size,
    resize: Resize,
) -> Result<Managed<Protocol>, Errors> {
    if picker.protocol_type() != ProtocolType::Kitty {
        return picker
            .new_protocol(image, size, resize)
            .map(|protocol| Managed {
                protocol,
                upload: None,
            });
    }
    // Match Picker::new_protocol's resizing. Kitty uses transparent padding;
    // scan paper supplies a background color only for Halfblocks.
    let font = picker.font_size();
    let desired = Resize::natural_size(&image, font);
    let unchanged = !matches!(resize, Resize::Scale(_))
        && desired.width <= size.width
        && desired.height <= size.height
        && (image.width() == u32::from(desired.width) * u32::from(font.width)
            || image.height() == u32::from(desired.height) * u32::from(font.height));
    let (image, size) = if unchanged {
        (image, desired)
    } else {
        let size = resize.size_for(&image, font, size);
        (resize.resize(&image, font, size, None), size)
    };
    let managed = kitty(picker, image, size)?;
    Ok(Managed {
        protocol: Protocol::Kitty(managed.protocol),
        upload: managed.upload,
    })
}

pub(super) fn new_sliced(
    picker: &Picker,
    image: DynamicImage,
    size: Size,
    scan: bool,
) -> Result<Managed<SlicedProtocol>, Errors> {
    let resize = Resize::Scale(Some(image::imageops::FilterType::Triangle));
    if picker.protocol_type() != ProtocolType::Kitty {
        return SlicedProtocol::new_with_resize(picker, image, size, resize).map(|protocol| {
            Managed {
                protocol,
                upload: None,
            }
        });
    }
    let managed = if scan {
        // Kitty preserves the upload's aspect ratio inside a virtual placement.
        // Match the pixel aspect of its whole-cell box, or rounding the page
        // height up to rows leaves a transparent strip below the final pixels.
        let font = picker.font_size();
        let width = u32::from(size.width) * u32::from(font.width.max(1));
        let height = u32::from(size.height) * u32::from(font.height.max(1));
        let (mut gcd, mut remainder) = (width, height);
        while remainder != 0 {
            (gcd, remainder) = (remainder, gcd % remainder);
        }
        let (unit_w, unit_h) = (width / gcd, height / gcd);
        let units = gcd
            .min(image.width() / unit_w)
            .min(image.height() / unit_h)
            .max(1);
        let (w, h) = (unit_w * units, unit_h * units);
        if u64::from(w) * u64::from(h) > 8_000_000 {
            // draw_strip falls back to rendering just the visible window.
            return Err(io::Error::other("Scan placement exceeds the raster budget").into());
        }
        let image = if image.width() == w && image.height() == h {
            image
        } else {
            image.resize_exact(w, h, image::imageops::FilterType::Triangle)
        };
        kitty(picker, image, size)?
    } else {
        let managed = new_protocol(picker, image, size, resize)?;
        let Protocol::Kitty(protocol) = managed.protocol else {
            unreachable!()
        };
        Managed {
            protocol,
            upload: managed.upload,
        }
    };
    Ok(Managed {
        protocol: SlicedProtocol::Kitty(managed.protocol),
        upload: managed.upload,
    })
}

pub(crate) struct Retired(Vec<Delete>);

/// Take before rendering: a rendition dropped during this frame must survive
/// its publication. Worker drops never write to stdout or interleave a transfer.
pub(crate) fn take_retired() -> Retired {
    Retired(std::mem::take(
        &mut queue().lock().unwrap_or_else(|p| p.into_inner()).pending,
    ))
}

impl Retired {
    pub(crate) fn write(&self, out: &mut impl Write) -> io::Result<()> {
        for delete in &self.0 {
            let (start, escape, end) = Parser::tmux_start_escape_end(delete.tmux);
            write!(
                out,
                "{start}{escape}_Ga=d,d=I,i={},q=2{escape}\\{end}",
                delete.id
            )?;
        }
        Ok(())
    }
}

pub(crate) fn open() {
    queue().lock().unwrap_or_else(|p| p.into_inner()).open = true;
}

pub(crate) fn close(out: &mut impl Write) -> io::Result<()> {
    let retired = close_queue(&mut queue().lock().unwrap_or_else(|p| p.into_inner()));
    retired.write(out)?;
    out.flush()
}

fn close_queue(queue: &mut Queue) -> Retired {
    queue.open = false;
    let mut retired = std::mem::take(&mut queue.pending);
    // Thread-local corner caches and outstanding workers can outlive the UI.
    // Delete their own IDs as well, never another program's terminal graphics.
    retired.extend(queue.live.drain().map(|(id, tmux)| Delete { id, tmux }));
    Retired(retired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_last_holder_retires_upload_even_when_worker_drops_it() {
        let queue = Arc::new(Mutex::new(Queue::default()));
        let make = |id| Managed {
            protocol: (),
            upload: Some(Arc::new(Upload::new(
                Delete { id, tmux: false },
                Arc::clone(&queue),
            ))),
        };
        let cached = make(7);
        let current = cached.clone();
        let background = make(8);
        drop(cached);
        assert!(queue.lock().unwrap().pending.is_empty());
        std::thread::spawn(move || drop(current)).join().unwrap();
        assert_eq!(
            queue.lock().unwrap().pending,
            [Delete { id: 7, tmux: false }]
        );
        assert_eq!(background.upload.as_ref().unwrap().delete.id, 8);
        let mut closed = close_queue(&mut queue.lock().unwrap()).0;
        closed.sort_by_key(|d| d.id);
        assert_eq!(
            closed,
            [Delete { id: 7, tmux: false }, Delete { id: 8, tmux: false }]
        );
        drop(background);
        assert!(
            queue.lock().unwrap().pending.is_empty(),
            "late worker drops must not accumulate after terminal shutdown"
        );
    }

    #[test]
    fn deletion_frees_only_its_id_and_preserves_tmux_passthrough() {
        let mut bytes = Vec::new();
        Retired(vec![
            Delete {
                id: 42,
                tmux: false,
            },
            Delete { id: 99, tmux: true },
        ])
        .write(&mut bytes)
        .unwrap();
        assert_eq!(
            bytes,
            b"\x1b_Ga=d,d=I,i=42,q=2\x1b\\\x1bPtmux;\x1b\x1b_Ga=d,d=I,i=99,q=2\x1b\x1b\\\x1b\\"
        );
    }

    #[test]
    fn cloning_preserves_the_protocols_single_transmission() {
        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        let image = new_protocol(
            &picker,
            DynamicImage::new_rgb8(20, 40),
            Size::new(2, 2),
            Resize::Fit(None),
        )
        .unwrap();
        let clone = image.clone();
        let area = Rect::new(0, 0, 2, 2);
        let mut first = Buffer::empty(area);
        ratatui_image::Image::new(&image).render(area, &mut first);
        drop(image);
        let mut again = Buffer::empty(area);
        ratatui_image::Image::new(&clone).render(area, &mut again);
        assert!(first.content.iter().any(|c| c.symbol().contains("\x1b_G")));
        assert!(!again.content.iter().any(|c| c.symbol().contains("\x1b_G")));
        assert_eq!(first[(1, 1)], again[(1, 1)]);
    }

    #[test]
    fn managed_factory_keeps_the_pickers_dimensions_and_raster() {
        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        let payload = |protocol: &Protocol| {
            let size = protocol.size();
            let area = Rect::new(0, 0, size.width, size.height);
            let mut buffer = Buffer::empty(area);
            ratatui_image::Image::new(protocol).render(area, &mut buffer);
            buffer
                .content
                .iter()
                .flat_map(|cell| {
                    cell.symbol().split("\x1b_G").skip(1).filter_map(|chunk| {
                        chunk
                            .split_once(';')
                            .map(|(_, data)| data.split("\x1b\\").next().unwrap())
                    })
                })
                .collect::<String>()
        };
        for (width, height) in [(20, 40), (23, 37), (80, 20)] {
            for bounds in [Size::new(1, 1), Size::new(4, 3)] {
                for resize in [
                    Resize::Fit(None),
                    Resize::Scale(Some(image::imageops::FilterType::Triangle)),
                ] {
                    let source = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(
                        width,
                        height,
                        |x, y| image::Rgba([x as u8, y as u8, 127, 255]),
                    ));
                    let original = picker
                        .new_protocol(source.clone(), bounds, resize.clone())
                        .unwrap();
                    let managed = new_protocol(&picker, source, bounds, resize).unwrap();
                    assert_eq!(managed.size(), original.size());
                    assert_eq!(payload(&managed), payload(&original));
                }
            }
        }
    }

    #[test]
    fn picture_eviction_retires_old_sizes_but_keeps_the_cached_background() {
        use super::super::{ImagePlacement, Picture};
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        let picture = Picture::new(DynamicImage::new_rgb8(50, 80));
        let mut terminal = Terminal::new(TestBackend::new(20, 20)).unwrap();
        let mut ids = Vec::new();
        for height in [10, 12, 14] {
            terminal
                .draw(|frame| {
                    picture.draw_region_mode(
                        frame,
                        Rect::new(0, 0, 20, height),
                        &picker,
                        super::super::Crop::default(),
                        false,
                        ImagePlacement::Fit,
                    )
                })
                .unwrap();
            ids.push(
                picture
                    .ready
                    .borrow()
                    .last()
                    .unwrap()
                    .1
                    .upload
                    .as_ref()
                    .unwrap()
                    .delete
                    .id,
            );
        }
        {
            let state = queue().lock().unwrap();
            assert!(state.pending.iter().any(|d| d.id == ids[0]));
            assert!(!state.live.contains_key(&ids[0]));
            for id in &ids[1..] {
                assert!(state.live.contains_key(id));
                assert!(!state.pending.iter().any(|d| &d.id == id));
            }
        }
        drop(picture);
        let state = queue().lock().unwrap();
        for id in ids {
            assert!(!state.live.contains_key(&id));
            assert_eq!(state.pending.iter().filter(|d| d.id == id).count(), 1);
        }
    }
}
