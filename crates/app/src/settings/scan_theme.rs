use super::Settings;
use crate::theme::Theme;
use botui_store::ScanAppearance;

impl Settings {
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
