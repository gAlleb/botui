//! The same contextual actions for reflowable text and PDF text layers.
use crate::{AppTerminal, prompt};
use std::io;

#[derive(Clone, Copy)]
pub enum Action {
    Copy,
    Highlight,
    Search,
    Translate,
    Define,
    AddNote,
}

pub fn choose(term: &mut AppTerminal) -> io::Result<Option<Action>> {
    let actions = [
        Action::Copy,
        Action::Highlight,
        Action::Search,
        Action::Translate,
        Action::Define,
        Action::AddNote,
    ];
    let labels = [
        "Copy",
        "Highlight",
        "Search",
        "Translate",
        "Define",
        "Add note",
    ]
    .map(String::from);
    Ok(prompt::choose(term, "Selected text", &labels, 0)?.map(|i| actions[i]))
}
