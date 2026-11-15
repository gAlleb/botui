//! The illustration dialog uses the same fitted viewport and zoom steps as PDF pages.
use crate::cover::{Crop, Picture};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Rect, Size},
    style::Stylize,
    text::Line,
    widgets::Paragraph,
};
use ratatui_image::picker::Picker;

pub(super) struct Illustration {
    crop: Crop,
    buttons: Vec<(Rect, KeyCode)>,
}

impl Default for Illustration {
    fn default() -> Self {
        Self {
            crop: Crop {
                y: 128,
                ..Crop::default()
            },
            buttons: Vec::new(),
        }
    }
}

impl Illustration {
    pub fn draw(&mut self, f: &mut Frame, inner: Rect, image: &Picture, picker: &Picker) {
        let body = self.toolbar(f, inner);
        if self.crop.percent <= 100 {
            image.draw(f, self.crop.fit(body, true), picker);
            return;
        }
        let font = picker.font_size();
        let placements = crate::scan::layout::pages(
            body,
            Size::new(font.width, font.height),
            &[image.dimensions()],
            0,
            self.crop,
            false,
        );
        if let [Some(p)] = placements.as_slice() {
            image.draw_window(f, p.screen, picker, p.window);
        }
    }

    fn toolbar(&mut self, f: &mut Frame, inner: Rect) -> Rect {
        use KeyCode::{Char, Down, Esc, Left, Right, Up};
        let percent = format!("{}%", self.crop.percent);
        let open = if inner.width < 30 {
            "[ Open · o ]"
        } else {
            "[ Open externally · o ]"
        };
        let mut items = [
            ("[ Back · Esc ]", Some(Esc)),
            (open, Some(Char('o'))),
            ("Zoom", None),
            ("[−]", Some(Char('-'))),
            (&percent, None),
            ("[+]", Some(Char('+'))),
            ("Pan", None),
            ("[←]", Some(Left)),
            ("[↑]", Some(Up)),
            ("[↓]", Some(Down)),
            ("[→]", Some(Right)),
        ]
        .map(|(text, key)| (text, key, Rect::default()));
        let mut x = 0u16;
        let mut y = 0;
        for (text, _, rect) in &mut items {
            let width = (Line::raw(*text).width() as u16).min(inner.width);
            if x > 0 && x + width > inner.width {
                x = 0;
                y += 1;
            }
            *rect = Rect::new(inner.x + x, y, width, 1);
            x = x.saturating_add(width).saturating_add(1);
        }

        let body = Rect {
            height: inner.height.saturating_sub(y + 1),
            ..inner
        };
        self.buttons.clear();
        for (text, key, mut rect) in items {
            rect.y += body.bottom();
            rect = rect.intersection(inner);
            if let Some(key) = key {
                self.buttons.push((rect, key));
            }
            f.render_widget(Paragraph::new(text).cyan(), rect);
        }
        body
    }

    pub fn key(&self, event: &Event) -> Option<KeyCode> {
        match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => Some(k.code),
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => self
                .buttons
                .iter()
                .find(|(rect, _)| rect.contains((m.column, m.row).into()))
                .map(|&(_, key)| key),
            _ => None,
        }
    }

    pub fn adjust(&mut self, key: KeyCode) {
        let crop = &mut self.crop;
        match key {
            KeyCode::Char('+' | '=') => crop.step(true),
            KeyCode::Char('-') => crop.step(false),
            KeyCode::Left if crop.percent > 100 => crop.x = crop.x.saturating_sub(64),
            KeyCode::Right if crop.percent > 100 => crop.x = crop.x.saturating_add(64),
            KeyCode::Up if crop.percent > 100 => crop.y = crop.y.saturating_sub(64),
            KeyCode::Down if crop.percent > 100 => crop.y = crop.y.saturating_add(64),
            _ => return,
        }
        if crop.percent <= 100 {
            crop.x = 128;
            crop.y = 128;
        }
    }
}
