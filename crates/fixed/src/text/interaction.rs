use super::*;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq)]
pub enum LinkTarget {
    Page { page: u32, point: [f32; 2] },
    Uri(String),
}
#[derive(Clone, Debug)]
pub struct Link {
    pub bounds: [f32; 4],
    pub target: LinkTarget,
}

pub(super) fn enrich(path: &Path, page: u32, cancel: &AtomicBool, result: &mut PageText) {
    let mut cmd = Command::new("python3");
    cmd.args(["-I", "-c", include_str!("interaction.py")])
        .arg(path.canonicalize().unwrap_or_else(|_| path.to_owned()))
        .arg(page.to_string());
    // This optional helper is tried once per cached page. Missing GI, damaged
    // layout or cancellation leaves the existing word interaction intact.
    if let Ok(data) = crate::output(cmd, cancel, 8 << 20)
        && let Ok(xml) = std::str::from_utf8(&data)
    {
        apply(xml, result);
    }
}

fn number(n: roxmltree::Node<'_, '_>, key: &str) -> Option<f32> {
    n.attribute(key)?
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
}
fn bounds(n: roxmltree::Node<'_, '_>) -> Option<[f32; 4]> {
    let r = [
        number(n, "x0")?,
        number(n, "y0")?,
        number(n, "x1")?,
        number(n, "y1")?,
    ];
    (r[0] <= r[2] && r[1] <= r[3]).then(|| r.map(|v| v.clamp(0., 1.)))
}
fn apply(xml: &str, result: &mut PageText) {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return;
    };
    let root = doc.root_element();
    if number(root, "width")
        .zip(number(root, "height"))
        .is_none_or(|(w, h)| (w - result.size[0]).abs() > 0.1 || (h - result.size[1]).abs() > 0.1)
    {
        return;
    }
    result.links = root
        .children()
        .filter(|n| n.has_tag_name("link"))
        .filter_map(|n| {
            let target = if let Some(uri) = n.attribute("uri") {
                // Launch actions, file links and arbitrary URI handlers are not web links.
                if !["https://", "http://", "mailto:"]
                    .iter()
                    .any(|p| uri.starts_with(p))
                {
                    return None;
                }
                LinkTarget::Uri(uri.to_owned())
            } else {
                let page = n
                    .attribute("page")?
                    .parse::<u32>()
                    .ok()
                    .filter(|p| *p > 0)?;
                LinkTarget::Page {
                    page,
                    point: [number(n, "x")?.clamp(0., 1.), number(n, "y")?.clamp(0., 1.)],
                }
            };
            Some(Link {
                bounds: bounds(n)?,
                target,
            })
        })
        .collect();
    let glyphs: Option<Vec<_>> = root
        .children()
        .filter(|n| n.has_tag_name("char"))
        .map(|n| {
            let mut chars = n.text()?.chars();
            let ch = chars.next()?;
            chars.next().is_none().then_some((
                ch,
                if ch.is_whitespace() {
                    [0.; 4]
                } else {
                    bounds(n)?
                },
            ))
        })
        .collect();
    let Some(glyphs) = glyphs else { return };
    let source: Vec<_> = glyphs.iter().filter(|(c, _)| !c.is_whitespace()).collect();
    let canonical: Vec<_> = result
        .text
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .collect();
    // Reject the whole mapping if ligatures, reading order or character counts
    // disagree. Existing offsets and annotations must never be remapped by guesswork.
    if source.len() != canonical.len()
        || source.iter().zip(&canonical).any(|((a, _), (_, b))| a != b)
    {
        return;
    }
    let mut characters: Vec<Word> = Vec::with_capacity(canonical.len());
    let mut word_index = 0;
    let mut source_index = 0;
    for (byte, ch) in result.text.char_indices() {
        if ch.is_whitespace() {
            if ch == ' '
                && let Some(before) = characters.last()
                && let Some((_, after)) = glyphs[source_index..]
                    .iter()
                    .find(|(c, _)| !c.is_whitespace())
                && let Some(geometry) = whitespace_box(before.bounds, *after)
            {
                let word = &result.words[word_index];
                characters.push(Word {
                    bytes: byte..byte + 1,
                    bounds: geometry,
                    line: word.line,
                    block: word.block,
                });
            }
            continue;
        }
        while glyphs
            .get(source_index)
            .is_some_and(|(c, _)| c.is_whitespace())
        {
            source_index += 1;
        }
        let geometry = &glyphs[source_index].1;
        source_index += 1;
        while result
            .words
            .get(word_index)
            .is_some_and(|w| w.bytes.end <= byte)
        {
            word_index += 1;
        }
        let Some(word) = result
            .words
            .get(word_index)
            .filter(|w| w.bytes.contains(&byte))
        else {
            return;
        };
        // Verify the glyph belongs to the same rendered word, including rotation.
        if geometry[0] < word.bounds[0] - 0.01
            || geometry[2] > word.bounds[2] + 0.01
            || geometry[1] < word.bounds[1] - 0.01
            || geometry[3] > word.bounds[3] + 0.01
        {
            return;
        }
        if geometry[0] == geometry[2] || geometry[1] == geometry[3] {
            // A combining accent can have no advance of its own. Keep its
            // source bytes with its measured base letter for selection/copy.
            let end = byte + ch.len_utf8();
            if let Some(base) = characters.last_mut()
                && base.bytes.start >= word.bytes.start
                && base.bytes.end == byte
                && result.text[base.bytes.start..end].graphemes(true).count() == 1
            {
                base.bytes.end = end;
                continue;
            }
            return;
        }
        characters.push(Word {
            bytes: byte..byte + ch.len_utf8(),
            bounds: *geometry,
            line: word.line,
            block: word.block,
        });
    }
    result.characters = characters;
}

