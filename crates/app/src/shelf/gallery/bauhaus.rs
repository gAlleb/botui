// BookOrbit artwork adaptation: see licenses/NOTICE.md and licenses/bookorbit/.
//! BookOrbit's default generated cover. The browser chooses its palette by series or
//! author and its geometric figure by the catalog title. These keys intentionally
//! use JavaScript UTF-16 code units, including for non-Latin catalog entries.
//! Compatibility reference: BookOrbit client/src/features/book/lib/placeholder/{palette,styles/bauhaus}.ts.
mod artwork;
mod lettering;
use super::{Book, caption};
use crate::cover::Picture;
use image::{DynamicImage, RgbaImage};
use lru::LruCache;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::Paragraph,
};
use ratatui_image::picker::{Picker, ProtocolType};
use resvg::{tiny_skia, usvg};
use std::{cell::RefCell, num::NonZeroUsize};

const W: u32 = 240;
const H: u32 = 360;
#[derive(Debug, Hash, Eq, PartialEq)]
struct Design {
    group: u32,
    item: u32,
    title: String,
    author: String,
    index: Option<String>,
}
fn hash(s: &str) -> u32 {
    s.encode_utf16().fold(2_166_136_261, |h, c| {
        (h ^ u32::from(c)).wrapping_mul(16_777_619)
    })
}
fn key(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
fn design(title: &str, author: &str, series: Option<&str>, index: Option<&str>) -> Design {
    let clean = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let main = clean
        .strip_suffix(')')
        .and_then(|s| s.rsplit_once('('))
        .filter(|(base, suffix)| {
            !base.trim().is_empty() && !suffix.is_empty() && !suffix.contains(')')
        })
        .map_or(clean.as_str(), |(base, _)| base.trim_end());
    let main = [" – ", " — ", " - ", ": "]
        .iter()
        .filter_map(|sep| main.split_once(sep))
        .map(|(base, _)| base.trim())
        .filter(|base| base.split_whitespace().count() >= 2)
        .min_by_key(|base| base.len())
        .unwrap_or(main);
    let normalized = key(main);
    let author_key = key(author);
    let group = series
        .map(key)
        .filter(|s| !s.is_empty() && !s.chars().all(char::is_numeric))
        .map(|s| format!("series:{s}"))
        .unwrap_or_else(|| {
            if author_key.is_empty() {
                format!("title:{normalized}")
            } else {
                format!("author:{author_key}")
            }
        });
    let index = index.map(str::trim).filter(|s| !s.is_empty());
    Design {
        group: hash(&group),
        item: hash(&format!("{normalized}|{}|{title}", index.unwrap_or(""))),
        title: main.chars().take(220).collect(),
        author: author.chars().take(100).collect(),
        index: index.map(|s| s.chars().take(12).collect()),
    }
}
struct Rendered {
    picture: Picture,
    cell_text: bool,
    bg: Color,
    fg: Color,
}

thread_local! {
    static COVERS: RefCell<LruCache<(Design,bool),Rendered>> = RefCell::new(LruCache::new(NonZeroUsize::new(32).unwrap()));
}
fn render(d: &Design, cells: bool) -> Option<Rendered> {
    let (source, cell_text) = raster(d, cells, 1)?;
    let palette = artwork::palette(d.group);
    Some(Rendered {
        picture: Picture::generated_cover(source),
        cell_text,
        bg: palette.bg.color(),
        fg: palette.text.color(),
    })
}
fn raster(d: &Design, cells: bool, scale: u32) -> Option<(DynamicImage, bool)> {
    let palette = artwork::palette(d.group);
    let drawings = artwork::motif(d.item, &palette, d.index.as_deref());
    let mut options = usvg::Options::default();
    let mut letters = String::new();
    if !cells {
        lettering::with_fonts(|fonts| {
            options.fontdb = fonts.clone();
            if !fonts.is_empty() {
                letters = lettering::typeset(d, &options, &palette);
            }
        });
    }
    let cell_text = letters.is_empty();
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}"><rect width="{W}" height="{H}" fill="{}"/>{drawings}{letters}</svg>"#,
        palette.bg.hex()
    );
    let tree = usvg::Tree::from_str(&svg, &options).ok()?;
    let mut canvas = tiny_skia::Pixmap::new(W * scale, H * scale)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale as f32, scale as f32),
        &mut canvas.as_mut(),
    );
    let mut pixels = canvas.take();
    if !cells {
        // Reproducible paper grain; keep the same texture between frames.
        let mut noise = d.group | 1;
        for pixel in pixels.as_chunks_mut::<4>().0 {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            let grain = ((noise >> 29) as i8) - 3;
            for channel in &mut pixel[..3] {
                *channel = channel.saturating_add_signed(grain);
            }
        }
    }
    Some((
        DynamicImage::ImageRgba8(RgbaImage::from_raw(W * scale, H * scale, pixels)?),
        cell_text,
    ))
}

