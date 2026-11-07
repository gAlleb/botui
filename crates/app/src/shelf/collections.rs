use super::{Command, Loaded, Row, Section, Shelf};
use crate::{AppTerminal, prompt};
use botui_bookorbit::collections::Membership;
use std::io;

impl Shelf {
    pub(super) fn book_collections(&mut self, book: u64) {
        let Some(session) = self.session.clone() else {
            return;
        };
        self.fetch("Loading book collections…", move |_| {
            Ok(Loaded::Memberships(
                book,
                session.with_token(|a, t| a.memberships(t, book))?,
            ))
        });
    }

    pub(super) fn show_memberships(&mut self, book: u64, items: Vec<Membership>) {
        let rows = items
            .into_iter()
            .map(|item| {
                let add = item.member_count == 0;
                let verb = if add { "Add to" } else { "Remove from" };
                Row::new(
                    format!("{verb} {}", item.name),
                    Command::Membership {
                        book,
                        collection: item.id,
                        add,
                    },
                )
            })
            .collect();
        let pane = super::Pane::new(
            Section::Memberships(book),
            "Collections for this book",
            rows,
        );
        if matches!(self.current().section, Section::Memberships(_)) {
            *self.current_mut() = pane;
        } else {
            self.stack.push(pane);
        }
    }

    pub(super) fn deletable_collection(&self) -> Option<&botui_bookorbit::Collection> {
        let collection = match &self.current().section {
            Section::Books {
                scope: super::Scope::Collection(c),
                ..
            } => c,
            Section::Collections => {
                let pane = self.current();
                let row = pane.rows.get(pane.selection.selected()?)?;
                let Command::Books(super::Scope::Collection(c), _) = &row.action else {
                    return None;
                };
                c
            }
            _ => return None,
        };
        collection.is_owner.then_some(collection)
    }

    pub(super) fn delete_collection(&mut self, term: &mut AppTerminal) -> io::Result<()> {
        let (Some(c), Some(session)) = (self.deletable_collection(), self.session.clone()) else {
            return Ok(());
        };
        if prompt::confirm_delete_collection(term, &c.name, "your BookOrbit account")? {
            let id = c.id;
            self.fetch("Deleting collection…", move |_| {
                session.with_token(|a, t| a.delete_collection(t, id))?;
                Ok(Loaded::CollectionChanged(None))
            });
            if let Some(job) = &mut self.pending {
                job.saving = true;
            }
        }
        Ok(())
    }

    pub(super) fn new_collection(&mut self, term: &mut AppTerminal) -> io::Result<()> {
        let Some(session) = self.session.clone() else {
            return Ok(());
        };
        let book = match self.current().section {
            Section::Memberships(book) => Some(book),
            Section::Collections => None,
            _ => return Ok(()),
        };
        let Some(name) = prompt::collection_name(term, |f| self.draw(f))? else {
            return Ok(());
        };
        self.fetch("Creating collection…", move |_| {
            let created = session.with_token(|a, t| a.create_collection(t, &name))?;
            if let Some(id) = book {
                session
                    .with_token(|a, t| a.set_membership(t, created.id, id, true))
                    .map_err(|e| format!("Collection created. Could not add book: {e}"))?;
            }
            Ok(Loaded::CollectionChanged(book))
        });
        self.pending.as_mut().unwrap().saving = true;
        Ok(())
    }

    pub(super) fn change_membership(&mut self, book: u64, collection: u64, add: bool) {
        let Some(session) = self.session.clone() else {
            return;
        };
        self.fetch("Updating collection…", move |_| {
            session.with_token(|a, t| a.set_membership(t, collection, book, add))?;
            Ok(Loaded::CollectionChanged(Some(book)))
        });
        self.pending.as_mut().unwrap().saving = true;
    }
}
