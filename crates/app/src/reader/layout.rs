//! A spread is one centered rectangle. The distance between its text columns is explicit.
use super::{Action, LayoutMode};
use crate::appearance::ReadingFrame;
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
    screen.height = screen.height.saturating_sub(u16::from(!zen));
    splitter.layout(screen, !zen && screen.width >= 78)
}

pub(super) fn width_setting(width: u16) -> u16 {
    if width == 0 { 0 } else { width.clamp(32, 240) }
}

pub(super) fn geometry(
    frame: &ReadingFrame,
    mode: LayoutMode,
    width: u16,
    gap: u16,
    zen: bool,
) -> (Rect, LayoutMode, u16) {
    let area = frame.inner;
    let available = area
        .width
        .saturating_sub(if area.width >= 12 { 2 } else { 0 });
    let spread = mode == LayoutMode::Spread && available >= 66;
    let gap = if spread {
        gap.min(available.saturating_sub(64))
    } else {
        0
    };
    let width = if width == 0 { available } else { width };
    let used = if spread {
        (width.min((available - gap) / 2)) * 2 + gap
    } else {
        width.min(available)
    };
    // In the normal reader the frame itself provides one row of separation.
    let vertical = (area.height / 13)
        .min(2)
        .saturating_sub(u16::from(zen || frame.has_border()));
    let mut text = crate::centered(area, used, area.height);
    text.y += vertical;
    text.height = area.height.saturating_sub(vertical * 2);
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

pub(super) struct Panel {
    supports_hyphens: bool,
    hits: Vec<(Rect, Action)>,
}
impl Panel {
    pub fn new(supports_hyphens: bool) -> Self {
        Self {
            supports_hyphens,
            hits: Vec::new(),
        }
    }
    pub fn draw(
        &mut self,
        f: &mut Frame,
        style: Style,
        (effective, width): (u16, u16),
        typography: botui_core::Typography,
        gap: u16,
        actual_gap: Option<u16>,
    ) {
        let screen = f.area();
        let mut panel = crate::centered(screen, 52, 6 + u16::from(self.supports_hyphens));
        panel.y = screen.bottom().saturating_sub(panel.height);
        crate::overlay::backdrop(f, panel);
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
                if width == 0 {
                    format!("Full ({effective})")
                } else if effective < width {
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
            let width = row.width.min(16);
            let full = Rect::new(row.x, row.y, width, 1);
            let align = Rect::new(full.right(), row.y, row.width - width, 1);
            f.render_widget(Paragraph::new("[F Full width]"), full);
            f.render_widget(
                Paragraph::new(format!("[A Alignment: {}]", typography.alignment.label())),
                align,
            );
            self.hits
                .extend([(full, Action::FullWidth), (align, Action::Alignment)]);
        }
        if self.supports_hyphens && inside.height >= 4 {
            let value = if typography.hyphenation { "On" } else { "Off" };
            let label = format!("[H Hyphenation: {value}]");
            let hit = Rect::new(
                inside.x,
                inside.y + 3,
                inside.width.min(label.len() as u16),
                1,
            );
            f.render_widget(Paragraph::new(label), hit);
            self.hits.push((hit, Action::Hyphenation));
        }
        let footer = 3 + u16::from(self.supports_hyphens);
        if inside.height > footer {
            let row = Rect::new(inside.x, inside.y + footer, inside.width, 1);
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
                KeyCode::Char('f' | 'F') => Some(Action::FullWidth),
                KeyCode::Char('h' | 'H') if self.supports_hyphens => Some(Action::Hyphenation),
                KeyCode::Char('a' | 'A') => Some(Action::Alignment),
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

pub(super) fn choose_alignment(
    term: &mut crate::AppTerminal,
    reader: &mut super::Reader,
    store: Option<&botui_store::LocalState>,
) -> std::io::Result<()> {
    use crate::settings::book::BookSettings;
    use botui_core::TextAlignment;
    let mut settings = BookSettings::effective(store)?;
    let original = settings.text_alignment;
    let mut row = original as usize;
    let accepted = loop {
        let mut body = Rect::default();
        term.draw(|f| {
            reader.draw(f, None);
            let area = crate::centered(f.area(), 27, 8);
            crate::overlay::paint(f, area, |f| {
                let panel = crate::appearance::block()
                    .title(" Text alignment ")
                    .style(reader.theme.style());
                body = panel.inner(area);
                f.render_widget(Clear, area);
                f.render_widget(panel, area);
                for (i, alignment) in TextAlignment::ALL
                    .iter()
                    .enumerate()
                    .take(usize::from(body.height))
                {
                    let style = if i == row {
                        crate::appearance::SELECTION
                    } else {
                        reader.theme.style()
                    };
                    f.render_widget(
                        Paragraph::new(alignment.label()).style(style),
                        Rect::new(body.x, body.y + i as u16, body.width, 1),
                    );
                }
                if body.height > 5 {
                    f.render_widget(
                        Paragraph::new("Enter apply · Esc cancel").dim(),
                        Rect::new(body.x, body.y + 5, body.width, 1),
                    );
                }
            });
            crate::settings::paint_book(f, reader.theme);
        })?;
        reader.pager.start_pagination();
        let event = crate::settings::read_dialog()?;
        let mut accept = false;
        match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Up | KeyCode::Char('k') => row = row.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => row = (row + 1).min(4),
                KeyCode::Enter => break true,
                KeyCode::Esc | KeyCode::Char('q') => break false,
                _ => continue,
            },
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                if !body.contains(Position::new(m.column, m.row)) {
                    break false;
                }
                let i = usize::from(m.row - body.y);
                if i >= 5 {
                    continue;
                }
                row = i;
                accept = true;
            }
            _ => continue,
        }
        if settings.text_alignment != TextAlignment::ALL[row] {
            settings.text_alignment = TextAlignment::ALL[row];
            reader.apply_typography(term, &settings)?;
        }
        if accept {
            break true;
        }
    };
    if accepted {
        if let Some(store) = store {
            store.update_book_preferences(|p: &mut BookSettings| {
                p.alignment = Some(settings.text_alignment)
            })?;
        } else {
            crate::settings::Settings::update(|s| s.text_alignment = settings.text_alignment)?;
        }
    } else if settings.text_alignment != original {
        settings.text_alignment = original;
        reader.apply_typography(term, &settings)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::BordersStyle;
    fn frame(area: Rect, zen: bool, style: BordersStyle) -> ReadingFrame {
        ReadingFrame::with_borders(area, zen, true, style)
    }
    #[test]
    fn extra_terminal_space_stays_outside_the_spread() {
        for width in [180, 240, 320] {
            for page in [52, 76] {
                for gap in [0, 4, 12] {
                    let (area, mode, actual_gap) = geometry(
                        &frame(Rect::new(0, 0, width, 40), false, BordersStyle::None),
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
        let (single, mode, _) = geometry(
            &frame(Rect::new(0, 0, 45, 14), false, BordersStyle::None),
            LayoutMode::Spread,
            76,
            6,
            false,
        );
        assert_eq!(mode, LayoutMode::Single);
        assert!(single.width <= 45);
    }
    #[test]
    fn clean_zen_keeps_its_margins_and_spread_width() {
        for (height, top) in [(8, 0), (24, 0), (40, 1)] {
            let screen = Rect::new(0, 0, 160, height);
            let (normal, _, _) = geometry(
                &frame(screen, false, BordersStyle::None),
                LayoutMode::Spread,
                70,
                8,
                false,
            );
            let (zen, _, gap) = geometry(
                &ReadingFrame::new(screen, true, false),
                LayoutMode::Spread,
                70,
                8,
                true,
            );
            assert_eq!((zen.x, zen.width, gap), (normal.x, normal.width, 8));
            assert_eq!((zen.y, screen.bottom() - zen.bottom()), (top, top));
            assert_eq!(zen.height, height - top * 2);
        }
    }
    #[test]
    fn only_normal_framed_text_reclaims_one_of_its_existing_margin_rows() {
        for (height, framed_normal, unframed_normal, zen_margin) in
            [(3, 0, 0, 0), (8, 0, 0, 0), (24, 0, 1, 0), (40, 1, 2, 1)]
        {
            for (zen, border, expected) in [
                (false, BordersStyle::Rounded, framed_normal),
                (false, BordersStyle::Square, framed_normal),
                (false, BordersStyle::None, unframed_normal),
                (true, BordersStyle::Rounded, zen_margin),
                (true, BordersStyle::None, zen_margin),
            ] {
                let frame = frame(Rect::new(3, 4, 120, height), zen, border);
                for mode in [LayoutMode::Single, LayoutMode::Spread, LayoutMode::Scroll] {
                    let (text, _, _) = geometry(&frame, mode, 70, 8, zen);
                    assert_eq!(text.y - frame.inner.y, expected);
                    assert_eq!(frame.inner.bottom() - text.bottom(), expected);
                }
            }
        }
    }
}