pub(in crate::shelf) fn original(book: &Book) -> Option<DynamicImage> {
    let crate::catalog::Jacket::Bauhaus { series, index } = &book.jacket else {
        return None;
    };
    raster(
        &book_design(book, series.as_deref(), index.as_deref()),
        false,
        3,
    )
    .map(|(img, _)| img)
}

fn book_design(book: &Book, series: Option<&str>, index: Option<&str>) -> Design {
    let title = book.title.as_deref().unwrap_or("Untitled");
    let mut d = design(
        title,
        book.authors.first().map_or("", String::as_str),
        series,
        index,
    );
    if book.authors.len() == 2 {
        d.author = book.authors.join(" & ").chars().take(100).collect();
    } else if book.authors.len() > 2 {
        d.author = format!("{} +{}", d.author, book.authors.len() - 1);
    }
    d
}

pub(super) fn draw(
    f: &mut Frame,
    slot: Rect,
    graphics: &Picker,
    book: &Book,
    series: Option<&str>,
    index: Option<&str>,
) {
    let d = book_design(book, series, index);
    let mono = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let key = (
        d,
        mono || graphics.protocol_type() == ProtocolType::Halfblocks,
    );
    COVERS.with_borrow_mut(|cache| {
        if !cache.contains(&key) {
            let Some(art) = render(&key.0, key.1) else {
                return;
            };
            cache.push(key, art);
        } else {
            cache.promote(&key);
        }
        let Some(((design, _), jacket)) = cache.peek_mru() else {
            return;
        };
        jacket.picture.draw(f, slot, graphics);
        if jacket.cell_text {
            // Text remains real terminal lettering when pixels are represented by character cells.
            let cell = graphics.font_size();
            let ratio = if mono {
                4. / 3.
            } else {
                W as f32 / H as f32 * f32::from(cell.height) / f32::from(cell.width.max(1))
            };
            let w = slot
                .width
                .min((f32::from(slot.height) * ratio).round() as u16);
            let h = slot.height.min((f32::from(w) / ratio).ceil() as u16);
            let a = crate::centered(slot, w, h);
            if a.width < 3 || a.height < 3 {
                return;
            }
            let rows = (a.height / 2).saturating_sub(1).max(1);
            let body = Rect::new(a.x + 1, a.y + 1, a.width - 2, rows);
            let style = if mono {
                Style::new()
            } else {
                Style::new().fg(jacket.fg).bg(jacket.bg)
            };
            let txt = caption(
                &design.title,
                body.width,
                usize::from(rows.saturating_sub(1).max(1)),
            );
            let author_y = body.y + txt.lines().count() as u16;
            f.render_widget(Paragraph::new(txt).style(style.bold()), body);
            if author_y < body.bottom() {
                f.render_widget(
                    Paragraph::new(caption(&design.author, body.width, 1)).style(style),
                    Rect::new(body.x, author_y, body.width, 1),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cyrillic_catalog_seeds_match_the_browser_and_edition_changes_the_figure() {
        let book = design("Этот бессмертный (вариант 1)", "Роджер Желязны", None, None);
        assert_eq!(book.title, "Этот бессмертный");
        assert_eq!((book.group, book.item), (4239169901, 4034471136));
        assert_eq!(book.item % 7, 3, "concentric arches");
        let other = design("Этот бессмертный (вариант 2)", "Роджер Желязны", None, None);
        assert_eq!(book.group, other.group);
        assert_ne!(book.item, other.item);
        let palette = artwork::palette(book.group);
        assert_eq!(palette.bg.hex(), "#0f1610");
    }
    #[test]
    fn series_palette_follows_series_across_authors() {
        let first = design("A & B", "Jane", Some("One, Two"), Some("1"));
        let second = design("A < B", "John", Some("One Two"), Some("2"));
        assert_eq!(first.group, second.group);
        for d in [first, second] {
            assert!(render(&d, true).is_some());
        }
    }
    #[test]
    fn generated_art_contains_each_color_of_the_arch() {
        let d = design("Этот бессмертный (вариант 1)", "Роджер Желязны", None, None);
        let opts = usvg::Options::default();
        let p = artwork::palette(d.group);
        let svg = artwork::motif(d.item, &p, None);
        let tree = usvg::Tree::from_str(
            &format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}">{svg}</svg>"#
            ),
            &opts,
        )
        .unwrap();
        let mut canvas = tiny_skia::Pixmap::new(W, H).unwrap();
        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut canvas.as_mut(),
        );
        let rgb = |y| {
            let p = canvas.pixel(120, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        let (r, g, b) = rgb(222);
        assert!(g > r && g > b, "green outer arch");
        let (r, g, b) = rgb(269);
        assert!(r > g && b > g, "pink middle arch");
        let (r, g, b) = rgb(319);
        assert!(b > g && b > r, "blue inner arch");
    }
}
