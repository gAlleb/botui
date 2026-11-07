use super::{Reader, bookmarks::ServerMarks};
use crate::{AppTerminal, prompt};
use botui_core::Anchor;
use botui_store::LocalState;
use std::io;

enum Origin {
    Local(usize),
    BookOrbit(u64),
}
struct Mark {
    at: Option<Anchor>,
    label: String,
    origin: Origin,
}

impl Reader {
    fn listed_marks(&self, remote: Option<&ServerMarks>) -> Vec<Mark> {
        let mut rows: Vec<_> = self
            .marks
            .iter()
            .enumerate()
            .map(|(i, &at)| {
                let text = self
                    .doc
                    .blocks
                    .get(at.block)
                    .and_then(|b| b.text.get(at.byte..));
                let title = text
                    .filter(|s| !s.is_empty())
                    .map(|s| s.chars().take(120).collect())
                    .unwrap_or_else(|| format!("Passage {}", at.block.saturating_add(1)));
                Mark {
                    at: text.map(|_| at),
                    origin: Origin::Local(i),
                    label: format!("Local · {title}"),
                }
            })
            .collect();
        if let Some(remote) = remote {
            rows.extend(
                remote
                    .entries(&self.doc)
                    .into_iter()
                    .map(|(id, at, title)| Mark {
                        at,
                        origin: Origin::BookOrbit(id),
                        label: format!("BookOrbit · {}", title.replace(char::is_control, " ")),
                    }),
            );
        }
        // Put unavailable entries last while preserving server order among duplicates.
        rows.sort_by_key(|row| (row.at.is_none(), row.at));
        rows
    }
    pub(super) fn manage_marks(
        &mut self,
        term: &mut AppTerminal,
        mut remote: Option<&mut ServerMarks>,
        store: Option<&LocalState>,
    ) -> io::Result<()> {
        let mut selected = None;
        loop {
            let rows = self.listed_marks(remote.as_deref());
            let initial = *selected.get_or_insert_with(|| {
                rows.iter()
                    .rposition(|r| r.at.is_some_and(|v| v <= self.pager.anchor()))
                    .unwrap_or(0)
            });
            let labels: Vec<_> = rows
                .iter()
                .map(|r| {
                    if r.at.is_none() {
                        format!("{} · unavailable in this file", r.label)
                    } else {
                        r.label.clone()
                    }
                })
                .collect();
            let Some((i, delete)) = prompt::choose_removable(
                term,
                &format!("Bookmarks: {}", rows.len()),
                &labels,
                initial,
            )?
            else {
                return Ok(());
            };
            selected = Some(i);
            let row = &rows[i];
            if !delete {
                if let Some(at) = row.at {
                    self.jump_to(at);
                    self.dirty = true;
                    return Ok(());
                }
                prompt::message(
                    term,
                    "Bookmark location",
                    "This bookmark has no usable address in this edition.",
                )?;
                continue;
            }
            let result = match row.origin {
                Origin::BookOrbit(id) => remote
                    .as_deref_mut()
                    .ok_or_else(|| io::Error::other("BookOrbit is unavailable"))
                    .and_then(|r| r.remove(id).map_err(io::Error::other)),
                Origin::Local(i) => {
                    // Commit the updated list before acknowledging a local deletion.
                    let mut snapshot = self.snapshot();
                    snapshot.marks.remove(i);
                    store
                        .ok_or_else(|| io::Error::other("Local storage is unavailable"))
                        .and_then(|s| s.save(&snapshot))
                        .map(|()| {
                            self.marks = snapshot.marks;
                            self.dirty = false;
                        })
                }
            };
            if let Err(e) = result {
                prompt::message(term, "Could not delete bookmark", &e.to_string())?;
            }
        }
    }
}
