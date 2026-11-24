use super::*;
use crate::vim::{Motion as M, text as motions};

impl Text {
    pub(super) fn precise_motion(&mut self, motion: M, count: usize, yank: bool) -> Action {
        let Some(at) = self.cursor else {
            return Action::None;
        };
        let Some(Ok(page)) = self.cache.get(&at.page) else {
            return Action::None;
        };
        let units = page.units();
        let origin = units[at.index].bytes.start;
        let mut index = at.index;
        let mut target_byte = origin;
        let mut object: Option<std::ops::Range<usize>> = None;
        for step in 0..if matches!(motion, M::Find { .. } | M::Delimited { .. }) {
            1
        } else {
            count
        } {
            if step > 0 && matches!(motion, M::Line | M::Paragraph) {
                let previous = &units[index];
                let Some(next) = units
                    .iter()
                    .enumerate()
                    .skip(index + 1)
                    .find(|(_, c)| {
                        if motion == M::Line {
                            c.line != previous.line
                        } else {
                            c.block != previous.block
                        }
                    })
                    .map(|(i, _)| i)
                else {
                    break;
                };
                index = next;
            }
            let unit = &units[index];
            let byte = unit.bytes.start;
            let line_start = units
                .iter()
                .position(|c| c.line == unit.line)
                .unwrap_or(index);
            let line_end = units
                .iter()
                .rposition(|c| c.line == unit.line)
                .unwrap_or(index);
            let next_byte = match motion {
                M::Left => Some(units[index.saturating_sub(1)].bytes.start),
                M::Right => Some(units[(index + 1).min(units.len() - 1)].bytes.start),
                M::Word
                | M::BigWord
                | M::BackWord
                | M::BackBigWord
                | M::EndWord
                | M::EndBigWord => {
                    let forward = !matches!(motion, M::BackWord | M::BackBigWord);
                    let end = matches!(motion, M::EndWord | M::EndBigWord);
                    let word = if matches!(motion, M::BigWord | M::BackBigWord | M::EndBigWord) {
                        motions::big_word
                    } else {
                        motions::word
                    };
                    word(&page.text, byte, forward, end)
                        .or_else(|| (yank && forward).then_some(page.text.len()))
                }
                M::InnerWord | M::AroundWord | M::InnerBigWord | M::AroundBigWord => {
                    let byte = if step == 0 {
                        byte
                    } else {
                        object.as_ref().unwrap().end
                    };
                    let word = if matches!(motion, M::InnerBigWord | M::AroundBigWord) {
                        motions::big_word_at
                    } else {
                        motions::word_at
                    };
                    let Some(range) = word(
                        &page.text,
                        byte,
                        matches!(motion, M::AroundWord | M::AroundBigWord),
                    ) else {
                        break;
                    };
                    object = Some(
                        object
                            .as_ref()
                            .map_or(range.clone(), |r| r.start..range.end),
                    );
                    Some(range.end.saturating_sub(1))
                }
                M::Find {
                    ch,
                    forward,
                    till,
                    repeat,
                } => {
                    let start = units[line_start].bytes.start;
                    let end = units[line_end].bytes.end;
                    let Some(found) = motions::find_char(
                        &page.text[start..end],
                        byte - start,
                        ch,
                        forward,
                        till,
                        repeat,
                        count,
                    ) else {
                        return Action::None;
                    };
                    Some(start + found)
                }
                M::Delimited {
                    open,
                    close,
                    around,
                } => {
                    let Some(range) =
                        motions::delimited(&page.text, byte, open, close, around, count)
                            .filter(|r| !r.is_empty())
                    else {
                        return Action::None;
                    };
                    object = Some(range.clone());
                    Some(range.end.saturating_sub(1))
                }
                M::Start | M::First | M::ScreenStart => Some(units[line_start].bytes.start),
                M::End | M::ScreenEnd => Some(units[line_end].bytes.start),
                M::Top => Some(units[0].bytes.start),
                M::Bottom => Some(units.last().unwrap().bytes.start),
                M::Down | M::ScreenDown | M::Up | M::ScreenUp => {
                    let up = matches!(motion, M::Up | M::ScreenUp);
                    let line = if up {
                        unit.line.saturating_sub(1)
                    } else {
                        unit.line + 1
                    };
                    units
                        .iter()
                        .filter(|c| c.line == line)
                        .min_by(|a, b| {
                            (a.bounds[0] - unit.bounds[0])
                                .abs()
                                .total_cmp(&(b.bounds[0] - unit.bounds[0]).abs())
                        })
                        .map(|c| c.bytes.start)
                }
                M::NextParagraph => units
                    .iter()
                    .skip(index + 1)
                    .find(|c| c.block != unit.block)
                    .map(|c| c.bytes.start),
                M::PreviousParagraph => units[..index]
                    .iter()
                    .rfind(|c| c.block != unit.block)
                    .and_then(|prior| units.iter().find(|c| c.block == prior.block))
                    .map(|c| c.bytes.start),
                M::Paragraph | M::Line => {
                    let same = |c: &&botui_fixed::text::Word| {
                        if motion == M::Line {
                            c.line == unit.line
                        } else {
                            c.block == unit.block
                        }
                    };
                    let first = units.iter().find(same).unwrap().bytes.start;
                    let end = units.iter().rfind(same).unwrap().bytes.end;
                    object = Some(object.as_ref().map_or(first, |r| r.start)..end);
                    Some(end)
                }
            };
            if let Some(next) = next_byte {
                target_byte = next;
                index = units
                    .partition_point(|c| c.bytes.start <= next)
                    .saturating_sub(1);
            }
        }
        let to = units[index].bytes.start;
        let range = object.or_else(|| {
            yank.then(|| {
                if matches!(motion, M::Word | M::BigWord | M::Right) && target_byte > origin {
                    origin..target_byte
                } else if matches!(motion, M::Left | M::BackWord | M::BackBigWord) && to < origin {
                    to..origin
                } else {
                    origin.min(to)..units[at.index.max(index)].bytes.end
                }
            })
        });
        if let Some(range) = range {
            self.start = Some(WordAt {
                index: units.partition_point(|c| c.bytes.end <= range.start),
                ..at
            });
            index = units
                .partition_point(|c| c.bytes.start < range.end)
                .saturating_sub(1);
        }
        self.cursor = Some(WordAt { index, ..at });
        let bounds = units[index].bounds;
        if !self
            .views
            .iter()
            .any(|v| v.page == at.page && v.rect(bounds).is_some())
        {
            self.reveal = Some(PdfRegion {
                page: at.page,
                bounds,
            });
        }
        if yank {
            self.yank_origin = Some(at);
            Action::Copy
        } else {
            Action::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Text {
        let mut text = fixture_text("Alpha interoperability (Привет,мир!) Omega");
        text.cursor = Some(WordAt { page: 1, index: 6 });
        text
    }
    fn fixture_text(value: &str) -> Text {
        let mut page = PageText {
            size: [100., 100.],
            text: value.into(),
            interactive: true,
            ..Default::default()
        };
        for (i, c) in value.char_indices().filter(|(_, c)| *c != '\n') {
            let line = value[..i].bytes().filter(|b| *b == b'\n').count();
            page.characters.push(botui_fixed::text::Word {
                bytes: i..i + c.len_utf8(),
                bounds: [i as f32 / 100., 0.1, (i + c.len_utf8()) as f32 / 100., 0.2],
                line,
                block: line,
            });
        }
        for word in value.split_whitespace() {
            let start = value.find(word).unwrap();
            let line = value[..start].bytes().filter(|b| *b == b'\n').count();
            page.words.push(botui_fixed::text::Word {
                bytes: start..start + word.len(),
                bounds: [
                    start as f32 / 100.,
                    0.1,
                    (start + word.len()) as f32 / 100.,
                    0.2,
                ],
                line,
                block: line,
            });
        }
        let mut text = Text::new(None);
        text.cache.insert(1, Ok(page));
        text.cursor = Some(WordAt { page: 1, index: 0 });
        text
    }
    #[test]
    fn exact_subword_selection_and_old_word_highlights_use_the_same_source() {
        let mut t = fixture();
        t.start = t.cursor;
        t.precise_motion(M::Right, 4, false);
        let (range, quote, rects) = t.selection().unwrap();
        assert_eq!(quote, "inter");
        assert_eq!((range.start.byte, range.end.byte), (6, 11));
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].bounds, [0.06, 0.1, 0.11, 0.2]);
        t.precise_motion(M::InnerWord, 1, false);
        assert_eq!(t.selection().unwrap().1, "interoperability");
        assert_eq!(t.selection().unwrap().2[0].bounds, [0.06, 0.1, 0.22, 0.2]);
    }
    #[test]
    fn character_find_brackets_and_yanks_do_not_widen_to_whole_words() {
        let mut t = fixture();
        t.precise_motion(M::Right, 2, true);
        assert_eq!(t.selection().unwrap().1, "in");
        t = fixture();
        t.precise_motion(
            M::Find {
                ch: 'r',
                forward: true,
                till: false,
                repeat: false,
            },
            2,
            true,
        );
        assert_eq!(t.selection().unwrap().1, "interoper");
        t = fixture();
        t.precise_motion(
            M::Find {
                ch: 'П',
                forward: true,
                till: false,
                repeat: false,
            },
            1,
            false,
        );
        t.precise_motion(
            M::Delimited {
                open: '(',
                close: ')',
                around: false,
            },
            1,
            true,
        );
        assert_eq!(t.selection().unwrap().1, "Привет,мир!");
    }
    #[test]
    fn counted_lines_and_paragraphs_include_the_following_content() {
        for motion in [M::Line, M::Paragraph] {
            let mut t = fixture_text("Alpha\nBeta\nGamma");
            t.precise_motion(motion, 2, true);
            assert_eq!(t.selection().unwrap().1, "Alpha\nBeta");
        }
    }
    #[test]
    fn lazy_glyphs_preserve_both_ends_of_forward_reverse_and_saved_ranges() {
        for reverse in [false, true] {
            let mut t = fixture();
            let new = t.cache[&1].as_ref().unwrap().clone();
            let mut old = new.clone();
            old.characters.clear();
            t.cache.insert(1, Ok(old));
            let a = WordAt { page: 1, index: 1 };
            let b = WordAt { page: 1, index: 2 };
            let (a, b) = if reverse { (b, a) } else { (a, b) };
            t.start = Some(a);
            t.cursor = Some(b);
            t.previous = Some((a, b));
            t.yank_origin = Some(a);
            let before = t.selection().unwrap().1;
            t.remap_units(1, &new);
            t.cache.insert(1, Ok(new));
            assert_eq!(t.selection().unwrap().1, before);
            let (a, b) = t.previous.unwrap();
            t.start = Some(a);
            t.cursor = Some(b);
            assert_eq!(t.selection().unwrap().1, before);
        }
        let mut t = fixture();
        let new = t.cache[&1].as_ref().unwrap().clone();
        let mut old = new.clone();
        old.characters.clear();
        t.cache.insert(1, Ok(old));
        t.start = Some(WordAt { page: 1, index: 1 });
        t.cursor = t.start;
        t.remap_units(1, &new);
        t.cache.insert(1, Ok(new));
        assert_eq!(t.selection().unwrap().1, "interoperability");
    }
    #[test]
    fn whitespace_never_opens_an_unrelated_annotation() {
        let mut t = fixture();
        t.cursor = Some(WordAt { page: 1, index: 5 });
        t.notes.push(
            serde_json::from_value(serde_json::json!({
                "id":"unrelated","block":4,"byte":0,"end":[4,5],"body":"note","updated":0,
                "pdf_regions":[{"page":5,"bounds":[0.1,0.2,0.3,0.4]}]
            }))
            .unwrap(),
        );
        let (_, quote, regions) = t.selection().unwrap();
        assert_eq!(quote, " ");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].bounds, [0.05, 0.1, 0.06, 0.2]);
        assert!(t.active_note().is_none());
    }
    #[test]
    fn clicking_partial_highlight_does_not_expand_past_the_saved_note() {
        let mut t = fixture();
        t.start = t.cursor;
        t.drag = true;
        t.clicks = Some((Position::new(6, 1).into(), Instant::now(), 1));
        t.notes.push(
            serde_json::from_value(serde_json::json!({
                "id":"partial","block":0,"byte":6,"end":[0,11],"body":"note","updated":0,
                "pdf_regions":[{"page":1,"bounds":[0.06,0.1,0.11,0.2]}]
            }))
            .unwrap(),
        );
        t.input(&Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 6,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(t.take_note_click());
        assert_eq!(t.active_note().unwrap().id, "partial");
    }
}
