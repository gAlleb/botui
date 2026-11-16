//! Search the local note files and resolve their content-addressed editions.
use super::{
    Shelf,
    background::{Ticket, Worker},
};
use crate::{
    AppTerminal,
    live_search::{Item, Search, Source},
};
use botui_store::{HistoryEntry, LibraryNotes, LocalState, Note};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fs, io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::TryRecvError,
    },
};
use unicode_truncate::UnicodeTruncateStr;

#[derive(Clone)]
pub(crate) struct NoteId {
    edition: String,
    id: String,
}

impl NoteId {
    /// Resolve again inside the reader, in case the edition or its notes changed while browsing.
    pub fn resolve(&self, store: Option<&LocalState>) -> io::Result<Note> {
        let store = store.filter(|s| s.edition_id() == self.edition).ok_or_else(||
            io::Error::other("The book file has changed. Open the original edition to follow this annotation.")
        )?;
        store
            .notes()?
            .into_iter()
            .find(|n| n.id == self.id)
            .ok_or_else(|| io::Error::other("This annotation has been removed."))
    }
}

#[derive(Clone)]
struct Passage {
    target: NoteId,
    path: Option<PathBuf>,
    title: String,
    body: String,
}

type Hit = Item<(String, String), Passage>;
type Indexed = io::Result<(Vec<Hit>, usize)>;

struct Index {
    job: Option<Ticket<Indexed>>,
}

fn collect(stop: &AtomicBool) -> Indexed {
    let library = LibraryNotes::on_device()?;
    let mut hits = Vec::new();
    if library.editions.is_empty() {
        return Ok((hits, library.unreadable));
    }
    let mut candidates: Vec<_> = HistoryEntry::on_device()?
        .into_iter()
        .map(|h| h.path)
        .collect();
    match fs::read_dir(crate::cache::downloads()?) {
        Ok(entries) => candidates.extend(entries.flatten().map(|e| e.path())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e),
    }
    let mut seen = HashSet::new();
    let mut paths = HashMap::new();
    for path in candidates {
        if stop.load(Ordering::Acquire) {
            return Ok((hits, 0));
        }
        if paths.len() == library.editions.len() {
            break;
        }
        if seen.insert(path.clone())
            && botui_formats::FileKind::from_path(&path).is_some()
            && path.is_file()
            && let Ok(state) = LocalState::for_file(&path)
            && library.editions.contains_key(state.edition_id())
        {
            paths.entry(state.edition_id().to_owned()).or_insert(path);
        }
    }
    for (edition, notes) in library.editions {
        let path = paths.remove(&edition);
        let title = path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || format!("Unavailable edition {}", &edition[..12]),
            |n| n.to_string_lossy().into_owned(),
        );
        for note in notes {
            let body = [note.quote, note.body]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" — ");
            let label = super::single_line(format!("{title} · {body}"));
            hits.push(Item {
                key: (edition.clone(), note.id.clone()),
                name: label.clone(),
                text: label.clone(),
                label,
                action: Passage {
                    target: NoteId {
                        edition: edition.clone(),
                        id: note.id,
                    },
                    path: path.clone(),
                    title: super::single_line(title.clone()),
                    body,
                },
            });
        }
    }
    Ok((hits, library.unreadable))
}

impl Source for Index {
    type Key = (String, String);
    type Action = Passage;
    fn collect(&mut self, batch: &mut Vec<Hit>) -> io::Result<()> {
        let Some(job) = &self.job else { return Ok(()) };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                Err(io::Error::other("Annotation search interrupted"))
            }
        };
        self.job = None;
        let (rows, unreadable) = result?;
        batch.extend(rows);
        if unreadable > 0 {
            return Err(io::Error::other(format!(
                "Skipped {unreadable} unreadable annotation file(s)"
            )));
        }
        Ok(())
    }
    fn busy(&self) -> bool {
        self.job.is_some()
    }
    fn row_label<'a>(action: &Passage, label: &'a str, width: u16) -> Cow<'a, str> {
        let limit = usize::from(width / 3).max(1);
        let (short, _) = action.title.as_str().unicode_truncate(limit);
        if short.len() == action.title.len() {
            return Cow::Borrowed(label);
        }
        Cow::Owned(super::single_line(format!("{short}… · {}", action.body)))
    }

    fn detail(action: &Passage) -> Option<Cow<'_, str>> {
        Some(
            action
                .path
                .as_ref()
                .map_or(Cow::Borrowed("Book file unavailable on this device"), |p| {
                    Cow::Owned(super::single_line(p.to_string_lossy().into_owned()))
                }),
        )
    }
}

impl Shelf {
    pub fn browse_annotations(
        &mut self,
        term: &mut AppTerminal,
        notices: &mut Vec<String>,
    ) -> crate::Result<()> {
        loop {
            let index = Index {
                job: Some(Worker::new().start(collect)),
            };
            let mut search = Search::new(index, "Library highlights & notes");
            let Some(passage) = search.choose(term, |f| self.draw(f))? else {
                return Ok(());
            };
            if let Some(path) = &passage.path {
                if let Err(e) =
                    crate::open_local_doc(term, path, None, notices, Some(&passage.target))
                {
                    crate::prompt::message(term, "Could not open passage", &e.to_string())?;
                }
            } else {
                crate::prompt::message(
                    term,
                    "Book file unavailable",
                    &format!(
                        "{}\n\nOpen a local copy of this edition to follow the annotation.",
                        passage.body
                    ),
                )?;
            }
        }
    }
}
