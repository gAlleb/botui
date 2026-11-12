use botui_core::Document;
use botui_store::Note;
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};
pub(super) fn position(doc: &Document, at: botui_core::Anchor) -> String {
    let book = doc.text_percentage(at);
    let mut label = format!("Book {book:.1}%");
    if let Some((chapter, start)) = doc
        .contents
        .iter()
        .filter_map(|c| c.anchor().map(|a| (c, a)))
        .filter(|(_, a)| *a <= at)
        .max_by_key(|(_, a)| *a)
    {
        let begin = doc.text_percentage(start);
        let end = doc
            .contents
            .iter()
            .filter_map(|c| c.anchor())
            .filter(|a| *a > start)
            .min()
            .map_or(100., |a| doc.text_percentage(a));
        let percent = if end > begin {
            ((book - begin) / (end - begin) * 100.).clamp(0., 100.)
        } else {
            0.
        };
        label.push_str(&format!(" · Section {percent:.1}% · {}", chapter.title));
    }
    label
}
pub fn export(title: &str, doc: Option<&Document>, notes: &[Note]) -> io::Result<PathBuf> {
    if notes.is_empty() {
        return Err(io::Error::other("This book has no notes to export"));
    }
    let dir = crate::cache::downloads()?.with_file_name("exports");
    fs::create_dir_all(&dir)?;
    let mut file = tempfile::Builder::new()
        .prefix("Reading notes - ")
        .suffix(".md")
        .tempfile_in(dir)?;
    writeln!(file, "# {title}\n")?;
    for note in notes {
        let a = note.anchor();
        if doc.is_none() {
            writeln!(file, "## Page {}\n", a.block.saturating_add(1))?;
        }
        if !note.quote.is_empty() {
            for line in note.quote.lines() {
                writeln!(file, "> {line}")?;
            }
            writeln!(file)?;
        } else if let Some(block) = doc.and_then(|d| d.blocks.get(a.block)) {
            let quote = block
                .text
                .get(a.byte..)
                .filter(|s| !s.is_empty())
                .unwrap_or(&block.text);
            writeln!(file, "> {}\n", quote.chars().take(240).collect::<String>())?;
        }
        if let Some(c) = note.color {
            writeln!(file, "Highlight: {}\n", c.label())?;
        }
        if let Some(d) = doc
            && let Some(chapter) = d
                .contents
                .iter()
                .rfind(|c| c.anchor().is_some_and(|at| at <= a))
        {
            writeln!(file, "Section: {}\n", chapter.title)?;
        }
        writeln!(file, "{}\n", note.body)?;
    }
    let (_, path) = file.keep()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use botui_core::{Anchor, Block, ContentsEntry};
    #[test]
    fn position_uses_source_order_and_section_boundaries() {
        let at = |block, byte| Anchor { block, byte };
        let doc = Document {
            blocks: (0..4)
                .map(|_| Block {
                    text: "0123456789".into(),
                    heading: false,
                })
                .collect(),
            contents: vec![
                ContentsEntry::passage(at(2, 0), "Second".into()),
                ContentsEntry::passage(at(0, 0), "First".into()),
            ],
            ..Default::default()
        };
        assert_eq!(
            position(&doc, at(1, 0)),
            "Book 25.0% · Section 50.0% · First"
        );
        assert_eq!(
            position(&doc, at(2, 0)),
            "Book 50.0% · Section 0.0% · Second"
        );
        assert_eq!(
            position(&doc, at(3, 5)),
            "Book 87.5% · Section 75.0% · Second"
        );
        assert_eq!(position(&Document::default(), at(0, 0)), "Book 0.0%");
    }
}
