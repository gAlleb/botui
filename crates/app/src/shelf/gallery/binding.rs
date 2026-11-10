//! BOTUI's clothbound jacket, shared by every catalog and the desktop cover viewer.
use super::caption;
use crate::shelf::background::{Ticket, Worker};
use crate::{
    catalog::Book,
    cover::{ImagePlacement, Picture},
};
use image::{DynamicImage, RgbaImage};
use lru::LruCache;
use ratatui::{Frame, layout::Rect};
use ratatui_image::picker::Picker;
use resvg::{tiny_skia, usvg};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Hash, Eq, PartialEq)]
pub(crate) struct Binding {
    pub(super) title: String,
    author: String,
    collection: String,
    ink: usize,
}

const CLOTH: [(u8, u8, u8); 8] = [
    (49, 81, 77),
    (85, 49, 69),
    (55, 71, 103),
    (102, 58, 47),
    (65, 80, 54),
    (68, 56, 86),
    (44, 76, 91),
    (94, 71, 49),
];

struct Jackets {
    images: LruCache<Binding, Option<Picture>>,
    worker: Worker,
    pending: Vec<(Binding, Ticket<Option<DynamicImage>>)>,
    visible: Vec<Binding>,
}
impl Jackets {
    fn collect(&mut self) {
        self.pending.retain(|(key, ticket)| {
            let image = match ticket.receiver.try_recv() {
                Ok(image) => image,
                Err(std::sync::mpsc::TryRecvError::Empty) => return true,
                Err(_) => None,
            };
            self.images
                .put(key.clone(), image.map(Picture::generated_cover));
            false
        });
    }

    fn finish_frame(&mut self) {
        // Dropping a ticket skips its job if the worker hasn't started it yet.
        self.pending.retain(|(key, _)| self.visible.contains(key));
        for key in self.visible.drain(..) {
            if self.pending.len() >= 12 {
                break;
            }
            if !self.pending.iter().any(|(pending, _)| pending == &key) {
                let binding = key.clone();
                let job = self.worker.start(move |_| binding.thumbnail());
                self.pending.push((key, job));
            }
        }
    }
}
thread_local! {
    static JACKETS: RefCell<Jackets> = RefCell::new(Jackets {
        images: LruCache::new(NonZeroUsize::new(48).unwrap()),
        worker: Worker::new(),
        pending: Vec::new(),
        visible: Vec::new(),
    });
}

pub(crate) fn collect() {
    JACKETS.with_borrow_mut(Jackets::collect);
}

pub(crate) fn finish_frame() {
    JACKETS.with_borrow_mut(Jackets::finish_frame);
}

pub(crate) fn busy() -> bool {
    JACKETS.with_borrow(|cache| !cache.pending.is_empty())
}

impl From<&Book> for Binding {
    fn from(book: &Book) -> Self {
        Self::new(
            book.title.as_deref().unwrap_or_default(),
            &book.authors,
            book.series.as_deref(),
            book.series_index.as_deref(),
        )
    }
}

fn clean(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control() && !matches!(*c, '\u{fffe}' | '\u{ffff}'))
        .collect()
}

impl Binding {
    pub(crate) fn new(
        title: &str,
        authors: &[String],
        series: Option<&str>,
        index: Option<&str>,
    ) -> Self {
        let title = clean(title);
        let series = clean(series.unwrap_or_default());
        let ink = Sha256::digest(if series.is_empty() { &title } else { &series }.as_bytes())[0]
            as usize
            % CLOTH.len();
        let collection = match index.map(str::trim).filter(|s| !s.is_empty()) {
            Some(n) if !series.is_empty() => format!("{series} · {}", clean(n)),
            _ => series,
        };
        Self {
            title: if title.is_empty() {
                "Untitled".into()
            } else {
                title
            },
            author: clean(&authors.join(", ")),
            ink,
            collection,
        }
    }
    pub(super) fn cloth(&self) -> (u8, u8, u8) {
        CLOTH[self.ink]
    }

    pub(crate) fn png(&self) -> botui_bookorbit::Result<Vec<u8>> {
        crate::cover_viewer::png(self.render(3)?)
    }

    pub(super) fn draw_or_wait(
        &self,
        frame: &mut Frame,
        slot: Rect,
        picker: &Picker,
        placement: ImagePlacement,
    ) -> bool {
        JACKETS.with_borrow_mut(|cache| {
            if let Some(cached) = cache.images.get(self) {
                if let Some(picture) = cached {
                    picture.draw_cover(frame, slot, picker, placement);
                }
                return cached.is_some();
            }
            // The caption already identifies the book while its jacket is prepared.
            // Terminal lettering is the fallback if rasterization actually fails.
            if !cache.visible.contains(self) {
                cache.visible.push(self.clone());
            }
            true
        })
    }

    fn thumbnail(&self) -> Option<DynamicImage> {
        let dir = crate::cache::cache_dir().ok().map(|p| p.join("jackets-v1"));
        self.thumbnail_at(dir)
    }

