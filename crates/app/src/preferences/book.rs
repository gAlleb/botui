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
pub(super) enum Field {
    Reset,
    Zen,
    ZenProgress,
    Wheel,
    Gap,
    Printed,
    Images,
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
    fn value(self, local: &BookSettings, g: &Settings) -> String {
        use Field::*;
        match self {
            Zen => scope(local.zen.map(|v| view(v).into()), view(g.zen).into()),
            ZenProgress => scope(
                local.zen_progress.map(|v| toggle(v).into()),
                toggle(g.zen_progress).into(),
            ),
            Wheel => scope(
                local.scroll_lines.map(|v| v.to_string()),
                g.scroll_lines.unwrap_or(1).to_string(),
            ),
            Gap => scope(
                local.column_gap.map(cells),
                cells(g.column_gap.unwrap_or(4)),
            ),
            Printed => scope(
                local.preserve_printed_lines.map(|v| printed(v).into()),
                printed(g.preserve_printed_lines).into(),
            ),
            Images => scope(
                local.hide_inline_images.map(|v| images(v).into()),
                images(g.hide_inline_images).into(),
            ),
            Reset => String::new(),
            Theme => scope(
                local
                    .theme
                    .as_deref()
                    .map(|v| crate::theme::Theme::from_id(v).name()),
                crate::theme::Theme::from_id(g.theme.as_deref().unwrap_or("terminal")).name(),
            ),
            Layout => scope(
                local.reading_layout.clone(),
                g.reading_layout.clone().unwrap_or_else(|| "auto".into()),
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
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Zen => "Reader view",
            Self::ZenProgress => "Zen frame & progress",
            Self::Wheel => "Mouse wheel step",
            Self::Reset => "Use global settings for this book",
            Self::Gap => "Column gap",
            Self::Printed => "Printed line breaks",
            Self::Images => "Illustrations",
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

pub(super) fn rows(
    kind: Kind,
    local: &BookSettings,
    g: &Settings,
    global: Vec<super::SettingRow>,
) -> Vec<(super::Item, super::SettingRow)> {
    use Field::*;
    let fields: &[Field] = match kind {
        Kind::Markdown => &[
            Layout, Pagination, Theme, Width, Gap, Alignment, Rules, Images,
        ],
        Kind::Text => &[
            Layout, Pagination, Theme, Width, Gap, Alignment, Rules, Hyphens, Indent, Spacing,
            Printed, Images,
        ],
        Kind::Epub => &[
            Layout, Pagination, Theme, Width, Gap, Alignment, Rules, Hyphens, Indent, Spacing,
            Compact, Printed, Images,
        ],
        Kind::Comic => &[Layout, ComicTheme],
        Kind::Pdf => &[Layout, PdfTheme, PdfColors],
    };
    let mut rows = Vec::new();
    for &field in fields.iter().chain([&Zen, &ZenProgress, &Wheel, &Reset]) {
        let value = field.value(local, g);
        rows.push((
            super::Item::Book(field),
            super::SettingRow {
                label: format!("{}\t{value}", field.label()),
                section: matches!(field, Zen).then_some("Reading controls"),
                hint: if matches!(field, Reset) {
                    "Restore all inherited reading settings for this edition."
                } else {
                    "Use global follows the default set in the library."
                },
            },
        ));
    }
    // SyncTeX edits the external editor configuration shared by all PDFs.
    for (index, row) in global.into_iter().enumerate() {
        if matches!(kind, Kind::Pdf) && index >= 21 {
            rows.push((super::Item::Global(index), row));
        }
    }
    rows
}
fn view(value: bool) -> &'static str {
    if value { "Zen" } else { "Contents sidebar" }
}
fn printed(value: bool) -> &'static str {
    if value { "Preserve" } else { "Reflow" }
}
fn images(value: bool) -> &'static str {
    if value { "Viewer only" } else { "In text" }
}

pub(super) fn edit(
    term: &mut AppTerminal,
    store: &LocalState,
    field: Field,
    g: &Settings,
) -> io::Result<()> {
    use Field::*;
    let local: BookSettings = store.book_preferences()?;
    let edit = |local: Option<usize>, labels: &[String], term: &mut AppTerminal| {
        let mut choices = vec![field.value(&BookSettings::default(), g)];
        choices.extend_from_slice(labels);
        prompt::choose(term, field.label(), &choices, local.map_or(0, |v| v + 1))
    };
    match field {
        Reset => {
            store.update_book_preferences(|p: &mut BookSettings| *p = BookSettings::default())?
        }
        Zen | ZenProgress | Gap | Printed | Images | Layout | Pagination | Alignment | Rules
        | Hyphens | Indent | Compact | Spacing | PdfColors => {
            let (current, choices) = match field {
                Zen => (
                    local.zen.map(usize::from),
                    [view(false), view(true)].map(String::from).to_vec(),
                ),
                ZenProgress => (
                    local.zen_progress.map(usize::from),
                    ["Off", "On"].map(String::from).to_vec(),
                ),
                Gap => (
                    local.column_gap.map(usize::from),
                    (0..=32).map(cells).collect(),
                ),
                Printed => (
                    local.preserve_printed_lines.map(usize::from),
                    [printed(false), printed(true)].map(String::from).to_vec(),
                ),
                Images => (
                    local.hide_inline_images.map(usize::from),
                    [images(false), images(true)].map(String::from).to_vec(),
                ),
                Layout => (
                    local
                        .reading_layout
                        .as_ref()
                        .and_then(|v| ["single", "spread", "scroll"].iter().position(|s| s == v)),
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
                    Zen => p.zen = value.map(|i| i == 1),
                    ZenProgress => p.zen_progress = value.map(|i| i == 1),
                    Gap => p.column_gap = value.map(|i| i as u16),
                    Printed => p.preserve_printed_lines = value.map(|i| i == 1),
                    Images => p.hide_inline_images = value.map(|i| i == 1),
                    Layout => {
                        p.reading_layout = value.map(|i| ["single", "spread", "scroll"][i].into())
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
        Width | Wheel => {
            let width = matches!(field, Width);
            let (title, current) = if width {
                (
                    "Page width · 0 = full, 32–240 cells, blank to use global",
                    local.page_width,
                )
            } else {
                (
                    "Mouse wheel step · 1–12 lines, blank to use global",
                    local.scroll_lines.map(u16::from),
                )
            };
            if let Some(s) = prompt::edit(
                term,
                title,
                "[ Save ↵ ]",
                &current.map(|n| n.to_string()).unwrap_or_default(),
                |_| {},
            )? {
                let value = if s.trim().is_empty() {
                    None
                } else {
                    Some(
                        s.trim()
                            .parse::<u16>()
                            .ok()
                            .filter(|n| {
                                if width {
                                    *n == 0 || (32..=240).contains(n)
                                } else {
                                    (1..=12).contains(n)
                                }
                            })
                            .ok_or_else(|| io::Error::other(title))?,
                    )
                };
                store.update_book_preferences(|p: &mut BookSettings| {
                    if width {
                        p.page_width = value;
                    } else {
                        p.scroll_lines = value.map(|n| n as u8);
                    }
                })?;
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
                    let Some(t) = super::edit::choose_theme(term, initial, field.label())? else {
                        return Ok(());
                    };
                    Some(t.id())
                }
                None => return Ok(()),
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
    Ok(())
}
