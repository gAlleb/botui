mod section;
pub(crate) use section::SectionPosition;

use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Padding},
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

// ANSI roles are resolved by the existing interface or reading palette when the frame is painted.
pub const ACTION: Style = Style::new().fg(Color::Cyan);
pub const SELECTION: Style = ACTION.reversed().not_dim();
// Text selection must not inherit the terminal's ANSI black (which may be pale).
pub const TEXT_SELECTION: Style = Style::new()
    .fg(Color::Rgb(0, 0, 0))
    .bg(Color::Rgb(203, 214, 247))
    .not_dim()
    .not_reversed();
pub const CURRENT_SECTION: Style = ACTION.bold().underlined();
pub const SECONDARY: Style = Style::new().dim();

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum BordersStyle {
    #[default]
    Rounded,
    Square,
    None,
}

static ACTIVE: AtomicU8 = AtomicU8::new(BordersStyle::Rounded as u8);
impl BordersStyle {
    pub fn activate(self) {
        ACTIVE.store(self as u8, Ordering::Relaxed);
    }
    pub fn current() -> Self {
        match ACTIVE.load(Ordering::Relaxed) {
            1 => Self::Square,
            2 => Self::None,
            _ => Self::Rounded,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Rounded => "Rounded",
            Self::Square => "Square",
            Self::None => "None",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Rounded => Self::Square,
            Self::Square => Self::None,
            Self::None => Self::Rounded,
        }
    }
}
pub fn cycle() -> std::io::Result<()> {
    crate::settings::Settings::update(|s| s.borders = s.borders.next()).map(|_| ())
}
pub fn block<'a>() -> Block<'a> {
    let block = match BordersStyle::current() {
        BordersStyle::Rounded => Block::bordered().border_type(BorderType::Rounded),
        BordersStyle::Square => Block::bordered(),
        BordersStyle::None => Block::new().padding(Padding::horizontal(1)),
    };
    let border = border_style(false);
    let block = block.border_style(border);
    if let Some(caption) = reader_caption_style() {
        block.title_style(caption)
    } else {
        block
    }
}
pub fn progress_label(pages: Option<(usize, usize)>, percent: f64) -> String {
    pages.map_or_else(
        || format!("{percent:.0}%"),
        |(page, total)| format!("{page} / {total}  ·  {percent:.0}%"),
    )
}

/// Both readers lay out content inside the same frame. Hidden Zen chrome takes
/// no space: resizing the content keeps the document position, not old pages.
pub struct ReadingFrame {
    pub inner: Rect,
    area: Rect,
    borders: BordersStyle,
    focused: bool,
}

impl ReadingFrame {
    pub fn new(area: Rect, zen: bool, zen_progress: bool) -> Self {
        Self::with_borders(area, zen, zen_progress, BordersStyle::current())
    }

    pub(crate) fn with_borders(
        area: Rect,
        zen: bool,
        zen_progress: bool,
        style: BordersStyle,
    ) -> Self {
        let borders = if (!zen || zen_progress) && area.width >= 3 && area.height >= 3 {
            style
        } else {
            BordersStyle::None
        };
        let inner = if borders != BordersStyle::None {
            area.inner(Margin::new(1, 1))
        } else {
            Rect {
                height: area.height.saturating_sub(u16::from(zen && zen_progress)),
                ..area
            }
        };
        Self {
            inner,
            area,
            borders,
            focused: false,
        }
    }

