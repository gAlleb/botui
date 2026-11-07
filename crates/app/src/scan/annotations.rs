//! Device-local anchors for scans refer to physical pages, never to a text CFI.
use botui_core::Anchor;
use botui_store::{LocalState, Saved};
use std::{io, path::Path};
pub(super) fn open(
    term: &mut crate::AppTerminal,
    which: usize,
    store: &LocalState,
    path: &Path,
    reading: &mut Saved,
) -> io::Result<(Option<u32>, String)> {
    let at = reading.anchor;
    let choices: Vec<(Anchor, String)> = match which {
        18 => {
            let add = !reading.marks.contains(&at);
            reading.marks = store.set_bookmark(reading, at, add)?;
            let message = if add {
                "Page bookmarked"
            } else {
                "Bookmark removed"
            };
            return Ok((None, message.into()));
        }
        19 => {
            if let Some(current) = store.load()? {
                reading.marks = current.marks;
            }
            let marks = &reading.marks;
            let labels: Vec<_> = marks
                .iter()
                .map(|a| format!(" Page {}", a.block + 1))
                .collect();
            let initial = marks.partition_point(|mark| *mark < at);
            let choice = crate::prompt::choose_removable(term, "Page bookmarks", &labels, initial)?;
            return match choice {
                Some((i, true)) => {
                    reading.marks = store.set_bookmark(reading, marks[i], false)?;
                    Ok((None, "Bookmark removed".into()))
                }
                Some((i, false)) => Ok((Some(marks[i].block as u32 + 1), String::new())),
                None => Ok((None, String::new())),
            };
        }
        20 => {
            let notes = store.notes()?;
            let initial = notes
                .iter()
                .find(|v| v.anchor() == at)
                .map_or("", |v| &v.body);
            if let Some(body) = crate::prompt::edit(
                term,
                "Page note · clear to remove",
                "[ Save ↵ ]",
                initial,
                |_| {},
            )? {
                store.edit_note(at, body)?;
            }
            return Ok((None, String::new()));
        }
        21 => store
            .notes()?
            .iter()
            .map(|v| {
                (
                    v.anchor(),
                    format!(" Page {} · {}", v.anchor().block + 1, v.body),
                )
            })
            .collect(),
        22 => {
            let title = path.file_stem().unwrap_or_default().to_string_lossy();
            let output = crate::reader::notes::export(&title, None, &store.notes()?)?;
            return Ok((None, format!("Saved {}", output.display())));
        }
        _ => return Ok((None, String::new())),
    };
    let initial = choices.iter().position(|(a, _)| *a >= at).unwrap_or(0);
    let labels: Vec<_> = choices.iter().map(|v| v.1.clone()).collect();
    let i = crate::prompt::choose(term, "Page notes", &labels, initial)?;
    Ok((i.map(|i| choices[i].0.block as u32 + 1), String::new()))
}
