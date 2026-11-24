//! Poppler can mistake the wide spaces in justified OCR for column gutters.
//! Use the content stream as independent evidence before changing reading order.
use super::*;

fn returns_up_page(page: &PageText) -> bool {
    page.words
        .windows(2)
        .any(|w| w[0].block != w[1].block && w[1].bounds[3] < w[0].bounds[1])
}

pub(super) fn restore(
    pages: &mut [PageText],
    path: &Path,
    first: u32,
    cancel: &AtomicBool,
) -> io::Result<()> {
    let Some(start) = pages.iter().position(returns_up_page) else {
        return Ok(());
    };
    let end = pages.iter().rposition(returns_up_page).unwrap();
    let mut cmd = Command::new("pdftotext");
    cmd.args([
        "-raw",
        "-enc",
        "UTF-8",
        "-f",
        &(first + start as u32).to_string(),
        "-l",
        &(first + end as u32).to_string(),
    ])
    .arg(path.canonicalize()?)
    .arg("-");
    let data = match crate::output(cmd, cancel, 32 << 20) {
        Ok(data) => data,
        Err(err) if cancel.load(std::sync::atomic::Ordering::Relaxed) => return Err(err),
        Err(_) => return Ok(()),
    };
    let Ok(raw) = std::str::from_utf8(&data) else {
        return Ok(());
    };
    for (page, raw) in pages[start..=end].iter_mut().zip(raw.split('\x0c')) {
        if returns_up_page(page) {
            restore_page(page, raw);
        }
    }
    Ok(())
}

fn restore_page(page: &mut PageText, raw: &str) {
    let mut ordered = page.words.clone();
    ordered.sort_by(|a, b| {
        a.bounds[1]
            .total_cmp(&b.bounds[1])
            .then(a.bounds[0].total_cmp(&b.bounds[0]))
    });
    let mut lines = raw.lines().filter(|line| !line.trim().is_empty());
    // Every physical row must also be a complete source line, with exactly
    // the same words. Separate columns or an ambiguous baseline fail closed.
    for row in ordered.chunk_by(|a, b| a.bounds[1] == b.bounds[1]) {
        if row.windows(2).any(|pair| {
            let a = pair[0].bounds;
            let b = pair[1].bounds;
            pair[0].block != pair[1].block
                && (b[0] - a[2]) * page.size[0]
                    > 1.5 * (a[3] - a[1]).min(b[3] - b[1]) * page.size[1]
        }) {
            return;
        }
        let Some(line) = lines.next() else { return };
        if !line
            .split_whitespace()
            .eq(row.iter().map(|w| &page.text[w.bytes.clone()]))
        {
            return;
        }
    }
    if lines.next().is_some() {
        return;
    }
    let mut text = String::with_capacity(page.text.len());
    let mut characters = Vec::with_capacity(page.characters.len());
    let mut line = 0;
    let mut block = 0;
    let mut previous: Option<Word> = None;
    for word in &mut ordered {
        if let Some(prev) = &previous {
            if prev.bounds[1] == word.bounds[1] {
                let start = text.len();
                text.push(' ');
                if !page.characters.is_empty()
                    && let Some(bounds) = interaction::whitespace_box(prev.bounds, word.bounds)
                {
                    characters.push(Word {
                        bytes: start..start + 1,
                        bounds,
                        line,
                        block,
                    });
                }
            } else {
                line += 1;
                text.push('\n');
                // Retain existing paragraph boundaries between intact blocks.
                if prev.block != word.block && prev.bytes.end + 2 == word.bytes.start {
                    block += 1;
                    text.push('\n');
                }
            }
        }
        previous = Some(word.clone());
        let start = text.len();
        text.push_str(&page.text[word.bytes.clone()]);
        let first = page
            .characters
            .partition_point(|c| c.bytes.end <= word.bytes.start);
        for c in page.characters[first..]
            .iter()
            .take_while(|c| c.bytes.start < word.bytes.end)
        {
            characters.push(Word {
                bytes: start + (c.bytes.start - word.bytes.start)
                    ..start + (c.bytes.end - word.bytes.start),
                bounds: c.bounds,
                line,
                block,
            });
        }
        word.bytes = start..text.len();
        word.line = line;
        word.block = block;
    }
    page.text = text;
    page.characters = characters;
    page.words = ordered;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_lines_confirm_interleaved_fragments_but_keep_real_columns() {
        let xml = r#"<page width="100" height="100">
          <block><line><word xMin="2" xMax="28" yMin="20" yMax="27">А</word></line>
                 <line><word xMin="2" xMax="29" yMin="30" yMax="37">Б</word></line></block>
          <block><line><word xMin="32" xMax="52" yMin="20" yMax="27">В</word></line>
                 <line><word xMin="32" xMax="48" yMin="30" yMax="37">Г</word></line></block></page>"#;
        let mut page = parse(xml).unwrap();
        page.characters = page.words.clone();
        restore_page(&mut page, "А\nБ\nВ\nГ");
        assert_eq!(page.text, "А\nБ\n\nВ\nГ");
        let mut columns = page.clone();
        columns.words[2].bounds[0] = 0.49;
        restore_page(&mut columns, "А В\nБ Г");
        assert_eq!(columns.text, page.text);
        restore_page(&mut page, "А В\nБ Г");
        assert_eq!(page.text, "А В\nБ Г");
        assert_eq!(
            page.characters
                .iter()
                .map(|w| &page.text[w.bytes.clone()])
                .collect::<Vec<_>>(),
            ["А", " ", "В", "Б", " ", "Г"]
        );
        assert_eq!(page.characters[2].bounds, [0.32, 0.20, 0.52, 0.27]);
    }
}