// GLib whitespace boxes can be reversed after intrinsic rotation. Whitespace
// has no ink: use the measured gap between its adjacent glyphs, never estimate
// character widths inside a word.
pub(super) fn whitespace_box(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let r = if a[2] <= b[0] {
        [a[2], a[1].max(b[1]), b[0], a[3].min(b[3])]
    } else if b[2] <= a[0] {
        [b[2], a[1].max(b[1]), a[0], a[3].min(b[3])]
    } else if a[3] <= b[1] {
        [a[0].max(b[0]), a[3], a[2].min(b[2]), b[1]]
    } else if b[3] <= a[1] {
        [a[0].max(b[0]), b[3], a[2].min(b[2]), a[1]]
    } else {
        return None;
    };
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> PageText {
        PageText {
            size: [100., 100.],
            text: "Жи fi".into(),
            words: vec![
                Word {
                    bytes: 0..4,
                    bounds: [0.1, 0.2, 0.3, 0.3],
                    line: 0,
                    block: 0,
                },
                Word {
                    bytes: 5..7,
                    bounds: [0.4, 0.2, 0.6, 0.3],
                    line: 0,
                    block: 0,
                },
            ],
            ..Default::default()
        }
    }
    const GEOMETRY: &str = r#"<interaction width="100" height="100">
        <char x0="0.1" y0="0.2" x1="0.22" y1="0.3">Ж</char>
        <char x0="0.22" y0="0.2" x1="0.3" y1="0.3">и</char>
        <char x0="0.3" y0="0.2" x1="0.4" y1="0.3"> </char>
        <char x0="0.4" y0="0.2" x1="0.51" y1="0.3">f</char>
        <char x0="0.51" y0="0.2" x1="0.6" y1="0.3">i</char>
        <link x0="0.1" y0="0.2" x1="0.3" y1="0.3" page="3" x="0" y="0.4"/>
        <link x0="0.4" y0="0.2" x1="0.6" y1="0.3" uri="https://example.invalid/test"/>
        <link x0="0.4" y0="0.2" x1="0.6" y1="0.3" uri="file:///tmp/not-a-web-link"/>
        </interaction>"#;

    #[test]
    fn genuine_unicode_geometry_preserves_text_offsets_and_saved_word_boxes() {
        let mut p = page();
        apply(GEOMETRY, &mut p);
        assert_eq!(p.text, "Жи fi");
        assert_eq!(p.characters.len(), 5);
        assert_eq!(p.characters[1].bytes, 2..4);
        assert_eq!(p.characters[1].bounds, [0.22, 0.2, 0.3, 0.3]);
        assert_eq!(p.characters[2].bytes, 4..5);
        assert_eq!(p.words[0].bounds, [0.1, 0.2, 0.3, 0.3]);
        assert_eq!(p.links.len(), 2);
        assert_eq!(
            p.links[0].target,
            LinkTarget::Page {
                page: 3,
                point: [0., 0.4]
            }
        );
    }

    #[test]
    fn zero_width_accent_is_copied_with_the_base_letter() {
        let mut p = page();
        p.text.insert(4, '\u{301}');
        p.words[0].bytes.end += 2;
        p.words[1].bytes = 7..9;
        let xml = GEOMETRY.replace(
            ">и</char>",
            ">и</char><char x0=\"0.3\" x1=\"0.3\" y0=\"0.2\" y1=\"0.3\">\u{301}</char>",
        );
        apply(&xml, &mut p);
        assert_eq!(p.characters.len(), 5);
        assert_eq!(&p.text[p.characters[1].bytes.clone()], "и\u{301}");
        assert_eq!(p.characters[1].bounds, [0.22, 0.2, 0.3, 0.3]);
        assert_eq!(p.characters[3].bytes, 7..8);
    }

    #[test]
    fn reading_order_ligatures_and_rotation_must_agree_before_using_glyphs() {
        for xml in [
            GEOMETRY.replace(">Ж<", ">и<"),
            GEOMETRY.replace(">f<", ">ﬁ<"),
            GEOMETRY.replace("width=\"100\"", "width=\"200\""),
            GEOMETRY.replace("x0=\"0.22\"", "x0=\"0.02\""),
        ] {
            let mut p = page();
            apply(&xml, &mut p);
            assert!(p.characters.is_empty());
            assert_eq!(p.units().len(), 2);
            assert_eq!(p.text, "Жи fi");
        }
    }
}
