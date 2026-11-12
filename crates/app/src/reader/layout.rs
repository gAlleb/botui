//! A spread is one centered rectangle. The distance between its text columns is explicit.
use super::{Action, LayoutMode};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Style, Stylize},
    widgets::{Clear, Paragraph},
};

pub(super) fn panes(
    mut screen: Rect,
    splitter: &mut crate::splitter::Splitter,
    zen: bool,
) -> (Rect, Rect) {
    if !zen {
        screen.height = screen.height.saturating_sub(1);
    }
    splitter.layout(screen, !zen && screen.width >= 78)
}

pub(super) fn geometry(
    area: Rect,
    mode: LayoutMode,
    width: u16,
    gap: u16,
    zen: bool,
) -> (Rect, LayoutMode, u16) {
    let available = area
        .width
        .saturating_sub(if area.width >= 12 { 2 } else { 0 });
    let spread = mode == LayoutMode::Spread && available >= 66;
    let gap = if spread {
        gap.min(available.saturating_sub(64))
    } else {
        0
    };
    let used = if spread {
        (width.min((available - gap) / 2)) * 2 + gap
    } else {
        width.min(available)
    };
    let vertical = (area.height / 13).min(2).saturating_sub(u16::from(zen));
    // The progress/back-link footer uses the bottom margin, even when hidden.
    // Toggling it must neither repaginate the book nor move the saved passage.
    let bottom = if zen { vertical.max(1) } else { vertical };
    let mut text = crate::centered(area, used, area.height);
    text.y += vertical;
    text.height = area.height.saturating_sub(vertical + bottom);
    (
        text,
        if spread {
            LayoutMode::Spread
        } else if mode == LayoutMode::Scroll {
            LayoutMode::Scroll
        } else {
            LayoutMode::Single
        },
        gap,
    )
}

#[derive(Default)]
pub(super) struct Panel {
    hits: Vec<(Rect, Action)>,
}
impl Panel {
    pub fn draw(
        &mut self,
        f: &mut Frame,
        style: Style,
        effective: u16,
        width: u16,
        gap: u16,
        actual_gap: Option<u16>,
    ) {
        let screen = f.area();
        let mut panel = crate::centered(screen, 52, 5);
        panel.y = screen.bottom().saturating_sub(panel.height);
        let block = crate::appearance::block()
            .title(" Page layout ")
            .style(style);
        let inside = block.inner(panel);
        f.render_widget(Clear, panel);
        f.render_widget(block, panel);
        self.hits.clear();
        let values = [
            (
                "Page width",
                if effective < width {
                    format!("{effective} (max here)")
                } else {
                    format!("{width} cells")
                },
                Action::Margin(1),
                Action::Margin(-1),
            ),
            (
                "Column gap",
                match actual_gap {
                    Some(v) if v < gap => format!("{v} (max here)"),
                    Some(_) => format!("{gap} cells"),
                    None => format!("{gap} · spread"),
                },
                Action::Gap(-1),
                Action::Gap(1),
            ),
        ];
        for (i, (label, value, less, more)) in values.into_iter().enumerate() {
            if i >= inside.height as usize {
                break;
            }
            let row = Rect::new(inside.x, inside.y + i as u16, inside.width, 1);
            let button = (row.width / 2).min(4);
            let value_w = row.width.saturating_sub(2 * button).min(14);
            let left = Rect::new(row.right() - button * 2 - value_w, row.y, button, 1);
            let right = Rect::new(row.right() - button, row.y, button, 1);
            f.render_widget(
                Paragraph::new(label),
                Rect::new(row.x, row.y, left.x - row.x, 1),
            );
            f.render_widget(
                Paragraph::new(value).centered(),
                Rect::new(left.right(), row.y, value_w, 1),
            );
            for (text, rect, action) in [("[-]", left, less), ("[+]", right, more)] {
                f.render_widget(Paragraph::new(text).centered().bold(), rect);
                self.hits.push((rect, action));
            }
        }
        if inside.height >= 3 {
            let row = Rect::new(inside.x, inside.y + 2, inside.width, 1);
            let done = Rect::new(row.right() - row.width.min(8), row.y, row.width.min(8), 1);
            f.render_widget(
                Paragraph::new("[ ] width   { } gap").dim(),
                Rect::new(row.x, row.y, done.x - row.x, 1),
            );
            f.render_widget(Paragraph::new("[ Done ]"), done);
            self.hits.push((done, Action::Layout));
        }
    }
    pub fn input(&self, event: Event) -> Option<Action> {
        match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Char('[') => Some(Action::Margin(-1)),
                KeyCode::Char(']') => Some(Action::Margin(1)),
                KeyCode::Char('{') => Some(Action::Gap(-1)),
                KeyCode::Char('}') => Some(Action::Gap(1)),
                KeyCode::Char('1') => Some(Action::Single),
                KeyCode::Char('2') => Some(Action::Spread),
                KeyCode::Char('3') => Some(Action::Scroll),
                KeyCode::Esc
                | KeyCode::Enter
                | KeyCode::Char('q' | 'Q' | 'w' | 'W' | 'm' | 'M') => Some(Action::Layout),
                _ => None,
            },
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => self
                .hits
                .iter()
                .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
                .map(|(_, a)| *a),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extra_terminal_space_stays_outside_the_spread() {
        for width in [180, 240, 320] {
            for page in [52, 76] {
                for gap in [0, 4, 12] {
                    let (area, mode, actual_gap) = geometry(
                        Rect::new(0, 0, width, 40),
                        LayoutMode::Spread,
                        page,
                        gap,
                        false,
                    );
                    assert_eq!(
                        (mode, actual_gap, area.width),
                        (LayoutMode::Spread, gap, page * 2 + gap)
                    );
                    assert!(area.x.abs_diff(width - area.right()) <= 1);
                }
            }
        }
        let (single, mode, _) = geometry(Rect::new(0, 0, 45, 14), LayoutMode::Spread, 76, 6, false);
        assert_eq!(mode, LayoutMode::Single);
        assert!(single.width <= 45);
    }
    #[test]
    fn zen_reclaims_one_row_on_each_edge_without_changing_the_spread() {
        for (height, top) in [(8, 0), (24, 0), (40, 1)] {
            let screen = Rect::new(0, 0, 160, height);
            let (normal, _, _) = geometry(screen, LayoutMode::Spread, 70, 8, false);
            let (zen, _, gap) = geometry(screen, LayoutMode::Spread, 70, 8, true);
            assert_eq!((zen.x, zen.width, gap), (normal.x, normal.width, 8));
            assert_eq!((zen.y, screen.bottom() - zen.bottom()), (top, top.max(1)));
            assert_eq!(zen.height, height - top - top.max(1));
        }
    }
}