    fn thumbnail_at(&self, dir: Option<std::path::PathBuf>) -> Option<DynamicImage> {
        let path = dir
            .as_ref()
            .map(|d| d.join(format!("{}.png", self.cache_key())));
        if let Some(path) = &path
            && let Ok(bytes) = std::fs::read(path)
            && bytes.len() < 1 << 20
            && let Ok(image) = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            && image.width() == 240
            && image.height() == 360
        {
            return Some(image);
        }
        let image = self.render(1).ok()?;
        // Cache failure must never stop reading. Atomic publication also permits
        // concurrent readers; generated jackets are disposable, not downloaded books.
        if let (Some(dir), Some(path)) = (dir, path) {
            let _ = (|| -> std::io::Result<()> {
                use image::ImageEncoder;
                std::fs::create_dir_all(&dir)?;
                let mut file = tempfile::NamedTempFile::new_in(&dir)?;
                image::codecs::png::PngEncoder::new_with_quality(
                    file.as_file_mut(),
                    image::codecs::png::CompressionType::Fast,
                    image::codecs::png::FilterType::Sub,
                )
                .write_image(image.as_bytes(), 240, 360, image::ExtendedColorType::Rgba8)
                .map_err(std::io::Error::other)?;
                file.persist(path).map_err(|e| e.error)?;
                trim_cache(&dir);
                Ok(())
            })();
        }
        Some(image)
    }

    fn cache_key(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(include_bytes!("binding.svg"));
        for value in [&self.title, &self.author, &self.collection] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        hash.update([self.ink as u8]);
        format!("{:x}", hash.finalize())
    }

    fn render(&self, scale: u32) -> botui_bookorbit::Result<DynamicImage> {
        let fonts = fonts();
        if fonts.is_empty() {
            return Err("Install a system font to export a lettered cover.".into());
        }
        let opts = usvg::Options {
            fontdb: Arc::clone(fonts),
            font_resolver: font_resolver(),
            ..Default::default()
        };
        let (r, g, b) = self.cloth();
        let cloth = format!("#{r:02x}{g:02x}{b:02x}");
        let title = caption(&self.title, 16, 5);
        let by = caption(&self.author, 26, 2);
        let mut labels = Vec::new();
        let first = 178 - 12 * (title.lines().count() - 1);
        for (i, line) in title.lines().enumerate() {
            labels.push((line, first + i * 24, 21));
        }
        for (i, line) in by.lines().enumerate() {
            labels.push((line, 257 + i * 15, 13));
        }
        let series = caption(&self.collection, 31, 1);
        if !series.is_empty() {
            labels.push((&series, 331, 10));
        }

        let page = |measure: Option<&usvg::Tree>| {
            let mut lettering = String::new();
            for (i, (line, y, size)) in labels.iter().enumerate() {
                let id = format!("line{i}");
                let compressed = measure
                    .and_then(|t| t.node_by_id(&id))
                    .is_some_and(|n| n.abs_bounding_box().width() > 177.);
                let fit = if compressed {
                    r#" textLength="175" lengthAdjust="spacingAndGlyphs""#
                } else {
                    ""
                };
                let text = line
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                use std::fmt::Write;
                write!(
                    lettering,
                    r#"<text id="{id}" x="128" y="{y}" font-size="{size}"{fit}>{text}</text>"#
                )
                .unwrap();
            }
            format!(
                include_str!("binding.svg"),
                cloth = cloth,
                lettering = lettering
            )
        };
        let measured = usvg::Tree::from_str(&page(None), &opts)?;
        let tree = usvg::Tree::from_str(&page(Some(&measured)), &opts)?;
        let (w, h) = (240 * scale, 360 * scale);
        let mut surface = tiny_skia::Pixmap::new(w, h).ok_or("Invalid cover dimensions")?;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale as f32, scale as f32),
            &mut surface.as_mut(),
        );
        Ok(DynamicImage::ImageRgba8(
            RgbaImage::from_raw(w, h, surface.take()).ok_or("Invalid cover pixels")?,
        ))
    }
}