    pub fn with_focus(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn has_border(&self) -> bool {
        self.borders != BordersStyle::None
    }

    pub fn draw(
        &self,
        frame: &mut ratatui::Frame,
        style: Style,
        title: Option<&str>,
        book_position: Option<&str>,
        section: Option<SectionPosition<'_>>,
    ) {
        let room = usize::from(self.inner.width).saturating_sub(2);
        let book_progress = book_position.and_then(|label| {
            let compact;
            let label = if Line::from(label).width() <= room {
                label
            } else {
                compact = label.split_whitespace().collect::<String>();
                if Line::from(compact.as_str()).width() <= room {
                    &compact
                } else {
                    label.rsplit('·').next().unwrap_or_default().trim()
                }
            };
            (Line::from(label).width() <= room).then(|| Line::from(format!(" {label} ")).centered())
        });
        if self.has_border() {
            let border = border_style(self.focused);
            let mut block = Block::bordered()
                .border_type(if self.borders == BordersStyle::Rounded {
                    BorderType::Rounded
                } else {
                    BorderType::Plain
                })
                .border_style(border)
                .title_style(reader_caption_style().unwrap_or_else(|| style.dim()));
            let (book, chapter) = section::top_labels(self.inner.width, title, section);
            if !book.is_empty() {
                block = block.title_top(Line::from(format!(" {book} ")));
            }
            if !chapter.is_empty() {
                block = block.title_top(Line::from(format!(" {chapter} ")).right_aligned());
            }
            if let Some(book) = book_progress {
                block = block.title_bottom(book);
            }
            frame.render_widget(block, self.area);
            return;
        }
        if let Some(book) = book_progress {
            let row = Rect::new(
                self.inner.x,
                self.area.bottom().saturating_sub(1),
                self.inner.width,
                self.area.height.min(1),
            );
            let book = book.style(style.dim());
            let right_space = usize::from(row.width).saturating_sub(book.width()) / 2;
            frame.render_widget(book, row);
            // With borders disabled, Zen's single progress row is its only chrome.
            if let Some(section) = section {
                let section = Line::from(format!(" {} ", section.compact_label()))
                    .style(style.dim())
                    .right_aligned();
                if section.width() < right_space {
                    frame.render_widget(section, row);
                }
            }
        }
    }
}

fn reader_caption_style() -> Option<Style> {
    // Titles are drawn over an existing border cell, including its modifiers.
    crate::settings::dialog_reader_theme().map(|theme| theme.frame_style(true).not_dim())
}

pub fn border_style(focused: bool) -> Style {
    if let Some(theme) = crate::settings::dialog_reader_theme() {
        return theme.frame_style(focused);
    }
    let accent = ACTION;
    if focused { accent.bold() } else { accent.dim() }
}
pub fn outline_block(title: &str, focused: bool) -> Block<'_> {
    let panel = block().title(title);
    if focused {
        let accent = border_style(true);
        panel
            .border_style(accent)
            .title_style(reader_caption_style().unwrap_or(accent))
    } else {
        panel
    }
}
pub fn card<'a>(active: bool) -> Block<'a> {
    let tile = block().border_style(border_style(active));
    if active { tile.title(" ▸ ") } else { tile }
}

pub fn card_title(selected: bool) -> Style {
    let title = Style::new().bold();
    if selected { SELECTION.bold() } else { title }
}

