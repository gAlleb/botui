use super::{PARAGRAPH_SPACING, Settings};
use crate::{
    AppTerminal, prompt,
    settings::{
        PageColors,
        book::{BookSettings, Spacing as BookSpacing},
    },
    theme::Theme,
};
use botui_store::LocalState;
use std::io;

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Markdown,
    Text,
    Epub,
    Pdf,
}

#[derive(Clone, Copy)]
enum Field {
    Width,
    Indent,
    Spacing,
    Compact,
    PdfTheme,
    PdfColors,
}
impl Field {
    fn label(self) -> &'static str {
        match self {
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
fn cells(n: u16) -> String {
    format!("{n} cells")
}

pub(super) fn edit(term: &mut AppTerminal, store: &LocalState, kind: Kind) -> io::Result<()> {
    use Field::*;
    let fields: &[Field] = match kind {
        Kind::Markdown => &[Width],
        Kind::Text => &[Width, Indent, Spacing],
        Kind::Epub => &[Width, Indent, Spacing, Compact],
        Kind::Pdf => &[PdfTheme, PdfColors],
    };
    let mut selected = 0;
    loop {
        let g = Settings::load()?;
        let local: BookSettings = store.book_preferences()?;
        let mut rows = Vec::new();
        for field in fields {
            let value = match field {
                Width => scope(
                    local.page_width.map(cells),
                    cells(g.page_width.unwrap_or(78)),
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
                PdfTheme => scope(
                    local
                        .pdf_theme
                        .as_deref()
                        .map(|id| Theme::from_id(id).name()),
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
            Indent | Compact | Spacing | PdfColors => {
                let (current, choices) = match field {
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
                    "Page width · 32–240 cells, blank to use global",
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
                                .filter(|n| (32..=240).contains(n))
                                .ok_or_else(|| {
                                    io::Error::other("Enter 32–240 cells or clear to use global.")
                                })?,
                        )
                    };
                    store.update_book_preferences(|p: &mut BookSettings| p.page_width = width)?;
                }
            }
            PdfTheme => {
                let choice = edit(
                    local.pdf_theme.as_ref().map(|_| 0),
                    &["Choose theme…".into()],
                    term,
                )?;
                let theme = match choice {
                    Some(0) => None,
                    Some(_) => {
                        let initial = local
                            .pdf_theme
                            .as_deref()
                            .map_or_else(|| g.page_theme(), Theme::from_id);
                        let Some(t) = super::edit::choose_theme(
                            term,
                            initial,
                            " PDF reading theme · this book ",
                        )?
                        else {
                            continue;
                        };
                        Some(t.id())
                    }
                    None => continue,
                };
                store.update_book_preferences(|p: &mut BookSettings| p.pdf_theme = theme)?;
            }
        }
    }
}
