//! Mark the reading position briefly after landing on a chapter-based page.
use botui_core::{Anchor, Document, PageLine, TextRange};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub(super) struct WordFlash {
    active: Option<(TextRange, Instant)>,
}

impl WordFlash {
    pub fn show(&mut self, doc: &Document, anchor: Anchor) {
        self.active =
            reading_word(doc, anchor).map(|range| (range, Instant::now() + Duration::from_secs(2)));
    }

    pub fn clear(&mut self) {
        self.active = None;
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.active
            .map(|(_, end)| end.saturating_duration_since(Instant::now()))
    }

    pub fn expire(&mut self) -> bool {
        self.active
            .take_if(|(_, end)| *end <= Instant::now())
            .is_some()
    }

    pub fn paint(&self, doc: &Document, line: &PageLine, row: Rect, buf: &mut Buffer) {
        let Some((word, _)) = self.active else { return };
        if !line.spans.is_empty()
            && !line.spans.iter().any(|span| {
                span.source.is_some_and(|at| {
                    at.block == word.start.block
                        && at.byte < word.end.byte
                        && span.source_end > word.start.byte
                })
            })
        {
            return;
        }
        let style = Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD);
        for cell in line.text_cells(doc) {
            if cell.source.intersects(word) && cell.columns.end <= usize::from(row.width) {
                let x = row.x.saturating_add(cell.columns.start as u16);
                buf.set_style(Rect::new(x, row.y, cell.columns.len() as u16, 1), style);
            }
        }
    }
}

