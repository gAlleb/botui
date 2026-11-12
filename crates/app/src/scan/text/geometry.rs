use ratatui::layout::{Position, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct View {
    pub page: u32,
    pub screen: Rect,
    /// Visible rectangle in the rotated page (fractions of full width/height).
    pub window: [f32; 4],
    pub turns: u8,
}

pub(in crate::scan) fn rotate(mut r: [f32; 4], turns: u8) -> [f32; 4] {
    for _ in 0..turns % 4 {
        r = [1. - r[3], r[0], 1. - r[1], r[2]];
    }
    r
}
impl View {
    pub fn point(&self, p: Position) -> Option<(f32, f32)> {
        if !self.screen.contains(p) {
            return None;
        }
        let mut x = self.window[0]
            + (f32::from(p.x - self.screen.x) + 0.5) / f32::from(self.screen.width)
                * (self.window[2] - self.window[0]);
        let mut y = self.window[1]
            + (f32::from(p.y - self.screen.y) + 0.5) / f32::from(self.screen.height)
                * (self.window[3] - self.window[1]);
        for _ in 0..self.turns % 4 {
            (x, y) = (y, 1. - x);
        }
        Some((x, y))
    }
    pub fn rect(&self, original: [f32; 4]) -> Option<Rect> {
        self.rotated_rect(rotate(original, self.turns))
    }
    pub fn rotated_rect(&self, r: [f32; 4]) -> Option<Rect> {
        let b = [
            r[0].max(self.window[0]),
            r[1].max(self.window[1]),
            r[2].min(self.window[2]),
            r[3].min(self.window[3]),
        ];
        if b[0] >= b[2] || b[1] >= b[3] {
            return None;
        }
        let x = |v| {
            (v - self.window[0]) / (self.window[2] - self.window[0]) * f32::from(self.screen.width)
        };
        let y = |v| {
            (v - self.window[1]) / (self.window[3] - self.window[1]) * f32::from(self.screen.height)
        };
        let left = x(b[0]).floor().max(0.) as u16;
        let top = y(b[1]).floor().max(0.) as u16;
        let right = (x(b[2]).ceil() as u16).min(self.screen.width);
        let bottom = (y(b[3]).ceil() as u16).min(self.screen.height);
        Some(Rect::new(
            self.screen.x + left,
            self.screen.y + top,
            right.saturating_sub(left),
            bottom.saturating_sub(top),
        ))
    }
    pub fn window_for(&self, r: Rect) -> [f32; 4] {
        let x = |v| {
            self.window[0]
                + f32::from(v - self.screen.x) / f32::from(self.screen.width)
                    * (self.window[2] - self.window[0])
        };
        let y = |v| {
            self.window[1]
                + f32::from(v - self.screen.y) / f32::from(self.screen.height)
                    * (self.window[3] - self.window[1])
        };
        [x(r.x), y(r.y), x(r.right()), y(r.bottom())]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_zoomed_and_scrolled_words_map_to_the_same_source() {
        let original = [0.31, 0.42, 0.36, 0.46];
        for turns in 0..4 {
            let b = rotate(original, turns);
            let view = View {
                page: 2,
                screen: Rect::new(17, 3, 160, 80),
                window: [b[0] - 0.05, b[1] - 0.05, b[2] + 0.05, b[3] + 0.05],
                turns,
            };
            let r = view.rect(original).unwrap();
            let (x, y) = view
                .point(Position::new(r.x + r.width / 2, r.y + r.height / 2))
                .unwrap();
            assert!((original[0]..original[2]).contains(&x));
            assert!((original[1]..original[3]).contains(&y));
            assert!(view.rect([0., 0., 0.01, 0.01]).is_none());
        }
    }
}
