//! Fit a publisher's section title and its independent position on the top border.
use ratatui::text::Line;
use unicode_truncate::UnicodeTruncateStr;

#[derive(Clone, Copy)]
pub(crate) struct SectionPosition<'a> {
    pub index: usize,
    pub total: usize,
    pub title: Option<&'a str>,
    pub percent: Option<u8>,
}

impl SectionPosition<'_> {
    pub fn compact_label(self) -> String {
        let count = format!("{}/{}", self.index, self.total);
        match self.percent {
            Some(percent) => format!("{count} · {percent}%"),
            None => count,
        }
    }

    fn fit(self, room: usize, compact: String) -> String {
        let compact_width = Line::from(compact.as_str()).width();
        if compact_width > room {
            // Keep complete numbers even where the spaced separators cannot fit.
            let count = format!("{}/{}", self.index, self.total);
            let percent = self
                .percent
                .map(|value| format!("{value}%"))
                .unwrap_or_default();
            let tight = format!("{count} {percent}");
            return [tight, count, percent]
                .into_iter()
                .find(|s| s.len() <= room)
                .unwrap_or_default();
        }
        let name = shortened(self.title, room.saturating_sub(compact_width + 3));
        if name.is_empty() {
            return compact;
        }
        let count = format!("{}/{}", self.index, self.total);
        match self.percent {
            Some(percent) => format!("{count} · {name} · {percent}%"),
            None => format!("{count} · {name}"),
        }
    }
}

fn shortened(title: Option<&str>, room: usize) -> String {
    let text = title.unwrap_or_default().trim();
    let (prefix, _) = text.unicode_truncate(room);
    if prefix.len() == text.len() {
        text.to_owned()
    } else if room >= 4 {
        let (prefix, _) = text.unicode_truncate(room - 1);
        format!("{}…", prefix.trim_end())
    } else {
        String::new()
    }
}

/// Two cells pad each label. Keep at least one visible border cell between
/// labels, then share the space left after the section count and percentage.
pub(super) fn top_labels(
    width: u16,
    book: Option<&str>,
    section: Option<SectionPosition<'_>>,
) -> (String, String) {
    let room = usize::from(width).saturating_sub(2);
    let Some(section) = section else {
        return (shortened(book, room), String::new());
    };
    let compact = section.compact_label();
    let available = room.saturating_sub(Line::from(compact.as_str()).width() + 3);
    let book_width = Line::from(book.unwrap_or_default().trim()).width();
    let chapter_width = Line::from(section.title.unwrap_or_default().trim()).width();
    // Account for the extra separator before a chapter name, otherwise two
    // long names can unnecessarily push one another out at narrow widths.
    let name_room = available.saturating_sub(3);
    let book_min = book_width.min(4);
    let chapter_min = chapter_width.min(4);
    let book_share = if chapter_width > 0 && name_room >= book_min + chapter_min {
        name_room
            .div_ceil(2)
            .clamp(book_min, name_room - chapter_min)
            .min(book_width)
    } else if available >= book_min {
        available.min(book_width)
    } else {
        0
    };
    let section_room = if book_share == 0 {
        room
    } else {
        room - book_share - 3
    };
    let right = section.fit(section_room, compact);
    let book_room = if right.is_empty() {
        room
    } else {
        room.saturating_sub(Line::from(right.as_str()).width() + 3)
    };
    (shortened(book, book_room), right)
}
