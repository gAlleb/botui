use ratatui::{
    style::{Color, Style},
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
pub fn zen_progress(
    frame: &mut ratatui::Frame,
    pages: Option<(usize, usize)>,
    percent: f64,
    style: Style,
) {
    let screen = frame.area();
    let row =
        ratatui::layout::Rect::new(screen.x, screen.bottom().saturating_sub(1), screen.width, 1);
    let label = pages.map_or_else(
        || format!("{percent:.0}%"),
        |(page, total)| format!("{page} / {total}  ·  {percent:.0}%"),
    );
    frame.render_widget(
        ratatui::widgets::Paragraph::new(label)
            .centered()
            .style(style.dim()),
        row,
    );
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
