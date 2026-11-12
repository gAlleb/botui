use super::Settings;
use crate::theme::Theme;
use botui_store::ScanAppearance;

impl Settings {
    pub fn comic_theme(&self) -> Theme {
        Theme::from_id(self.comic_background.as_deref().unwrap_or("terminal"))
    }
    pub fn fixed_theme(&self, comic: bool) -> Theme {
        if comic {
            self.comic_theme()
        } else {
            self.page_theme()
        }
    }
    pub fn apply_fixed_theme(&self, appearance: &mut ScanAppearance, comic: bool) {
        if comic {
            // Old saved color flags and PDF settings must never recolor artwork.
            appearance.inverted = false;
            appearance.terminal_paper = false;
        } else {
            self.apply_page_theme(appearance);
        }
    }
    pub fn page_theme(&self) -> Theme {
        Theme::from_id(self.scan_theme.as_deref().unwrap_or("original"))
    }
    pub fn page_theme_label(&self) -> String {
        match self.scan_theme.as_deref().unwrap_or("original") {
            "original" => "Original".into(),
            "inverted" => "Inverted".into(),
            _ => self.page_theme().name(),
        }
    }
    pub fn apply_page_theme(&self, appearance: &mut ScanAppearance) {
        let id = self.scan_theme.as_deref().unwrap_or("original");
        appearance.inverted = id == "inverted";
        appearance.terminal_paper = !matches!(id, "original" | "inverted");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comic_background_never_recolors_artwork_or_changes_pdf_preference() {
        for pdf in ["inverted", "terminal", "sepia-dark"] {
            let s = Settings {
                scan_theme: Some(pdf.into()),
                comic_background: Some("sage-light".into()),
                ..Default::default()
            };
            let mut a = ScanAppearance {
                quarter_turns: 3,
                inverted: true,
                terminal_paper: true,
            };
            s.apply_fixed_theme(&mut a, true);
            assert_eq!(
                (a.inverted, a.terminal_paper, a.quarter_turns),
                (false, false, 3)
            );
            assert_eq!(s.fixed_theme(true).id(), "sage-light");
            s.apply_fixed_theme(&mut a, false);
            assert_eq!(a.inverted, pdf == "inverted");
            assert_eq!(a.terminal_paper, pdf != "inverted");
        }
        assert!(super::super::decode("comic_background = 'inverted'").is_err());
    }
    #[test]
    fn page_palette_is_independent_and_overrides_old_flags_but_keeps_rotation() {
        let mut s = Settings {
            theme: Some("ember-dark".into()),
            ..Default::default()
        };
        let mut a = ScanAppearance {
            quarter_turns: 3,
            inverted: true,
            terminal_paper: true,
        };
        for (id, inverted, paper) in [
            ("original", false, false),
            ("inverted", true, false),
            ("sage-light", false, true),
        ] {
            s.scan_theme = Some(id.into());
            s.apply_page_theme(&mut a);
            assert_eq!(
                (a.inverted, a.terminal_paper, a.quarter_turns),
                (inverted, paper, 3)
            );
            assert_eq!(s.theme.as_deref(), Some("ember-dark"));
        }
        assert_eq!(s.page_theme().id(), "sage-light");
        assert!(super::super::decode("scan_theme = 'unknown'").is_err());
    }
}
