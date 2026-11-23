use super::{PAGINATION_MODES, PARAGRAPH_SPACING, Settings, pagination_label};
use crate::{
    AppTerminal, prompt,
    settings::{
        PageColors,
        book::{BookSettings, Spacing as BookSpacing},
    },
};
use botui_store::LocalState;
use std::io;

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Markdown,
    Text,
    Epub,
    Pdf,
    Comic,
}

#[derive(Clone, Copy)]
enum Field {
    Width,
    Indent,
    Spacing,
    Compact,
    Theme,
    Layout,
    Pagination,
    Alignment,
    Rules,
    Hyphens,
    ComicTheme,
    PdfTheme,
    PdfColors,
}
impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::ComicTheme => "Comic background",
            Self::Theme => "Reading theme",
            Self::Layout => "Reading layout",
            Self::Pagination => "Pagination",
            Self::Alignment => "Text alignment",
            Self::Rules => "Heading rules",
            Self::Hyphens => "Word hyphenation (EN/RU)",
            Self::Width => "Page width",
            Self::Indent => "First-line indent",
            Self::Spacing => "Paragraph spacing",
            Self::Compact => "Compact blocks",
            Self::PdfTheme => "PDF reading theme",
            Self::PdfColors => "PDF page colors",
        }
    }
}

fn scope(local: Option<String>, global: String) -> String {
    local.unwrap_or_else(|| format!("Use global · {global}"))
}
fn spacing_label(lines: Option<u8>) -> &'static str {
    PARAGRAPH_SPACING[lines.map_or(0, |n| 1 + usize::from(n.min(2)))]
}
fn toggle(b: bool) -> &'static str {
    if b { "On" } else { "Off" }
}
pub(super) fn width_label(n: u16) -> String {
    if n == 0 {
        "Full width".into()
    } else {
        cells(n)
    }
}
fn cells(n: u16) -> String {
    format!("{n} cells")
}