fn trim_cache(dir: &std::path::Path) {
    // Bound persistent storage without scanning on a cache hit or in the UI loop.
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = files
        .flatten()
        .filter_map(|f| {
            let name = f.file_name();
            let name = name.to_str()?;
            let key = name.strip_suffix(".png")?;
            if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            Some((f.metadata().ok()?.modified().ok()?, f.path()))
        })
        .collect();
    if entries.len() > 512 {
        entries.sort_unstable();
        for (_, path) in entries.iter().take(entries.len() - 512) {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn font_resolver() -> usvg::FontResolver<'static> {
    let fallback = usvg::FontResolver::default_fallback_selector();
    let scanned = std::sync::atomic::AtomicBool::new(false);
    usvg::FontResolver {
        select_font: usvg::FontResolver::default_font_selector(),
        select_fallback: Box::new(move |ch, excluded, db| {
            if let Some(id) = fallback(ch, excluded, db) {
                return Some(id);
            }
            if let Ok(output) = std::process::Command::new("fc-match")
                .args(["-f", "%{file}\n", &format!("serif:charset={:x}", ch as u32)])
                .output()
                && output.status.success()
                && let Some(path) = String::from_utf8_lossy(&output.stdout).lines().next()
            {
                let _ = Arc::make_mut(db).load_font_file(path);
                if let Some(id) = fallback(ch, excluded, db) {
                    return Some(id);
                }
            }
            // Portable fallback when fontconfig is absent or cannot resolve a glyph.
            if !scanned.swap(true, std::sync::atomic::Ordering::Relaxed) {
                Arc::make_mut(db).load_system_fonts();
            }
            fallback(ch, excluded, db)
        }),
    }
}

fn fonts() -> &'static Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        // Load the preferred face from the system index, not every fallback font.
        // Other scripts are resolved lazily only when a title actually needs them.
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}\n", "DejaVu Serif"])
            .output()
            && output.status.success()
        {
            let mut seen = std::collections::HashSet::new();
            for path in String::from_utf8_lossy(&output.stdout).lines().take(1) {
                if seen.insert(path) {
                    let _ = db.load_font_file(path);
                }
            }
        }
        if db.is_empty() {
            db.load_system_fonts();
        }
        let preferred = ["DejaVu Serif", "Liberation Serif", "Noto Serif"]
            .into_iter()
            .find(|wanted| {
                db.faces()
                    .any(|f| f.families.iter().any(|(name, _)| name == wanted))
            });
        let family = preferred.map(String::from).or_else(|| {
            db.faces()
                .find_map(|f| f.families.first().map(|(s, _)| s.clone()))
        });
        if let Some(name) = family {
            db.set_serif_family(name);
        }
        Arc::new(db)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_jacket_is_reused_and_damaged_cache_is_repaired() {
        if fonts().is_empty() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let binding = Binding::new("Прах и пепел", &["Автор".into()], None, None);
        let root = Some(dir.path().to_path_buf());
        let image = binding.thumbnail_at(root.clone()).unwrap();
        let path = dir.path().join(format!("{}.png", binding.cache_key()));
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(
            binding.thumbnail_at(root.clone()).unwrap().as_bytes(),
            image.as_bytes()
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        std::fs::write(&path, b"broken png").unwrap();
        assert_eq!(
            binding.thumbnail_at(root).unwrap().as_bytes(),
            image.as_bytes()
        );
        assert!(image::open(path).is_ok());
    }

    #[test]
    fn changing_shelves_replaces_waiting_jackets_in_the_same_frame() {
        use std::{sync::mpsc, time::Duration};
        JACKETS.with_borrow_mut(|cache| {
            let (release, gate) = mpsc::channel();
            let _hold = cache.worker.start(move |_| gate.recv().unwrap());
            let (started, jobs) = mpsc::channel();
            for i in 0..12 {
                let key = Binding::new(&format!("Book {i}"), &[], None, None);
                let started = started.clone();
                let job = cache.worker.start(move |_| {
                    started.send(i).unwrap();
                    None
                });
                cache.pending.push((key, job));
            }
            let keep = cache.pending[11].0.clone();
            let new = Binding::new("Next shelf", &[], None, None);
            cache.visible.extend([keep.clone(), new.clone()]);
            cache.finish_frame();
            assert_eq!(cache.pending.len(), 2);
            release.send(()).unwrap();
            let drained = cache.worker.start(|_| ());
            drained
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            cache.collect();
            assert!(cache.images.contains(&new) && cache.images.contains(&keep));
            assert_eq!(jobs.try_iter().collect::<Vec<_>>(), [11]);
        });
    }

    #[test]
    fn series_has_one_cloth_and_keeps_distinct_titles_and_volume_numbers() {
        let one = Binding::new("A & B", &[], Some("Amber"), Some("1"));
        let two = Binding::new("Свет <и> тень", &[], Some("Amber"), Some("2"));
        assert_eq!(one.cloth(), two.cloth());
        assert_ne!(one.title, two.title);
        assert_eq!(two.collection, "Amber · 2");
        assert_eq!(Binding::new("\n", &[], None, None).title, "Untitled");
    }
    #[test]
    fn names_with_markup_and_cyrillic_render_as_letters_on_the_jacket() {
        // On a fontless machine the gallery uses native terminal lettering.
        if fonts().is_empty() {
            return;
        }
        let binding = Binding::new("Книга <A&B>", &["И. Иванов".into()], None, None);
        let image = binding.render(1).unwrap().into_rgb8();
        let (r, g, b) = binding.cloth();
        assert_eq!(image[(75, 115)].0, [r, g, b]);
        // The title and author sit between the sprig and the bottom rule.
        for rows in [152..181, 240..260] {
            assert!(
                rows.into_iter()
                    .any(|y| (36..217).any(|x| image[(x, y)][0] > 200))
            );
        }
    }
}
