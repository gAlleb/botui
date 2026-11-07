use super::{Context, Settings};
use ratatui::{
    Frame,
    layout::Rect,
    style::Stylize,
    text::Line,
    widgets::{Paragraph, Wrap},
};

pub(super) enum Entry {
    Section(String),
    Key(String, String),
}
fn key(k: &str, v: &str) -> Entry {
    Entry::Key(k.into(), v.into())
}
pub(super) fn entries(ctx: Context, p: &Settings) -> Vec<Entry> {
    let mut rows = vec![
        key("F2", "Open settings"),
        key("? / F1", "Keyboard shortcuts"),
        key("Esc", "Back / close a panel"),
    ];
    let mut groups = vec![ctx];
    for c in [Context::Shelf, Context::Local, Context::Text, Context::Scan] {
        if c != ctx {
            groups.push(c)
        }
    }
    for c in groups {
        let (name, keys): (&str, &[(&str, &str)]) = match c {
            Context::Shelf => (
                "BookOrbit & catalogs",
                &[
                    (
                        "0…7",
                        "Dashboard, books, reading, recent, libraries, collections, authors, series",
                    ),
                    ("/", "Find books or search the current directory"),
                    ("V / U / F", "Grid or list / borders / background"),
                    ("Enter / click", "Open the selected item"),
                    ("F6", "Focus navigation; ←/→ fold or expand groups"),
                    ("Q / Esc", "Return to source chooser / back one level"),
                    ("PgUp/PgDn", "Move through the shelf"),
                    ("D", "Save the selected file offline"),
                    ("L / B", "Reading log / related books on the book card"),
                ],
            ),
            Context::Local => (
                "Local library",
                &[
                    ("P / R", "Choose a folder / use this folder as the library"),
                    ("1 / 2 / 3", "Folders / currently reading / recently added"),
                    ("/", "Find a file, title, author or series in your library"),
                    ("V / U / F", "Grid or list / borders / background"),
                ],
            ),
            Context::Text => (
                "EPUB & FB2",
                &[
                    ("M / Esc", "Reading menu"),
                    ("← / →", "Previous / next page (or scroll)"),
                    ("1 / 2 / 3", "Single page / spread / continuous scroll"),
                    ("W", "Set page width and column gap"),
                    ("[ / ]", "Widen / narrow each page"),
                    ("{ / }", "Decrease / increase column gap"),
                    ("O", "Contents"),
                    ("/", "Find in book"),
                    ("F3 / F4 / F5", "Next match / previous match / result list"),
                    ("T / F / U", "Reader theme / background / borders"),
                    ("B / G", "Toggle bookmark / open bookmarks"),
                    ("Delete", "Remove the selected bookmark from its list"),
                    ("N / A / E", "Edit note / show notes / export Markdown"),
                    ("I / S", "Links and illustrations / reading statistics"),
                    (", / .", "Jump back / forward"),
                    ("Q", "Close the book"),
                ],
            ),
            Context::Scan => (
                "PDF & DjVu",
                &[
                    ("M / Esc", "Reading menu"),
                    ("1 / 2 / 3", "Single page / spread / continuous scroll"),
                    ("+ / −", "Zoom in / out; arrows pan when zoomed"),
                    ("P / Ctrl+G", "Go directly to a page number"),
                    ("Home / End", "First / last page"),
                    ("/", "Find in the text layer"),
                    (
                        "F3 / F4 / F5",
                        "Next matching page / previous page with a match / result list",
                    ),
                    (", / .", "Jump back / forward"),
                    ("B / G", "Toggle bookmark / open bookmarks"),
                    ("Delete", "Remove the selected bookmark from its list"),
                    ("I / A / E", "Edit page note / show notes / export Markdown"),
                    ("S / Q", "Book statistics / close book"),
                ],
            ),
        };
        rows.push(Entry::Section(name.into()));
        rows.extend(keys.iter().map(|(k, v)| key(k, v)));
        if c == Context::Text && !p.keys.is_empty() {
            rows.push(Entry::Section("Your EPUB / FB2 overrides".into()));
            rows.extend(p.keys.iter().map(|(k, v)| key(&k.to_string(), v)));
        }
    }
    rows
}
/// Two actual terminal columns, with a shared display-cell width. Scroll physical
/// rows so a wrapped description is still reachable in a short terminal.
pub(super) fn draw(f: &mut Frame, area: Rect, entries: &[Entry], offset: &mut usize) -> usize {
    if area.is_empty() {
        return 0;
    }
    let pad = u16::from(area.width >= 8);
    let x = area.x + pad;
    let w = area.width - 2 * pad;
    let stacked = w < 30;
    let key_width = entries
        .iter()
        .filter_map(|e| {
            if let Entry::Key(k, _) = e {
                Some(Line::from(k.as_str()).width() as u16)
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0)
        .min(w / 2);
    let desc_dx = if stacked {
        2.min(w.saturating_sub(1))
    } else {
        (key_width + 2).min(w.saturating_sub(1))
    };
    let desc_w = w - desc_dx;
    let wrap = Wrap { trim: true };
    let mut row = 0usize;
    let mut blocks = vec![];
    for entry in entries {
        match entry {
            Entry::Section(title) => {
                row += 1;
                let p = Paragraph::new(format!("── {title}")).bold().wrap(wrap);
                let h = p.line_count(w).max(1);
                blocks.push((p, x, w, row, h));
                row += h;
            }
            Entry::Key(k, v) => {
                let key = Paragraph::new(k.as_str()).cyan().wrap(wrap);
                let kw = if stacked { w } else { key_width };
                let kh = key.line_count(kw).max(1);
                let desc = Paragraph::new(v.as_str()).wrap(wrap);
                let dh = desc.line_count(desc_w).max(1);
                blocks.push((key, x, kw, row, kh));
                blocks.push((
                    desc,
                    x + desc_dx,
                    desc_w,
                    row + if stacked { kh } else { 0 },
                    dh,
                ));
                row += if stacked { kh + dh } else { kh.max(dh) };
            }
        }
    }
    let max = row.saturating_sub(usize::from(area.height));
    *offset = (*offset).min(max);
    for (p, x, w, y, h) in blocks {
        let from = y.max(*offset);
        let to = (y + h).min(*offset + usize::from(area.height));
        if from < to {
            f.render_widget(
                p.scroll(((from - y) as u16, 0)),
                Rect::new(x, area.y + (from - *offset) as u16, w, (to - from) as u16),
            );
        }
    }
    max
}