/// Depth ink stays inside the active palette; terminal mode keeps live ANSI colors.
pub fn outline_style(base: Style, depth: usize) -> Style {
    let (fg, bg, accent) =
        if let (Some(fg @ Color::Rgb(..)), Some(bg @ Color::Rgb(..))) = (base.fg, base.bg) {
            (fg, bg, fg)
        } else {
            let palette = crate::settings::interface_palette();
            if palette.0 == "terminal" {
                (Color::Reset, Color::Reset, Color::Reset)
            } else {
                palette.colors()
            }
        };
    let mix = |a: Color, b: Color, weight: u16| match (a, b) {
        (Color::Rgb(r, g, b), Color::Rgb(x, y, z)) => {
            let blend = |a: u8, b: u8| {
                ((u16::from(a) * weight + u16::from(b) * (100 - weight)) / 100) as u8
            };
            Color::Rgb(blend(r, x), blend(g, y), blend(b, z))
        }
        _ => a,
    };
    let ink = match (fg, depth % 3) {
        (Color::Rgb(..), 0) => accent,
        (Color::Rgb(..), 1) => {
            if accent == fg {
                mix(fg, bg, 85)
            } else {
                mix(fg, accent, 65)
            }
        }
        (Color::Rgb(..), _) => {
            if accent == fg {
                mix(fg, bg, 70)
            } else {
                fg
            }
        }
        (_, 0) => Color::Green,
        (_, 1) => Color::Magenta,
        _ => Color::Reset,
    };
    base.fg(ink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_selection_stays_readable_after_palette_resolution_and_modal_dimming() {
        use ratatui::{
            Terminal,
            backend::TestBackend,
            style::{Modifier, Stylize},
            widgets::Paragraph,
        };
        fn luminance(color: Color) -> f64 {
            let Color::Rgb(r, g, b) = color else {
                panic!("selection depends on the terminal palette: {color:?}");
            };
            let [r, g, b] = [r, g, b].map(|v| {
                let v = f64::from(v) / 255.;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            0.2126 * r + 0.7152 * g + 0.0722 * b
        }
        for row in 0..crate::theme::Theme::COUNT {
            for dark in [false, true] {
                if row == 0 && dark {
                    continue;
                }
                let theme = crate::theme::Theme::at(row, dark);
                let _context = crate::settings::reader_dialogs(theme);
                for modal in [false, true] {
                    let mut term = Terminal::new(TestBackend::new(40, 12)).unwrap();
                    term.draw(|frame| {
                        frame.render_widget(
                            Paragraph::new("Selected").dim().reversed(),
                            frame.area(),
                        );
                        for x in 0..8 {
                            frame.buffer_mut()[(x, 0)].set_style(TEXT_SELECTION);
                        }
                        crate::settings::paint_book(frame, theme);
                        if modal {
                            crate::overlay::modal(frame, Rect::new(20, 2, 15, 8), |_| {});
                        }
                    })
                    .unwrap();
                    let cell = &term.backend().buffer()[(0, 0)];
                    assert!(!cell.modifier.intersects(Modifier::DIM | Modifier::REVERSED));
                    let fg = luminance(cell.fg);
                    let bg = luminance(cell.bg);
                    let contrast = (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05);
                    assert!(contrast >= 7., "{theme:?} modal={modal}: {contrast}");
                    if std::env::var_os("BOTUI_COLOR_AUDIT").is_some() {
                        println!(
                            "selection_rgb,{theme:?},modal={modal},{:?},{:?},{contrast:.3}",
                            cell.fg, cell.bg
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn zen_reclaims_the_frame_and_disabled_borders_only_reserve_visible_progress() {
        let area = Rect::new(23, 4, 100, 30);
        for borders in [
            BordersStyle::Rounded,
            BordersStyle::Square,
            BordersStyle::None,
        ] {
            let clean = ReadingFrame::with_borders(area, true, false, borders);
            assert_eq!(clean.inner, area);
            let shown = ReadingFrame::with_borders(area, true, true, borders);
            let normal = ReadingFrame::with_borders(area, false, false, borders);
            if borders == BordersStyle::None {
                assert_eq!(normal.inner, area);
                assert_eq!(shown.inner, Rect::new(23, 4, 100, 29));
            } else {
                assert_eq!(shown.inner, Rect::new(24, 5, 98, 28));
                assert_eq!(normal.inner, shown.inner);
            }
        }
        // Even a very small terminal must not produce content outside its pane.
        for width in 0..5 {
            for height in 0..5 {
                let area = Rect::new(3, 2, width, height);
                for zen in [false, true] {
                    let frame = ReadingFrame::with_borders(area, zen, true, BordersStyle::Rounded);
                    assert_eq!(frame.inner.intersection(area), frame.inner);
                }
            }
        }
    }

    #[test]
    fn frame_caption_uses_the_border_without_repainting_highlights_in_the_book() {
        use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};
        let mut term = Terminal::new(TestBackend::new(50, 8)).unwrap();
        let area = Rect::new(10, 0, 40, 8);
        let chrome = ReadingFrame::with_borders(area, true, true, BordersStyle::Rounded);
        let highlight = Style::new().fg(Color::Red).bg(Color::Yellow);
        term.draw(|frame| {
            frame.render_widget(Paragraph::new("selected").style(highlight), chrome.inner);
            chrome.draw(
                frame,
                Style::new().fg(Color::White),
                Some("Северная книга"),
                Some(&progress_label(Some((4, 12)), 25.)),
                Some(SectionPosition {
                    index: 2,
                    total: 4,
                    title: None,
                    percent: Some(25),
                }),
            );
        })
        .unwrap();
        let buffer = term.backend().buffer();
        assert_eq!(buffer[(area.x, area.y)].symbol(), "╭");
        let top: String = (area.x..area.right())
            .map(|x| buffer[(x, area.y)].symbol())
            .collect();
        assert!(top.starts_with("╭ Северная книга ─"));
        assert_eq!(buffer[(area.right() - 1, area.bottom() - 1)].symbol(), "╯");
        assert_eq!(buffer[(chrome.inner.x, chrome.inner.y)].fg, Color::Red);
        assert_eq!(buffer[(chrome.inner.x, chrome.inner.y)].bg, Color::Yellow);
        let bottom: String = (area.x..area.right())
            .map(|x| buffer[(x, 7)].symbol())
            .collect();
        assert!(bottom.contains("4 / 12  ·  25%"));
        assert!(!bottom.contains("Chapter"));
        assert_eq!(buffer[(0, 0)].symbol(), " ");
    }

    #[test]
    fn all_four_reader_edges_follow_focus_and_every_caption_stays_legible() {
        use crate::theme::Theme;
        use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};
        let area = Rect::new(3, 1, 64, 7);
        for theme in [
            Theme::TERMINAL,
            Theme::from_id("sepia-dark"),
            Theme::from_id("sepia-light"),
        ] {
            let _context = crate::settings::reader_dialogs(theme);
            for focused in [false, true] {
                let expected = theme.frame_style(focused);
                for borders in [BordersStyle::Rounded, BordersStyle::Square] {
                    let chrome =
                        ReadingFrame::with_borders(area, false, false, borders).with_focus(focused);
                    let mut term = Terminal::new(TestBackend::new(72, 10)).unwrap();
                    term.draw(|f| {
                        f.render_widget(
                            Paragraph::new("link").style(Style::new().fg(theme.link())),
                            chrome.inner,
                        );
                        chrome.draw(
                            f,
                            theme.style(),
                            Some("Book"),
                            Some("10 / 20 · 50%"),
                            Some(SectionPosition {
                                index: 3,
                                total: 8,
                                title: Some("Chapter"),
                                percent: Some(15),
                            }),
                        );
                    })
                    .unwrap();
                    let buf = term.backend().buffer();
                    let reference = &buf[(area.x, area.y)];
                    assert_eq!(Some(reference.fg), expected.fg);
                    assert_eq!(reference.modifier, expected.add_modifier);
                    for (x, y) in area.positions().map(|p| (p.x, p.y)) {
                        if x == area.x
                            || x + 1 == area.right()
                            || y == area.y
                            || y + 1 == area.bottom()
                        {
                            let cell = &buf[(x, y)];
                            if matches!(
                                cell.symbol(),
                                "─" | "│" | "╭" | "╮" | "╰" | "╯" | "┌" | "┐" | "└" | "┘"
                            ) {
                                assert_eq!(
                                    cell.style(),
                                    reference.style(),
                                    "{x},{y} {} focused={focused}",
                                    cell.symbol()
                                );
                            } else {
                                let caption = theme.frame_style(true);
                                assert_eq!(Some(cell.fg), caption.fg, "{x},{y}");
                                assert_eq!(cell.modifier, caption.add_modifier, "{x},{y}");
                            }
                        }
                    }
                    assert_eq!(buf[(chrome.inner.x, chrome.inner.y)].fg, theme.link());
                }
            }
            let _shell = crate::settings::interface_dialogs();
            assert_eq!(border_style(true), ACTION.bold());
            assert_eq!(border_style(false), ACTION.dim());
        }
    }

    #[test]
    fn frame_separates_chapter_and_book_progress_across_widths() {
        use ratatui::{Terminal, backend::TestBackend};
        let section = SectionPosition {
            index: 8,
            total: 30,
            title: Some("Глава четвертая"),
            percent: Some(40),
        };
        for width in [3, 7, 12, 16, 28, 34, 50, 84] {
            let area = Rect::new(3, 0, width, 6);
            for borders in [BordersStyle::Square, BordersStyle::None] {
                let chrome = ReadingFrame::with_borders(area, true, true, borders);
                let mut term = Terminal::new(TestBackend::new(area.right(), 6)).unwrap();
                let mut book_x = None;
                for info in [None, Some(section)] {
                    term.draw(|frame| {
                        chrome.draw(
                            frame,
                            Style::default(),
                            Some("Северная книга"),
                            Some("79 / 307 · 26%"),
                            info,
                        )
                    })
                    .unwrap();
                    let row = |y| {
                        (area.x..area.right())
                            .map(|x| term.backend().buffer()[(x, y)].symbol())
                            .collect::<String>()
                    };
                    let top = row(0);
                    let bottom = row(5);
                    if let Some(x) = bottom.find("79 / 307") {
                        assert_eq!(*book_x.get_or_insert(x), x);
                    }
                    if borders == BordersStyle::None {
                        assert!(top.trim().is_empty());
                        if width == 84 && info.is_some() {
                            assert!(bottom.trim_end().ends_with("8/30 · 40%"));
                        }
                    } else {
                        assert!(top.starts_with('┌') && top.ends_with('┐'), "{top}");
                        assert!(bottom.starts_with('└') && bottom.ends_with('┘'), "{bottom}");
                        assert!(
                            !bottom.contains("8/30") && !bottom.contains("40%"),
                            "{bottom}"
                        );
                        if info.is_some() && width >= 34 {
                            assert!(top.starts_with("┌ Север"), "{top}");
                            assert!(top.contains("8/30") && top.ends_with("40% ┐"), "{top}");
                        }
                        if width == 84 && info.is_some() {
                            assert!(top.contains("8/30 · Глава четвертая · 40%"));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn top_border_shares_space_between_long_unicode_titles_and_keeps_numbers_whole() {
        let section = SectionPosition {
            index: 8,
            total: 30,
            title: Some("地球科学研究 методика измерений"),
            percent: Some(40),
        };
        let title = Some("長い本の名前 — подробное издание");
        let (left, right) = section::top_labels(40, title, Some(section));
        assert!(left.starts_with("長") && left.ends_with('…'));
        assert!(right.starts_with("8/30 · 地") && right.ends_with("… · 40%"));
        assert!(Line::from(left).width() + Line::from(right).width() + 5 <= 40);
        // There is just enough space for two short ellipsized names and both numbers.
        let (left, right) = section::top_labels(26, title, Some(section));
        assert!(
            left.starts_with("長") && right.contains("地"),
            "{left} | {right}"
        );
        for width in 0..100 {
            let (left, right) = section::top_labels(width, title, Some(section));
            let used = [left, right]
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| Line::from(s.as_str()).width() + 2)
                .sum::<usize>();
            assert!(used <= usize::from(width));
        }
        let unmeasured = SectionPosition {
            percent: None,
            ..section
        };
        assert_eq!(
            section::top_labels(80, None, Some(unmeasured)).1,
            "8/30 · 地球科学研究 методика измерений"
        );
    }
}
