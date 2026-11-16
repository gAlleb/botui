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
    epub_spine: Option<botui_core::EpubSpine>,
    /// Indices of the deepest entry at each destination, in physical page order.
    sections: Vec<usize>,
    visible: Vec<usize>,
    folded: std::collections::HashSet<usize>,
    selected: usize,
    first: usize,
    rows: Rect,
    focus: bool,
}
impl Contents {
    pub fn set_epub_spine(&mut self, spine: botui_core::EpubSpine) {
        self.epub_spine = Some(spine);
    }

    fn section_index(&self, shown: u32) -> Option<usize> {
        self.sections
            .partition_point(|&i| self.items[i].page <= shown)
            .checked_sub(1)
    }

    /// Position within the outline section containing the first displayed page.
    /// Scroll offsets cover a whole page, from zero at its top to u16::MAX at its end.
    pub fn section_position(
        &self,
        shown: u32,
        scroll_offset: Option<u16>,
        total: u32,
        finished: bool,
    ) -> Option<crate::appearance::SectionPosition<'_>> {
        if shown == 0 || shown > total {
            return None;
        }
        let outline = self.section_index(shown);
        let page = (shown - 1) as usize;
        // A plate is a page. Several plates can belong to one EPUB spine item.
        let (index, count, member_pages) = if let Some(spine) = &self.epub_spine {
            let member = spine.position_at_block(page)?;
            (member.ordinal, member.total, Some(member.blocks))
        } else {
            (self.sections[outline?] + 1, self.items.len(), None)
        };
        let (title, pages) = if let Some(section) = outline {
            let entry = &self.items[self.sections[section]];
            let end = self
                .sections
                .get(section + 1)
                .map_or(total, |&i| total.min(self.items[i].page - 1));
            (
                Some(entry.title.as_str()).filter(|s| !s.trim().is_empty()),
                (entry.page - 1) as usize..end as usize,
            )
        } else {
            (None, member_pages?)
        };
        let units_per_page = u64::from(u16::MAX);
        let elapsed =
            (page - pages.start) as u64 * units_per_page + u64::from(scroll_offset.unwrap_or(0));
        let length = pages.len() as u64 * units_per_page;
        Some(crate::appearance::SectionPosition {
            index,
            total: count,
            title,
            percent: Some(if finished && shown == total {
                100
            } else {
                (100 * elapsed / length) as u8
            }),
        })
    }
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
    fn index_for_page(&self, page: u32) -> Option<usize> {
        if self.items.is_empty() {
            return page.checked_sub(1).map(|i| i as usize);
        }
        let index = self.sections[self.section_index(page)?];
        self.visible.partition_point(|i| *i <= index).checked_sub(1)
    }
    pub fn set_outline(&mut self, items: Vec<botui_fixed::OutlineEntry>) {
        if items.is_empty() {
            return;
        }
        // A page selected while the outline was loading now belongs to a chapter.
        let selected_page = self.page(self.selected);
        self.items = items;
        self.sections = (0..self.items.len())
            .filter(|&i| self.items[i].page > 0)
            .collect();
        self.sections.sort_unstable_by_key(|&i| {
            let entry = &self.items[i];
            (entry.page, std::cmp::Reverse((entry.depth, i)))
        });
        self.sections.dedup_by_key(|i| self.items[*i].page);
        self.folded.clear();
        self.rebuild();
        self.selected = self.index_for_page(selected_page).unwrap_or(0);
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
        let title = if self.items.is_empty() {
            " Pages "
        } else {
            " Contents "
        };
        let border = crate::appearance::outline_block(title, self.focus);
        self.rows = border.inner(area);
        f.render_widget(border, area);
        let count = self.count(total);
        let current = self.index_for_page(page);
        if !self.focus {
            self.selected = current.unwrap_or(0);
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
            let place = if Some(i) == current { '•' } else { ' ' };
            let label = item.map_or_else(
                || format!("{place} Page {}", i + 1),
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
                        "{place}{:indent$}{marker} {}  {}",
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
                if self.focus && i == self.selected {
                    p.reversed()
                } else {
                    p
                },
                Rect::new(self.rows.x, self.rows.y + y as u16, self.rows.width, 1),
            );
        }
    }
    /// Some(None) consumes a focus or selection movement without changing page.
    pub fn focus(&mut self) {
        self.focus = true;
    }
    pub fn focused(&self) -> bool {
        self.focus
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
    fn section_percentage_tracks_the_visible_page_and_scroll_offset() {
        let mut toc = Contents::default();
        assert!(toc.section_position(2, None, 12, false).is_none());
        // Unordered destinations, duplicate headings, and an invalid destination
        // must not change the physical extent of a section.
        toc.set_outline(
            [8, 3, 0, 3, 7, 10, 20]
                .map(|page| botui_fixed::OutlineEntry {
                    title: format!("Destination {page}"),
                    page,
                    depth: 0,
                })
                .into(),
        );
        let percent = |page, offset, total| {
            toc.section_position(page, offset, total, false)
                .and_then(|s| s.percent)
        };
        assert_eq!(percent(2, None, 12), None);
        assert_eq!(percent(3, None, 12), Some(0));
        assert_eq!(percent(4, None, 12), Some(25));
        assert_eq!(percent(4, Some(0), 12), Some(25));
        assert_eq!(percent(4, Some(32768), 12), Some(37));
        assert_eq!(percent(6, None, 12), Some(75));
        assert_eq!(percent(6, Some(u16::MAX - 1), 12), Some(99));
        assert_eq!(percent(6, Some(u16::MAX), 12), Some(100));
        assert_eq!(percent(7, None, 12), Some(0));
        assert_eq!(percent(7, Some(32768), 12), Some(50));
        assert_eq!(percent(7, Some(u16::MAX), 12), Some(100));
        assert_eq!(percent(8, None, 12), Some(0));
        assert_eq!(percent(12, None, 12), Some(66));
        assert_eq!(percent(12, Some(u16::MAX), 12), Some(100));
        assert_eq!(percent(13, None, 12), None);
        assert_eq!(percent(0, None, 0), None);
    }

    #[test]
    fn section_title_and_percentage_choose_the_same_outline_destination() {
        let mut toc = Contents::default();
        toc.set_outline(
            [(9, 0, "Chapter 4"), (3, 1, "Section 2.1"), (3, 0, "Part I")]
                .map(|(page, depth, title)| botui_fixed::OutlineEntry {
                    page,
                    depth,
                    title: title.into(),
                })
                .into(),
        );
        let current = |page: u32, finished: bool| {
            toc.section_position(page, None, 10, finished)
                .map(|s| (s.index, s.total, s.title, s.percent))
        };
        assert_eq!(current(1, false), None);
        assert_eq!(
            current(3, false),
            Some((2, 3, Some("Section 2.1"), Some(0)))
        );
        assert_eq!(
            current(6, false),
            Some((2, 3, Some("Section 2.1"), Some(50)))
        );
        assert_eq!(current(9, true), Some((1, 3, Some("Chapter 4"), Some(0))));
        assert_eq!(
            current(10, true),
            Some((1, 3, Some("Chapter 4"), Some(100)))
        );
    }

    #[test]
    fn album_numbers_spine_items_and_measures_the_chapter_across_plates() {
        let mut toc = Contents::default();
        // Six plates across two XHTML files; two other itemrefs have no plates.
        toc.set_epub_spine(botui_core::EpubSpine::from_counts(vec![
            (0, 0),
            (4, 0),
            (2, 0),
            (0, 0),
        ]));
        let position = |toc: &Contents, page| {
            let s = toc.section_position(page, None, 6, false).unwrap();
            (s.index, s.total, s.title.map(str::to_owned), s.percent)
        };
        assert_eq!(position(&toc, 2), (2, 4, None, Some(25)));
        assert_eq!(position(&toc, 5), (3, 4, None, Some(0)));
        toc.set_outline(vec![botui_fixed::OutlineEntry {
            page: 3,
            title: "Plates".into(),
            depth: 0,
        }]);
        assert_eq!(position(&toc, 2), (2, 4, None, Some(25)));
        assert_eq!(position(&toc, 3), (2, 4, Some("Plates".into()), Some(0)));
        assert_eq!(position(&toc, 5), (3, 4, Some("Plates".into()), Some(50)));
    }

    #[test]
    fn page_marker_stays_put_while_browsing_an_unoutlined_pdf() {
        use ratatui::style::Modifier;
        let mut term = Terminal::new(TestBackend::new(25, 10)).unwrap();
        let mut sidebar = Contents::default();
        term.draw(|f| sidebar.draw(f, f.area(), 3, 7)).unwrap();
        sidebar.input(&Event::Key(KeyCode::Tab.into()), 7);
        sidebar.input(&Event::Key(KeyCode::Down.into()), 7);
        term.draw(|f| sidebar.draw(f, f.area(), 3, 7)).unwrap();
        let buffer = term.backend().buffer();
        let (x, y) = (sidebar.rows.x, sidebar.rows.y + 2);
        assert_eq!(buffer[(x, y)].symbol(), "•");
        assert!(!buffer[(x, y)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(x, y + 1)].modifier.contains(Modifier::REVERSED));
        assert_eq!(
            sidebar.input(&Event::Key(KeyCode::Enter.into()), 7),
            Some(Some(4))
        );
        term.draw(|f| sidebar.draw(f, f.area(), 4, 7)).unwrap();
        let marker = &term.backend().buffer()[(x, y + 1)];
        assert_eq!(marker.symbol(), "•");
        assert!(!marker.modifier.contains(Modifier::REVERSED));
    }

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
        let section = toc.section_position(7, None, 12, false).unwrap();
        assert_eq!(
            (section.index, section.total, section.title),
            (4, 5, Some("Next"))
        );
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
