//! Explicit edition overrides. Absent fields continue to follow the global profile.
use super::{PageColors, Settings};
use botui_store::LocalState;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Spacing {
    FromBook,
    BlankLines(u8),
}
impl Spacing {
    pub fn lines(self) -> Option<u8> {
        match self {
            Self::FromBook => None,
            Self::BlankLines(n) => Some(n.min(2)),
        }
    }
}

#[derive(Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct BookSettings {
    pub page_width: Option<u16>,
    pub theme: Option<String>,
    pub reading_layout: Option<String>,
    pub pagination_mode: Option<botui_core::PaginationMode>,
    pub alignment: Option<botui_core::TextAlignment>,
    pub heading_rules: Option<bool>,
    pub hyphenation: Option<bool>,
    pub paragraph_indent: Option<u8>,
    pub paragraph_spacing: Option<Spacing>,
    pub compact_spacing: Option<bool>,
    pub comic_background: Option<String>,
    pub pdf_theme: Option<String>,
    pub pdf_colors: Option<PageColors>,
}

impl BookSettings {
    pub fn load(store: Option<&LocalState>) -> io::Result<Self> {
        Ok(store
            .map(LocalState::book_preferences)
            .transpose()?
            .unwrap_or_default())
    }
    pub fn apply(&self, prefs: &mut Settings) {
        if let Some(v) = &self.comic_background {
            prefs.comic_background = Some(v.clone());
        }
        if let Some(v) = &self.theme {
            prefs.theme = Some(v.clone());
        }
        if let Some(v) = &self.reading_layout {
            prefs.reading_layout = Some(v.clone());
        }
        if let Some(mode) = self.pagination_mode {
            prefs.pagination_mode = mode;
        }
        if let Some(v) = self.alignment {
            prefs.text_alignment = v;
        }
        if let Some(v) = self.heading_rules {
            prefs.heading_rules = v;
        }
        if let Some(v) = self.hyphenation {
            prefs.hyphenation = v;
        }
        if let Some(n) = self.page_width {
            prefs.page_width = Some(if n == 0 { 0 } else { n.clamp(32, 240) });
        }
        if let Some(n) = self.paragraph_indent {
            prefs.paragraph_indent = n.min(8);
        }
        if let Some(s) = self.paragraph_spacing {
            prefs.paragraph_spacing = s.lines();
        }
        if let Some(b) = self.compact_spacing {
            prefs.preserve_compact_spacing = Some(b);
        }
        // Resolve the legacy global mode before overriding its palette.
        if self.pdf_theme.is_some() || self.pdf_colors.is_some() {
            prefs.scan_page_colors = Some(self.pdf_colors.unwrap_or_else(|| prefs.page_colors()));
            if let Some(t) = &self.pdf_theme {
                prefs.scan_theme = Some(t.clone());
            }
        }
    }
    pub fn effective(store: Option<&LocalState>) -> io::Result<Settings> {
        let mut prefs = Settings::load()?;
        Self::load(store)?.apply(&mut prefs);
        Ok(prefs)
    }
}

/// Quick palette changes belong to the open edition. Untouched fields inherit their defaults.
pub fn save_pdf_palette(
    store: Option<&LocalState>,
    theme: crate::theme::Theme,
    colors: PageColors,
    previous_theme: crate::theme::Theme,
    previous_colors: PageColors,
) -> io::Result<Settings> {
    if let Some(store) = store {
        store.update_book_preferences(|p: &mut BookSettings| {
            if theme != previous_theme {
                p.pdf_theme = Some(theme.id());
            }
            if colors != previous_colors {
                p.pdf_colors = Some(colors);
            }
        })?;
        BookSettings::effective(Some(store))
    } else {
        Settings::update(|s| {
            s.scan_page_colors = Some(colors);
            s.scan_theme = Some(theme.id());
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn fields_follow_the_edition_and_inherit_independently() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("book.fb2");
        let renamed = temp.path().join("copy.fb2");
        let dir = temp.path().join("state");
        fs::write(&path, b"edition A")?;
        fs::copy(&path, &renamed)?;
        let a = LocalState::in_directory(&path, &dir, None)?;
        let copy = LocalState::in_directory(&renamed, &dir, None)?;
        a.update_book_preferences(|s: &mut BookSettings| s.page_width = Some(44))?;
        copy.update_book_preferences(|s: &mut BookSettings| {
            s.paragraph_spacing = Some(Spacing::FromBook);
            s.pagination_mode = Some(botui_core::PaginationMode::Chapter);
        })?;
        let global = || Settings {
            page_width: Some(70),
            paragraph_indent: 3,
            paragraph_spacing: Some(2),
            pagination_mode: botui_core::PaginationMode::KeepWord,
            ..Settings::default()
        };
        let effective = |store: &LocalState| -> io::Result<Settings> {
            let mut result = global();
            BookSettings::load(Some(store))?.apply(&mut result);
            Ok(result)
        };
        let ours = effective(&copy)?;
        assert_eq!(ours.pagination_mode, botui_core::PaginationMode::Chapter);
        assert_eq!(
            (
                ours.page_width,
                ours.paragraph_spacing,
                ours.paragraph_indent
            ),
            (Some(44), None, 3)
        );
        fs::write(&path, b"edition B")?;
        let other = LocalState::in_directory(&path, &dir, None)?;
        let theirs = effective(&other)?;
        assert_eq!(theirs.pagination_mode, botui_core::PaginationMode::KeepWord);
        assert_eq!(
            (theirs.page_width, theirs.paragraph_spacing),
            (Some(70), Some(2))
        );
        a.update_book_preferences(|s: &mut BookSettings| s.page_width = None)?;
        assert_eq!(effective(&copy)?.page_width, Some(70));
        assert_eq!(effective(&copy)?.paragraph_spacing, None);
        copy.update_book_preferences(|s: &mut BookSettings| *s = BookSettings::default())?;
        assert_eq!(effective(&copy)?.paragraph_spacing, Some(2));
        assert_eq!(
            effective(&copy)?.pagination_mode,
            botui_core::PaginationMode::KeepWord
        );
        Ok(())
    }

    #[test]
    fn pinning_pdf_paper_inherits_the_legacy_color_mode() {
        let mut global = Settings {
            scan_theme: Some("inverted".into()),
            ..Settings::default()
        };
        let local = BookSettings {
            pdf_theme: Some("sage-light".into()),
            ..BookSettings::default()
        };
        local.apply(&mut global);
        assert_eq!(global.page_theme().id(), "sage-light");
        assert_eq!(global.page_colors(), PageColors::Inverted);
    }
}