fn reading_word(doc: &Document, anchor: Anchor) -> Option<TextRange> {
    let md = doc.markdown.as_ref().filter(|m| !m.show_source);
    let (text, pos) = if let Some(md) = md {
        (md.text.as_str(), md.display_position(anchor.byte))
    } else {
        (doc.blocks.get(anchor.block)?.text.as_str(), anchor.byte)
    };
    let prefix = text.get(..pos)?;
    let from = prefix
        .trim_end_matches(|ch: char| !ch.is_whitespace())
        .len();
    let mut words = text[from..]
        .unicode_word_indices()
        .map(|(start, word)| (from + start, word))
        .peekable();
    while let Some((start, word)) = words.next() {
        let mut end = start + word.len();
        // Keep hyphenated compounds and attached numeric note markers together.
        while let Some(&(next, suffix)) = words.peek() {
            let joiner = &text[end..next];
            let next_end = next + suffix.len();
            if matches!(joiner, "-" | "‐" | "‑") {
                end = next_end;
            } else if joiner.ends_with('[')
                && !joiner.chars().any(char::is_whitespace)
                && suffix.bytes().all(|b| b.is_ascii_digit())
                && text[next_end..].starts_with(']')
            {
                end = next_end + 1;
            } else {
                break;
            }
            words.next();
        }
        if pos < end {
            return if let Some(md) = md {
                md.source_range(start..end)
            } else {
                Some(TextRange::new(
                    Anchor {
                        byte: start,
                        ..anchor
                    },
                    Anchor {
                        byte: end,
                        ..anchor
                    },
                ))
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use botui_core::{Block, LayoutMode, Pager, TextAlignment};
    use ratatui::text::Line;
    use ratatui::widgets::{Paragraph, Widget};

    fn document(text: &str) -> Document {
        Document {
            blocks: vec![Block {
                text: text.into(),
                heading: false,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn word_uses_source_offsets_including_attached_note_and_combining_marks() {
        let text = "Привет, «кто-то…[21]»!   e\u{301}té.";
        let doc = document(text);
        for (part, expected) in [
            ("ивет", "Привет"),
            ("кто", "кто-то…[21]"),
            ("21", "кто-то…[21]"),
            ("   ", "e\u{301}té"),
            ("té", "e\u{301}té"),
        ] {
            let anchor = Anchor {
                block: 0,
                byte: text.find(part).unwrap(),
            };
            assert_eq!(
                doc.text_range(reading_word(&doc, anchor).unwrap()),
                expected
            );
        }
        let mut flash = WordFlash::default();
        flash.show(&doc, Anchor::default());
        assert!(flash.timeout().is_some());
        flash.active.as_mut().unwrap().1 = Instant::now();
        assert!(flash.expire());
        assert_eq!(flash.timeout(), None);
        flash.show(&doc, Anchor::default());
        flash.clear();
        assert!(!flash.expire());
        assert_eq!(flash.timeout(), None);
    }

    #[test]
    fn styled_markdown_keeps_the_selected_occurrence_across_inline_markup() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("book.md");
        let raw = "Читает море и мо**ре** рядом.";
        std::fs::write(&path, raw).unwrap();
        let doc = botui_formats::open_document(&path).unwrap();
        let pager = Pager::new(&doc, 56, 8, LayoutMode::Single);
        let mut flash = WordFlash::default();
        flash.show(
            &doc,
            Anchor {
                block: 0,
                byte: raw.rfind("ре").unwrap(),
            },
        );
        let mut buf = Buffer::empty(Rect::new(0, 0, 56, 8));
        for (y, line) in pager.lines(0).iter().enumerate() {
            let row = Rect::new(line.indent, y as u16, 56 - line.indent, 1);
            Paragraph::new(Line::raw(&line.text)).render(row, &mut buf);
            flash.paint(&doc, line, row, &mut buf);
            if let Some(before) = line.text.find("море") {
                let x = usize::from(line.indent) + line.text[..before].chars().count();
                assert!(
                    !buf[(x as u16, y as u16)]
                        .modifier
                        .contains(Modifier::REVERSED)
                );
            }
        }
        let marked: String = buf
            .content
            .iter()
            .filter(|c| c.modifier.contains(Modifier::REVERSED))
            .map(|c| c.symbol())
            .collect();
        assert_eq!(marked, "море");
    }

    #[test]
    fn paints_only_the_target_letters_on_each_aligned_hyphenated_line() {
        let text = "Два замечательных предложения";
        let doc = document(text);
        let mut pager = Pager::new(&doc, 12, 12, LayoutMode::Single);
        let mut options = pager.reflow_options();
        options.typography.alignment = TextAlignment::Right;
        options.typography.hyphenation = true;
        pager.reconfigure(&doc, 12, 12, LayoutMode::Single, 4, options);

        let mut flash = WordFlash::default();
        flash.show(
            &doc,
            Anchor {
                block: 0,
                byte: text.find("меч").unwrap(),
            },
        );
        let mut buf = Buffer::empty(Rect::new(0, 0, 22, 12));
        let mut shown = String::new();
        let mut fragment_rows = 0;
        for (y, line) in pager.lines(0).iter().enumerate() {
            let row = Rect::new(4 + line.indent, y as u16, 12 - line.indent, 1);
            Paragraph::new(Line::raw(&line.text)).render(row, &mut buf);
            flash.paint(&doc, line, row, &mut buf);
            let mut marked = buf
                .content
                .iter()
                .skip(y * 22)
                .take(22)
                .filter(|c| c.modifier.contains(Modifier::BOLD | Modifier::REVERSED))
                .peekable();
            if marked.peek().is_some() {
                fragment_rows += 1;
                for cell in marked {
                    shown.push_str(cell.symbol());
                }
            }
        }
        assert!(fragment_rows > 1);
        assert_eq!(shown, "замечательных");
    }

    #[test]
    fn clipping_a_wide_glyph_does_not_paint_half_of_it_or_the_gutter() {
        let doc = document("界 end");
        let pager = Pager::new(&doc, 12, 12, LayoutMode::Single);
        let line = &pager.lines(0)[0];
        let mut flash = WordFlash::default();
        flash.show(&doc, Anchor::default());
        let mut buf = Buffer::empty(Rect::new(0, 0, 8, 1));
        flash.paint(&doc, line, Rect::new(3, 0, 1, 1), &mut buf);
        assert!(buf.content.iter().all(|c| c.modifier.is_empty()));
        flash.paint(&doc, line, Rect::new(3, 0, 2, 1), &mut buf);
        for (x, cell) in buf.content.iter().enumerate() {
            assert_eq!(
                cell.modifier.contains(Modifier::REVERSED),
                (3..5).contains(&x)
            );
        }
    }
}
