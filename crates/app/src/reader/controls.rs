//! Keep action labels and their mouse targets whole, even in a narrow terminal.
use ratatui::{layout::Rect, text::Line};

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
