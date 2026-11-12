mod filter;
use super::{Context, Settings};
use crossterm::event::Event;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
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
        Entry::Section("General".into()),
        key("F2", "Open settings"),
        key(
            "F1 / Space ?",
            "Keyboard shortcuts (? also works on shelves)",
        ),
        key("Esc", "Back / close a panel"),
    ];
    rows.push(Entry::Section(
        "Space commands · short hint delay, immediate execution".into(),
    ));
    let context = match ctx {
        Context::Text => crate::leader::Context::Text,
        Context::Scan => crate::leader::Context::Pdf,
        _ => crate::leader::Context::Shelf,
    };
    rows.extend(
        crate::leader::help(context)
            .into_iter()
            .map(|(k, v)| key(&k, &v)),
    );
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
                    (
                        "/",
                        "BookOrbit: type to filter books, authors or series · Enter opens",
                    ),
                    ("V / U / F", "Grid or list / borders / background"),
                    ("Enter / click", "Open the selected item"),
                    ("F6", "Focus navigation; ←/→ fold or expand groups"),
                    (
                        "Q / Esc",
                        "Back to the previous screen; Q on the start screen quits",
                    ),
                    ("PgUp/PgDn", "Move through the shelf"),
                    (
                        "C",
                        "Open the cover in your image viewer from the book card",
                    ),
                    ("D", "Save the selected file offline"),
                    (
                        "A / N",
                        "Book card: manage collections / Collections: create one",
                    ),
                    (
                        "Del",
                        "Delete your selected collection or remove a downloaded copy",
                    ),
                    ("L / B", "Reading log / related books on the book card"),
                ],
            ),
            Context::Local => (
                "Local library",
                &[
                    (
                        "Q / Esc",
                        "Back to the folder, then its parents; leave at the library root",
                    ),
                    ("F6", "Focus the folder tree or return to the books"),
                    (
                        "← / →",
                        "Collapse / expand the focused folder; Enter opens it",
                    ),
                    ("P / R", "Choose a folder / use this folder as the library"),
                    ("I", "File info and external viewer for PDF or DjVu"),
                    ("1 / 2 / 3", "Folders / currently reading / recently added"),
                    (
                        "4 / A",
                        "Browse local collections / add or remove the selected book",
                    ),
                    (
                        "Del",
                        "Delete the open local collection or remove a downloaded copy",
                    ),
                    ("/", "Live fuzzy search · type to filter, Enter to open"),
                    ("V / U / F", "Grid or list / borders / background"),
                ],
            ),
            Context::Text => (
                "EPUB & FB2 · reading without cursor",
                &[
                    ("Q", "Close book and return to its source"),
                    ("← / → · PgUp / PgDn", "Previous / next page"),
                    (
                        "h j k l / arrows",
                        "Navigate; scroll mode moves through text",
                    ),
                    ("Mouse wheel", "Scroll; F2 → Reader sets lines per tick"),
                    (
                        "1 / 2 / 3",
                        "Single page / two-page spread / continuous scroll",
                    ),
                    ("F3 / F4 / F5", "Next / previous match / search results"),
                    (
                        "Mouse",
                        "Click: cursor; drag: select; double/triple click: word/paragraph",
                    ),
                    ("Ctrl+C", "Copy mouse selection"),
                    ("Tab", "Focus contents when visible"),
                    (
                        "Esc / Enter",
                        "Return from an in-book link / keep reading there",
                    ),
                    (
                        "Space",
                        "Commands below; letters such as v or t have no reading action",
                    ),
                ],
            ),
            Context::Scan => (
                "PDF & DjVu · reading without cursor",
                &[
                    ("Q", "Close book and return to its source"),
                    (
                        "← / → · PgUp / PgDn",
                        "Navigate pages; arrows pan a zoomed page",
                    ),
                    ("Home / End", "First / last page"),
                    ("+ / = / −", "Zoom in / zoom in / zoom out"),
                    ("Mouse wheel", "Scroll; F2 → Reader sets lines per tick"),
                    (
                        "1 / 2 / 3",
                        "Single page / two-page spread / continuous scroll",
                    ),
                    ("Tab; ← / →", "Focus contents; fold / expand a section"),
                    ("F3 / F4 / F5", "Next / previous matching page / results"),
                    (
                        "Mouse drag / Ctrl+C",
                        "Select / copy PDF text (text layer required; no OCR)",
                    ),
                    ("Space t", "Original colors / inverted / match background"),
                    ("Space", "Commands below; v/V select only in Vim mode"),
                ],
            ),
        };
        rows.push(Entry::Section(name.into()));
        rows.extend(keys.iter().map(|(k, v)| key(k, v)));
        if c != ctx && matches!(c, Context::Text | Context::Scan) {
            let context = if c == Context::Text {
                crate::leader::Context::Text
            } else {
                crate::leader::Context::Pdf
            };
            rows.extend(
                crate::leader::help(context)
                    .into_iter()
                    .map(|(k, v)| key(&k, &v)),
            );
        }
    }
    {
        rows.push(Entry::Section("Vim · enable / disable with Space c".into()));
        for (k, v) in [
            ("v / V", "Visual selection / logical line"),
            (
                "y",
                "Copy selection in Visual; begin copy operator in Normal",
            ),
            (
                "Esc",
                "Clear selection / pending command; in idle Normal return to reading",
            ),
            ("h j k l", "Move cursor; j/k follow visible text lines"),
            ("{ / }", "Previous / next paragraph (PDF: text block)"),
            ("w b e / 0 ^ $", "Words / logical line edges"),
            (
                "gj gk / g0 g$",
                "Wrapped screen lines / their edges (EPUB & FB2)",
            ),
            ("gg / G", "Start / end (PDF: current page)"),
            ("yiw / yaw / yip", "Copy word / word with space / paragraph"),
            (
                "yw / ye / yy / y$",
                "Copy through a motion; yy copies a logical line",
            ),
            ("3yw / y3w", "Copy three words"),
            (
                "viw / vip / gv",
                "Select word / paragraph / previous selection",
            ),
            (
                "/ ? / n N",
                "Search forward / backward; next / previous match",
            ),
            ("ma / 'a / `a", "Set named mark a / return to it"),
            ("Ctrl+O / Ctrl+I", "Jump back / forward"),
            ("K", "Dictionary for word or selection"),
        ] {
            rows.push(key(k, v));
        }
    }
    rows.push(Entry::Section("Panels & editors".into()));
    for (k, v) in [
        (
            "Esc",
            "Close panel; editor offers save/discard for unsaved changes",
        ),
        (
            "Ctrl+S / Enter / Tab",
            "Note editor: save / new line / highlight color",
        ),
        (
            "/ N H Y Del E",
            "Annotation list: filter / edit / color / copy / delete / export",
        ),
        (
            "P L S Y N",
            "Lookup result: provider / target language / source language / copy / note",
        ),
        ("[ ] / { }", "Page layout panel: width / column gap"),
    ] {
        rows.push(key(k, v));
    }
    if !p.keys.is_empty() {
        rows.push(Entry::Section("Your EPUB / FB2 overrides".into()));
        rows.extend(p.keys.iter().map(|(k, action)| key(&k.to_string(), action)));
    }
    rows
}

