use botui_core::Document;
use botui_store::Note;
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};
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