pub(super) fn edit(term: &mut AppTerminal, store: &LocalState, kind: Kind) -> io::Result<()> {
    use Field::*;
    let fields: &[Field] = match kind {
        Kind::Markdown => &[Width, Theme, Layout, Pagination, Alignment, Rules],
        Kind::Text => &[
            Width, Indent, Spacing, Theme, Layout, Pagination, Alignment, Rules, Hyphens,
        ],
        Kind::Epub => &[
            Width, Indent, Spacing, Compact, Theme, Layout, Pagination, Alignment, Rules, Hyphens,
        ],
        Kind::Comic => &[ComicTheme, Layout],
        Kind::Pdf => &[PdfTheme, PdfColors, Layout],
    };
    let mut selected = 0;
    loop {
        let g = Settings::load()?;
        let local: BookSettings = store.book_preferences()?;
        let mut rows = Vec::new();
        for field in fields {
            let value = match field {
                Theme => scope(
                    local
                        .theme
                        .as_deref()
                        .map(|v| crate::theme::Theme::from_id(v).name()),
                    crate::theme::Theme::from_id(g.theme.as_deref().unwrap_or("terminal")).name(),
                ),
                Layout => scope(
                    local.reading_layout.clone(),
                    g.reading_layout.clone().unwrap_or_else(|| "spread".into()),
                ),
                Pagination => scope(
                    local
                        .pagination_mode
                        .map(|mode| pagination_label(mode).into()),
                    pagination_label(g.pagination_mode).into(),
                ),
                Alignment => scope(
                    local.alignment.map(|v| v.label().into()),
                    g.text_alignment.label().into(),
                ),
                Rules => scope(
                    local.heading_rules.map(|v| toggle(v).into()),
                    toggle(g.heading_rules).into(),
                ),
                Hyphens => scope(
                    local.hyphenation.map(|v| toggle(v).into()),
                    toggle(g.hyphenation).into(),
                ),
                Width => scope(
                    local.page_width.map(width_label),
                    width_label(g.page_width.unwrap_or_default()),
                ),
                Indent => scope(
                    local.paragraph_indent.map(|n| cells(n.into())),
                    cells(g.paragraph_indent.into()),
                ),
                Spacing => scope(
                    local
                        .paragraph_spacing
                        .map(|s| spacing_label(s.lines()).into()),
                    spacing_label(g.paragraph_spacing).into(),
                ),
                Compact => scope(
                    local.compact_spacing.map(|b| toggle(b).into()),
                    toggle(g.compact_spacing()).into(),
                ),
                ComicTheme => scope(
                    local
                        .comic_background
                        .as_deref()
                        .map(|t| crate::theme::Theme::from_id(t).name()),
                    g.comic_theme().name(),
                ),
                PdfTheme => scope(
                    local
                        .pdf_theme
                        .as_deref()
                        .map(|id| crate::theme::Theme::from_id(id).name()),
                    g.page_theme().name(),
                ),
                PdfColors => scope(
                    local.pdf_colors.map(|c| c.label().into()),
                    g.page_colors().label().into(),
                ),
            };
            rows.push(format!("{}   {value}", field.label()));
        }
        rows.push("Use global settings for this book".into());
        let Some(i) = prompt::choose(term, "Settings for this book", &rows, selected)? else {
            return Ok(());
        };
        selected = i;
        let Some(&field) = fields.get(i) else {
            store.update_book_preferences(|p: &mut BookSettings| *p = BookSettings::default())?;
            continue;
        };
        let edit = |local: Option<usize>, labels: &[String], term: &mut AppTerminal| {
            let mut choices = vec!["Use global".into()];
            choices.extend_from_slice(labels);
            prompt::choose(term, field.label(), &choices, local.map_or(0, |v| v + 1))
        };
        match field {
            Layout | Pagination | Alignment | Rules | Hyphens | Indent | Compact | Spacing
            | PdfColors => {
                let (current, choices) = match field {
                    Layout => (
                        local.reading_layout.as_ref().and_then(|v| {
                            ["single", "spread", "scroll"].iter().position(|s| s == v)
                        }),
                        ["Single", "Spread", "Scroll"].map(String::from).to_vec(),
                    ),
                    Pagination => (
                        local.pagination_mode.map(|v| v as usize),
                        PAGINATION_MODES
                            .map(|v| pagination_label(v).into())
                            .to_vec(),
                    ),
                    Alignment => (
                        local.alignment.map(|v| v as usize),
                        botui_core::TextAlignment::ALL
                            .map(|v| v.label().to_owned())
                            .to_vec(),
                    ),
                    Rules | Hyphens => (
                        (if matches!(field, Rules) {
                            local.heading_rules
                        } else {
                            local.hyphenation
                        })
                        .map(usize::from),
                        vec!["Off".into(), "On".into()],
                    ),
                    Indent => (
                        local.paragraph_indent.map(usize::from),
                        (0..=8).map(cells).collect(),
                    ),
                    Compact => (
                        local.compact_spacing.map(usize::from),
                        vec!["Off".into(), "On".into()],
                    ),
                    Spacing => (
                        local
                            .paragraph_spacing
                            .map(|s| s.lines().map_or(0, |n| usize::from(n) + 1)),
                        PARAGRAPH_SPACING.map(String::from).to_vec(),
                    ),
                    _ => (
                        local.pdf_colors.map(|c| c as usize),
                        PageColors::ALL.map(|c| c.label().into()).to_vec(),
                    ),
                };
                if let Some(index) = edit(current, &choices, term)? {
                    let value = index.checked_sub(1);
                    store.update_book_preferences(|p: &mut BookSettings| match field {
                        Layout => {
                            p.reading_layout =
                                value.map(|i| ["single", "spread", "scroll"][i].into())
                        }
                        Alignment => p.alignment = value.map(|i| botui_core::TextAlignment::ALL[i]),
                        Pagination => p.pagination_mode = value.map(|i| PAGINATION_MODES[i]),
                        Rules => p.heading_rules = value.map(|i| i == 1),
                        Hyphens => p.hyphenation = value.map(|i| i == 1),
                        Indent => p.paragraph_indent = value.map(|v| v as u8),
                        Compact => p.compact_spacing = value.map(|v| v == 1),
                        Spacing => {
                            p.paragraph_spacing = value.map(|v| {
                                if v == 0 {
                                    BookSpacing::FromBook
                                } else {
                                    BookSpacing::BlankLines((v - 1) as u8)
                                }
                            })
                        }
                        _ => p.pdf_colors = value.map(|v| PageColors::ALL[v]),
                    })?;
                }
            }
            Width => {
                if let Some(s) = prompt::edit(
                    term,
                    "Page width · 0 = full, 32–240 cells, blank to use global",
                    "[ Save ↵ ]",
                    &local.page_width.map(|n| n.to_string()).unwrap_or_default(),
                    |_| {},
                )? {
                    let width = if s.trim().is_empty() {
                        None
                    } else {
                        Some(
                            s.trim()
                                .parse::<u16>()
                                .ok()
                                .filter(|n| *n == 0 || (32..=240).contains(n))
                                .ok_or_else(|| {
                                    io::Error::other("Enter 0 for full width, 32–240 cells or clear to use global.")
                                })?,
                        )
                    };
                    store.update_book_preferences(|p: &mut BookSettings| p.page_width = width)?;
                }
            }
            PdfTheme | Theme | ComicTheme => {
                let pdf = matches!(field, PdfTheme);
                let current = match field {
                    PdfTheme => &local.pdf_theme,
                    ComicTheme => &local.comic_background,
                    _ => &local.theme,
                };
                let choice = edit(current.as_ref().map(|_| 0), &["Choose theme…".into()], term)?;
                let theme = match choice {
                    Some(0) => None,
                    Some(_) => {
                        let initial = current.as_deref().map_or_else(
                            || {
                                if pdf {
                                    g.page_theme()
                                } else if matches!(field, ComicTheme) {
                                    g.comic_theme()
                                } else {
                                    crate::theme::Theme::from_id(
                                        g.theme.as_deref().unwrap_or("terminal"),
                                    )
                                }
                            },
                            crate::theme::Theme::from_id,
                        );
                        let Some(t) = super::edit::choose_theme(term, initial, field.label())?
                        else {
                            continue;
                        };
                        Some(t.id())
                    }
                    None => continue,
                };
                store.update_book_preferences(|p: &mut BookSettings| {
                    if pdf {
                        p.pdf_theme = theme
                    } else if matches!(field, ComicTheme) {
                        p.comic_background = theme;
                    } else {
                        p.theme = theme
                    }
                })?;
            }
        }
    }
}