#[derive(Default)]
pub(super) struct Guide {
    filter: filter::Filter,
    offset: usize,
    max: usize,
    // Physical scroll position and mouse target of each section.
    sections: Vec<(usize, Rect)>,
}

impl Guide {
    pub fn scroll(&mut self, amount: isize) {
        self.offset = self.offset.saturating_add_signed(amount).min(self.max);
    }

    pub fn section(&mut self, next: bool) {
        let mut rows = self.sections.iter().map(|(start, _)| *start);
        self.offset = if next {
            rows.find(|&n| n > self.offset).unwrap_or(self.max)
        } else {
            rows.rfind(|&n| n < self.offset).unwrap_or(0)
        };
    }

    pub fn filtering(&self) -> bool {
        self.filter.active()
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.offset = 0;
    }

    pub fn filter_input(&mut self, e: &Event) -> bool {
        if self.filter.input(e) {
            self.offset = 0;
            true
        } else {
            false
        }
    }

    pub fn click(&mut self, pos: Position) -> bool {
        if self.filter.click(pos) {
            return true;
        }
        let Some((row, _)) = self.sections.iter().find(|(_, r)| r.contains(pos)) else {
            return false;
        };
        self.filter.finish();
        self.offset = *row;
        true
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, entries: &[Entry]) {
        self.sections.clear();
        if area.is_empty() {
            return;
        }
        let matches = self.filter.matching(entries);
        let count = matches
            .iter()
            .filter(|e| matches!(e, Entry::Key(..)))
            .count();
        self.filter
            .draw(f, Rect::new(area.x, area.y, area.width, 1), count);
        let gap = 1 + u16::from(area.height >= 18);
        let area = Rect::new(
            area.x,
            area.y + gap,
            area.width,
            area.height.saturating_sub(gap),
        );
        if area.is_empty() {
            return;
        }
        if count == 0 {
            f.render_widget(Paragraph::new(" No matching shortcuts.").dim(), area);
            self.max = 0;
            return;
        }
        let rail = if area.width >= 96 && area.height >= 12 {
            26
        } else {
            0
        };
        let pad = u16::from(area.width >= 8);
        let x = area.x + rail + pad;
        let width = area.width - rail - 2 * pad;
        let stacked = width < 32;
        let wrap = Wrap { trim: true };
        let cap = cap_style();
        let key_width = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Key(k, _) => Some(keycaps(k, cap).width() as u16),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            .min(width / 2);
        let desc_x = if stacked {
            2.min(width.saturating_sub(1))
        } else {
            (key_width + 2).min(width.saturating_sub(1))
        };
        let desc_w = width - desc_x;
        // Use the same wrapped heights for drawing, jumps and wheel scrolling.
        let mut row = 0;
        let mut blocks = Vec::new();
        let mut headings = Vec::new();
        for e in matches {
            match e {
                Entry::Section(title) => {
                    if row != 0 {
                        row += 1;
                    }
                    headings.push((row, title.as_str()));
                    let title = format!("▎ {} ", title.to_uppercase());
                    let rule = "─".repeat(
                        usize::from(width).saturating_sub(Line::from(title.as_str()).width()),
                    );
                    let line = Line::from(vec![
                        Span::styled(title, Style::new().cyan().bold()),
                        Span::styled(rule, Style::new().cyan().dim()),
                    ]);
                    blocks.push((Paragraph::new(line), x, width, row, 1));
                    row += 1;
                }
                Entry::Key(k, description) => {
                    let keys = Paragraph::new(keycaps(k, cap)).wrap(Wrap { trim: false });
                    let kw = if stacked { width } else { key_width };
                    let kh = keys.line_count(kw).max(1);
                    let text = Paragraph::new(description.as_str()).wrap(wrap);
                    let dh = text.line_count(desc_w).max(1);
                    blocks.push((keys, x, kw, row, kh));
                    blocks.push((
                        text,
                        x + desc_x,
                        desc_w,
                        row + if stacked { kh } else { 0 },
                        dh,
                    ));
                    row += if stacked { kh + dh } else { kh.max(dh) };
                }
            }
        }
        self.max = row
            .saturating_sub(usize::from(area.height))
            .max(headings.last().map_or(0, |(y, _)| *y));
        self.offset = self.offset.min(self.max);
        let active = headings
            .partition_point(|(y, _)| *y <= self.offset)
            .saturating_sub(1);
        if rail > 0 {
            f.render_widget(
                Paragraph::new("Sections").bold(),
                Rect::new(area.x + 1, area.y, rail - 3, 1),
            );
            for y in area.y..area.bottom() {
                f.render_widget(
                    Paragraph::new("│").dim(),
                    Rect::new(area.x + rail - 2, y, 1, 1),
                );
            }
        }
        for (i, (start, label)) in headings.into_iter().enumerate() {
            let mut hit = Rect::default();
            if rail > 0 && i + 2 < usize::from(area.height) {
                hit = Rect::new(area.x + 1, area.y + i as u16 + 2, rail - 4, 1);
                let label = if label == "Your EPUB / FB2 overrides" {
                    "Your bindings"
                } else {
                    label
                };
                let row = if i == active {
                    Paragraph::new(format!("▸ {label}")).cyan().bold()
                } else {
                    Paragraph::new(format!("  {label}"))
                };
                f.render_widget(row, hit);
            }
            self.sections.push((start, hit));
        }
        for (p, x, w, y, h) in blocks {
            let from = y.max(self.offset);
            let to = (y + h).min(self.offset + usize::from(area.height));
            if from < to {
                f.render_widget(
                    p.scroll(((from - y) as u16, 0)),
                    Rect::new(
                        x,
                        area.y + (from - self.offset) as u16,
                        w,
                        (to - from) as u16,
                    ),
                );
            }
        }
    }
}

fn keycaps(keys: &str, style: Style) -> Line<'static> {
    let mut spans = vec![];
    for k in keys.split(" / ") {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(format!(" {k} "), style));
    }
    Line::from(spans)
}

fn cap_style() -> Style {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return Style::new().reversed();
    }
    let palette = crate::settings::interface_palette();
    if palette.0 == "terminal" {
        return Style::new().cyan().reversed();
    }
    let (fg, bg, accent) = palette.colors();
    let (Color::Rgb(r, g, b), Color::Rgb(ar, ag, ab)) = (bg, accent) else {
        return Style::new().bold();
    };
    let tint = |bg: u8, ink: u8| ((u16::from(bg) * 4 + u16::from(ink)) / 5) as u8;
    Style::new()
        .fg(fg)
        .bg(Color::Rgb(tint(r, ar), tint(g, ag), tint(b, ab)))
        .bold()
}
