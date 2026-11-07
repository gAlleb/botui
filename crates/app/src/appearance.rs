use ratatui::{
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Padding},
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
}
pub fn divider<'a>() -> Block<'a> {
    if BordersStyle::current() == BordersStyle::None {
        Block::new()
    } else {
        Block::new().borders(Borders::RIGHT)
    }
}
pub fn card<'a>(active: bool) -> Block<'a> {
    let tile = block().border_style(if active {
        Style::new().fg(Color::Cyan).bold()
    } else {
        Style::new().dim()
    });
    if active { tile.title(" ▸ ") } else { tile }
}
