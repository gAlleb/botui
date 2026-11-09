//! BOTUI's clothbound jacket, shared by every catalog and the desktop cover viewer.
use super::caption;
use crate::shelf::background::{Ticket, Worker};
use crate::{catalog::Book, cover::Picture};
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
    pending: Option<(Binding, Ticket<Option<DynamicImage>>)>,
}
impl Jackets {
    fn collect(&mut self) {
        let Some((key, ticket)) = &self.pending else {
            return;
        };
        let result = match ticket.receiver.try_recv() {
            Ok(value) => value,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => None,
        };
        self.images
            .put(key.clone(), result.map(Picture::generated_cover));
        self.pending = None;
    }
}
thread_local! {
    static JACKETS: RefCell<Jackets> = RefCell::new(Jackets {
        images: LruCache::new(NonZeroUsize::new(48).unwrap()),
        worker: Worker::new(),
        pending: None,
    });
}

pub(crate) fn busy() -> bool {
    JACKETS.with_borrow_mut(|cache| {
        let pending = cache.pending.is_some();
        cache.collect();
        pending
    })
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

    pub(super) fn draw(&self, frame: &mut Frame, slot: Rect, picker: &Picker) -> bool {
        JACKETS.with_borrow_mut(|cache| {
            cache.collect();
            if let Some(picture) = cache.images.get(self).and_then(Option::as_ref) {
                picture.draw(frame, slot, picker);
                true
            } else {
                // Font discovery, SVG shaping and rasterization must never
                // block entering a shelf. Use terminal lettering until ready.
                if !cache.images.contains(self) && cache.pending.is_none() {
                    let binding = self.clone();
                    cache.pending = Some((
                        self.clone(),
                        cache.worker.start(move |_| binding.render(1).ok()),
                    ));
                }
                false
            }
        })
    }

    fn render(&self, scale: u32) -> botui_bookorbit::Result<DynamicImage> {
        let fonts = fonts();
        if fonts.is_empty() {
            return Err("Install a system font to export a lettered cover.".into());
        }
        let opts = usvg::Options {
            fontdb: Arc::clone(fonts),
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

fn fonts() -> &'static Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
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
