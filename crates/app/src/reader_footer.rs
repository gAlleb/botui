//! One row of reading status and clickable shortcuts for text and fixed pages.
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};

/// The most important controls go last. A control only gets a hit area if its
/// whole label fits beside the position. Empty labels are contextual actions.
pub(crate) fn draw<const N: usize>(
    frame: &mut Frame,
    area: Rect,
    style: Style,
    title: &str,
    status: &str,
    actions: [(&str, &str); N],
) -> [Rect; N] {
    let mut hits = [Rect::default(); N];
    if area.is_empty() {
        return hits;
    }

    let position = Line::from(status).style(style.dim());
    let position_width = position.width().min(usize::from(area.width)) as u16;
    let mut right = area.right();
    for (index, (key, label)) in actions.into_iter().enumerate().rev() {
        if label.is_empty() {
            continue;
        }
        let button = Line::from(vec![
            Span::raw("["),
            Span::styled(key, style.bold()),
            Span::raw(" "),
            Span::raw(label),
            Span::raw("]"),
        ])
        .style(style);
        let width = button.width() as u16;
        if right - area.x < position_width.saturating_add(width).saturating_add(1) {
            continue;
        }
        right -= width;
        hits[index] = Rect::new(right, area.y, width, 1);
        frame.render_widget(button, hits[index]);
        right -= 1;
    }
    let position_x = if title.is_empty() {
        area.x
    } else {
        right - position_width
    };
    frame.render_widget(position, Rect::new(position_x, area.y, position_width, 1));
    if position_x > area.x {
        frame.render_widget(
            Line::from(title).style(style.dim()),
            Rect::new(area.x, area.y, position_x - area.x, 1),
        );
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn narrow_footer_keeps_the_position_and_clickable_labels_whole() {
        let actions = [("Space z", "Zen"), ("", ""), ("F1", "Help")];
        for width in [5, 10, 18, 32, 73] {
            let mut term = Terminal::new(TestBackend::new(width, 2)).unwrap();
            let mut hits = [Rect::default(); 3];
            term.draw(|f| {
                hits = draw(
                    f,
                    Rect::new(0, 1, width, 1),
                    Style::default(),
                    " Book",
                    " 1/4 ",
                    actions,
                );
            })
            .unwrap();
            let buf = term.backend().buffer();
            let line = (0..width).map(|x| buf[(x, 1)].symbol()).collect::<String>();
            assert!(line.contains(" 1/4 "));
            assert!(hits[1].is_empty());
            for ((key, label), hit) in actions.iter().zip(hits) {
                if !hit.is_empty() {
                    assert_eq!(
                        &line[hit.x as usize..hit.right() as usize],
                        format!("[{key} {label}]")
                    );
                }
            }
            assert_eq!(
                line.matches('[').count(),
                hits.iter().filter(|h| !h.is_empty()).count()
            );
            assert_eq!(line.matches('[').count(), line.matches(']').count());
        }
    }
}
