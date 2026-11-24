//! Keep action labels and their mouse targets whole, even in a narrow terminal.
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph},
};
use std::time::{Duration, Instant};

pub(crate) fn last_row(screen: Rect) -> Rect {
    Rect::new(
        screen.x,
        screen.bottom().saturating_sub(1).max(screen.y),
        screen.width,
        screen.height.min(1),
    )
}

pub(crate) fn reading_area(book: Rect, screen: Rect, full_height: bool) -> Rect {
    if full_height {
        Rect::new(book.x, screen.y, book.width, screen.height)
    } else {
        above_status(book, screen)
    }
}

pub(crate) fn above_status(mut book: Rect, screen: Rect) -> Rect {
    if screen.height > 1 {
        book.height = book.height.min(last_row(screen).y.saturating_sub(book.y));
    }
    book
}

#[derive(Default)]
pub(crate) struct ActionRow {
    offset: usize,
    more: Rect,
    next: usize,
    feedback: Option<(Instant, &'static str)>,
}

impl ActionRow {
    pub fn notify(&mut self, text: &'static str) {
        self.feedback = Some((Instant::now() + Duration::from_secs(2), text));
    }
    pub fn timeout(&self) -> Option<Duration> {
        self.feedback
            .map(|(end, _)| end.saturating_duration_since(Instant::now()))
    }
    pub fn expire(&mut self) -> bool {
        self.feedback
            .take_if(|(end, _)| *end <= Instant::now())
            .is_some()
    }
    pub fn feedback(&self) -> Option<&'static str> {
        self.feedback.map(|(_, text)| text)
    }
    pub fn advance(&mut self, at: Position) -> bool {
        if self.more.contains(at) {
            self.offset = self.next;
            true
        } else {
            false
        }
    }
    pub fn draw<T: Copy>(
        &mut self,
        f: &mut Frame,
        row: Rect,
        style: Style,
        message: Option<&str>,
        actions: &[(&str, Option<T>)],
        buttons: &mut Vec<(Rect, T)>,
    ) {
        self.more = Rect::default();
        if row.is_empty() {
            return;
        }
        f.render_widget(Clear, row);
        if let Some(text) = self.feedback() {
            f.render_widget(
                Paragraph::new(text.replace('\n', " · ")).style(style.italic()),
                row,
            );
            return;
        }
        let message = message.map(|s| s.replace('\n', " · "));
        let items = message
            .as_deref()
            .map(|text| (text, None))
            .into_iter()
            .chain(actions.iter().copied());
        let needed: usize = items.clone().map(|(s, _)| Line::raw(s).width() + 1).sum();
        if self.offset >= actions.len() + usize::from(message.is_some()) {
            self.offset = 0;
        }
        let overflow = needed.saturating_sub(1) > usize::from(row.width);
        let mut right = row.right();
        if overflow && row.width >= 4 {
            right -= 3;
            self.more = Rect::new(right, row.y, 3, 1);
            f.render_widget(Paragraph::new("[›]").style(style.bold()), self.more);
        } else {
            self.offset = 0;
        }
        let mut x = row.x;
        self.next = 0;
        for (index, (label, action)) in items.enumerate().skip(self.offset) {
            let available = right.saturating_sub(x);
            let width = Line::raw(label).width() as u16;
            if available == 0 || (x > row.x && width > available) {
                self.next = index;
                break;
            }
            let rect = Rect::new(x, row.y, width.min(available), 1);
            let item_style = if index == 0 && message.is_some() {
                style.italic()
            } else {
                style.bold()
            };
            f.render_widget(Paragraph::new(label).style(item_style), rect);
            if let Some(action) = action {
                buttons.push((rect, action));
            }
            x = rect.right().saturating_add(1);
        }
    }
}

pub(crate) fn layout(area: Rect, labels: &[&str]) -> (Vec<Rect>, u16) {
    let mut rects = vec![Rect::default(); labels.len()];
    let (mut x, mut row) = (0, 0);
    let mut height = 0;
    for (rect, label) in rects.iter_mut().zip(labels) {
        let width = Line::raw(*label).width().min(u16::MAX as usize) as u16;
        if width > area.width || width == 0 {
            continue;
        }
        if x + width > area.width {
            x = 0;
            row += 1;
        }
        if row >= area.height {
            break;
        }
        *rect = Rect::new(area.x + x, row, width, 1);
        x += width + 2;
        height = row + 1;
    }
    for rect in &mut rects {
        if !rect.is_empty() {
            rect.y += area.bottom().saturating_sub(height);
        }
    }
    (rects, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narrow_action_row_cycles_through_every_mouse_target_and_feedback_clears_them() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut bar = ActionRow::default();
        let mut term = Terminal::new(TestBackend::new(19, 3)).unwrap();
        let actions = [
            ("VISUAL", None),
            ("[Copy]", Some(0)),
            ("[Highlight]", Some(1)),
            ("[Note]", Some(2)),
        ];
        let mut seen = [false; 3];
        for _ in 0..4 {
            let mut hits = Vec::new();
            term.draw(|f| {
                bar.draw(
                    f,
                    last_row(f.area()),
                    Default::default(),
                    None,
                    &actions,
                    &mut hits,
                )
            })
            .unwrap();
            for (rect, command) in hits {
                assert_eq!(rect.y, 2);
                seen[command] = true;
            }
            assert!(bar.advance((17, 2).into()));
        }
        assert_eq!(seen, [true; 3]);
        bar.notify("Copied");
        let mut hits = Vec::new();
        term.draw(|f| {
            bar.draw(
                f,
                last_row(f.area()),
                Default::default(),
                None,
                &actions,
                &mut hits,
            )
        })
        .unwrap();
        assert!(hits.is_empty());
        assert!(!bar.advance((17, 2).into()));
    }
    #[test]
    fn actions_wrap_whole_and_leave_space_between_mouse_targets() {
        let labels = [
            "[Ctrl+S Save]",
            "[Esc Cancel]",
            "[Enter New line]",
            "[Tab Color]",
        ];
        for width in [24, 40, 52, 80, 120] {
            let area = Rect::new(3, 5, width, 8);
            let (rects, height) = layout(area, &labels);
            assert!(height >= 1);
            for (i, rect) in rects.iter().enumerate() {
                assert_eq!(usize::from(rect.width), labels[i].len());
                assert_eq!(*rect, rect.intersection(area));
                for other in &rects[..i] {
                    assert!(rect.intersection(*other).is_empty());
                    if rect.y == other.y {
                        assert!(rect.x >= other.right() + 2);
                    }
                }
            }
        }
    }
}
