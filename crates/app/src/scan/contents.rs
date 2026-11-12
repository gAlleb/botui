use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Stylize,
    widgets::Paragraph,
};

#[derive(Default)]
pub struct Contents {
    items: Vec<botui_fixed::OutlineEntry>,
    visible: Vec<usize>,
    folded: std::collections::HashSet<usize>,
    selected: usize,
    first: usize,
    rows: Rect,
    focus: bool,
}
impl Contents {
    fn count(&self, total: u32) -> usize {
        if self.items.is_empty() {
            total as usize
        } else {
            self.visible.len()
        }
    }
    fn page(&self, i: usize) -> u32 {
        self.visible
            .get(i)
            .map_or(i as u32 + 1, |&j| self.items[j].page)
    }
    fn index_for_page(&self, page: u32) -> usize {
        if self.items.is_empty() {
            return page.saturating_sub(1) as usize;
        }
        let index = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, v)| v.page <= page)
            .max_by_key(|(_, v)| v.page)
            .map_or(0, |(i, _)| i);
        self.visible
            .partition_point(|i| *i <= index)
            .saturating_sub(1)
    }
    pub fn set_outline(&mut self, items: Vec<botui_fixed::OutlineEntry>) {
        if items.is_empty() {
            return;
        }
        // A page selected while the outline was loading now belongs to a chapter.
        let selected_page = self.page(self.selected);
        self.items = items;
        self.folded.clear();
        self.rebuild();
        self.selected = self.index_for_page(selected_page);
        self.first = 0;
    }
    fn indent(&self, index: usize) -> usize {
        (self.items[index].depth * 2).min(self.rows.width.saturating_sub(8) as usize)
    }
    fn group(&self, i: usize) -> bool {
        self.items
            .get(i + 1)
            .is_some_and(|next| next.depth > self.items[i].depth)
    }
    fn rebuild(&mut self) {
        self.visible.clear();
        let mut hidden = None;
        for (i, e) in self.items.iter().enumerate() {
            if hidden.is_some_and(|depth| e.depth > depth) {
                continue;
            }
            self.visible.push(i);
            hidden = self.folded.contains(&i).then_some(e.depth);
        }
    }
    fn fold(&mut self, close: bool) {
        let Some(&i) = self.visible.get(self.selected) else {
            return;
        };
        let mut target = i;
        if close && (!self.group(i) || self.folded.contains(&i)) {
            if let Some(parent) = (0..i)
                .rev()
                .find(|&j| self.items[j].depth < self.items[i].depth)
            {
                target = parent;
            }
        } else if self.group(i) {
            if close {
                self.folded.insert(i);
            } else if !self.folded.remove(&i) {
                target = i + 1;
            }
        }
        self.rebuild();
        self.selected = self
            .visible
            .partition_point(|i| *i <= target)
            .saturating_sub(1);
    }
    pub fn draw(&mut self, f: &mut Frame, area: Rect, page: u32, total: u32) {
        self.rows = Rect::default();
        if area.is_empty() {
            self.focus = false;
            return;
        }
        let border = crate::appearance::block().title(if self.items.is_empty() {
            " Pages "
        } else {
            " Contents "
        });
        self.rows = border.inner(area);
        f.render_widget(border, area);
        let count = self.count(total);
        if !self.focus {
            self.selected = self.index_for_page(page);
        }
        self.selected = self.selected.min(count.saturating_sub(1));
        let height = usize::from(self.rows.height);
        self.first = self
            .first
            .min(self.selected)
            .max((self.selected + 1).saturating_sub(height))
            .min(count.saturating_sub(height));
        for (y, i) in (self.first..count).take(height).enumerate() {
            let item = self.visible.get(i).map(|&j| &self.items[j]);
            let label = item.map_or_else(
                || format!(" Page {}", i + 1),
                |e| {
                    let index = self.visible[i];
                    let marker = if !self.group(index) {
                        " "
                    } else if self.folded.contains(&index) {
                        "▸"
                    } else {
                        "▾"
                    };
                    format!(
                        " {:indent$}{marker} {}  {}",
                        "",
                        e.page,
                        e.title,
                        indent = self.indent(index)
                    )
                },
            );
            let base = crate::appearance::outline_style(
                ratatui::style::Style::default(),
                item.map_or(0, |e| e.depth),
            );
            let p = Paragraph::new(label).style(base);
            f.render_widget(
                if i == self.selected { p.reversed() } else { p },
                Rect::new(self.rows.x, self.rows.y + y as u16, self.rows.width, 1),
            );
        }
    }
    /// Some(None) consumes a focus or selection movement without changing page.
    pub fn focus(&mut self) {
        self.focus = true;
    }

    pub fn input(&mut self, e: &Event, total: u32) -> Option<Option<u32>> {
        if self.rows.is_empty() {
            return None;
        }
        let step = match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Tab => {
                    self.focus = !self.focus;
                    return Some(None);
                }
                KeyCode::Enter if self.focus => {
                    self.focus = false;
                    return Some(Some(self.page(self.selected)));
                }
                KeyCode::Left if self.focus => {
                    self.fold(true);
                    return Some(None);
                }
                KeyCode::Right if self.focus => {
                    self.fold(false);
                    return Some(None);
                }
                KeyCode::Down if self.focus => 1,
                KeyCode::Up if self.focus => -1,
                KeyCode::PageDown if self.focus => self.rows.height as isize,
                KeyCode::PageUp if self.focus => -(self.rows.height as isize),
                _ => return None,
            },
            Event::Mouse(m) if self.rows.contains(Position::new(m.column, m.row)) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let i = self.first + usize::from(m.row - self.rows.y);
                    if let Some(&index) = self.visible.get(i)
                        && self.group(index)
                        && m.column <= self.rows.x + 1 + self.indent(index) as u16
                    {
                        self.selected = i;
                        self.focus = true;
                        self.fold(!self.folded.contains(&index));
                        return Some(None);
                    }
                    self.focus = false;
                    return Some((i < self.count(total)).then(|| self.page(i)));
                }
                MouseEventKind::ScrollDown => 1,
                MouseEventKind::ScrollUp => -1,
                _ => return None,
            },
            _ => return None,
        };
        self.focus = true;
        self.selected = self
            .selected
            .saturating_add_signed(step)
            .min(self.count(total).saturating_sub(1));
        Some(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn nested_sections_fold_without_losing_current_page_or_parent() {
        let mut term = Terminal::new(TestBackend::new(50, 30)).unwrap();
        let mut toc = Contents::default();
        toc.set_outline(
            [
                (1, 0, "Book"),
                (1, 1, "Chapter"),
                (4, 2, "Section"),
                (7, 2, "Next"),
                (10, 1, "Final"),
            ]
            .map(|(page, depth, title)| botui_fixed::OutlineEntry {
                page,
                depth,
                title: title.into(),
            })
            .into(),
        );
        let mut draw = |toc: &mut Contents| {
            term.draw(|f| toc.draw(f, Rect::new(0, 0, 50, 30), 7, 12))
                .unwrap();
        };
        draw(&mut toc);
        assert_eq!((toc.first, toc.selected, toc.page(toc.selected)), (0, 3, 7));
        toc.input(&Event::Key(KeyCode::Tab.into()), 12);
        toc.input(&Event::Key(KeyCode::Left.into()), 12); // focus parent
        assert_eq!(toc.page(toc.selected), 1);
        toc.input(&Event::Key(KeyCode::Left.into()), 12); // collapse
        draw(&mut toc);
        assert_eq!(toc.visible, [0, 1, 4]);
        toc.input(&Event::Key(KeyCode::Tab.into()), 12); // current hidden under Chapter
        draw(&mut toc);
        assert_eq!(toc.selected, 1);
        toc.input(&Event::Key(KeyCode::Tab.into()), 12);
        toc.input(&Event::Key(KeyCode::Right.into()), 12);
        assert_eq!(toc.visible, [0, 1, 2, 3, 4]);
        toc.input(&Event::Key(KeyCode::Down.into()), 12);
        assert_eq!(
            toc.input(&Event::Key(KeyCode::Enter.into()), 12),
            Some(Some(4))
        );
    }

    #[test]
    fn resumed_outline_fills_the_pane_after_loading_and_resizing() {
        let mut term = Terminal::new(TestBackend::new(40, 30)).unwrap();
        let mut toc = Contents::default();
        let mut draw = |toc: &mut Contents, height| {
            term.draw(|f| toc.draw(f, Rect::new(0, 0, 40, height), 111, 181))
                .unwrap();
        };
        draw(&mut toc, 30);
        assert_eq!(toc.selected, 110);
        assert!(toc.first > 0);

        let pages = [
            5, 6, 7, 9, 11, 19, 39, 42, 52, 57, 61, 71, 78, 84, 90, 96, 102, 111, 117, 123, 130,
            138, 165, 181,
        ];
        toc.set_outline(
            pages
                .map(|p| botui_fixed::OutlineEntry {
                    page: p,
                    title: format!("Heading {p}"),
                    depth: 0,
                })
                .into(),
        );
        draw(&mut toc, 30);
        assert_eq!((toc.first, toc.page(toc.selected)), (0, 111));

        draw(&mut toc, 6);
        assert_eq!(toc.first + usize::from(toc.rows.height) - 1, toc.selected);
        toc.input(&Event::Key(KeyCode::Tab.into()), 181);
        toc.input(&Event::Key(KeyCode::Down.into()), 181);
        draw(&mut toc, 30);
        assert_eq!((toc.first, toc.page(toc.selected)), (0, 117));
        assert_eq!(
            toc.input(&Event::Key(KeyCode::Enter.into()), 181),
            Some(Some(117))
        );
    }
}
