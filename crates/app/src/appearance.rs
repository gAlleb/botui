use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Padding},
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

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
    match BordersStyle::current() {
        BordersStyle::Rounded => Block::bordered().border_type(BorderType::Rounded),
        BordersStyle::Square => Block::bordered(),
        BordersStyle::None => Block::new().padding(Padding::horizontal(1)),
    }
    .border_style(border_style(false))
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
}

impl ReadingFrame {
    pub fn new(area: Rect, zen: bool, zen_progress: bool) -> Self {
        Self::with_borders(area, zen, zen_progress, BordersStyle::current())
    }

    fn with_borders(area: Rect, zen: bool, zen_progress: bool, style: BordersStyle) -> Self {
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
        }
    }

    pub fn draw(&self, frame: &mut ratatui::Frame, style: Style, caption: Option<String>) {
        if self.borders != BordersStyle::None {
            let mut block = Block::bordered()
                .border_type(if self.borders == BordersStyle::Rounded {
                    BorderType::Rounded
                } else {
                    BorderType::Plain
                })
                .border_style(border_style(false))
                .title_style(style.dim());
            if let Some(label) = caption {
                block = block.title_bottom(Line::from(format!(" {label} ")).centered());
            }
            frame.render_widget(block, self.area);
        } else if let Some(label) = caption {
            frame.render_widget(
                ratatui::widgets::Paragraph::new(label)
                    .centered()
                    .style(style.dim()),
                Rect::new(
                    self.area.x,
                    self.area.bottom().saturating_sub(1),
                    self.area.width,
                    self.area.height.min(1),
                ),
            );
        }
    }
}

pub fn border_style(focused: bool) -> Style {
    let accent = Style::new().fg(Color::Cyan);
    if focused { accent.bold() } else { accent.dim() }
}
pub fn card<'a>(active: bool) -> Block<'a> {
    let tile = block().border_style(border_style(active));
    if active { tile.title(" ▸ ") } else { tile }
}

pub fn card_title(selected: bool) -> Style {
    let title = Style::new().bold();
    if selected {
        title.fg(Color::Cyan).reversed()
    } else {
        title
    }
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
                Some(progress_label(Some((4, 12)), 25.)),
            );
        })
        .unwrap();
        let buffer = term.backend().buffer();
        assert_eq!(buffer[(area.x, area.y)].symbol(), "╭");
        assert_eq!(buffer[(area.right() - 1, area.bottom() - 1)].symbol(), "╯");
        assert_eq!(buffer[(chrome.inner.x, chrome.inner.y)].fg, Color::Red);
        assert_eq!(buffer[(chrome.inner.x, chrome.inner.y)].bg, Color::Yellow);
        let bottom: String = (area.x..area.right())
            .map(|x| buffer[(x, 7)].symbol())
            .collect();
        assert!(bottom.contains("4 / 12  ·  25%"));
        assert_eq!(buffer[(0, 0)].symbol(), " ");
    }
}
