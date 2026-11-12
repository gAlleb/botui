use super::mapping::Mapper;
use botui_bookorbit::{
    Result,
    annotations::{Annotation, New},
};
use botui_store::{LocalState, Note};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub trait Remote {
    fn list(&self) -> Result<Vec<Annotation>>;
    fn create(&self, new: &New) -> Result<Annotation>;
    fn edit(&self, id: u64, note: &str, color: &str) -> Result<Annotation>;
    fn delete(&self, id: u64) -> Result<()>;
}
#[derive(Clone, Serialize, Deserialize)]
struct Link {
    local: Note,
    remote: Annotation,
}
#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    local: Note,
    new: New,
    before: BTreeSet<u64>,
}
#[derive(Default, Serialize, Deserialize)]
struct State {
    links: BTreeMap<String, Link>,
    #[serde(default)]
    pending: BTreeMap<String, Pending>,
}
#[derive(Default)]
pub struct Report {
    pub conflicts: usize,
    pub unlocated: usize,
    pub local_only: usize,
    pub uncertain: usize,
}
impl Report {
    pub fn message(&self) -> String {
        let mut parts = vec!["BookOrbit annotations synchronized".to_owned()];
        if self.conflicts > 0 {
            parts.push(format!("{} conflicts: both versions kept", self.conflicts));
        }
        if self.unlocated > 0 {
            parts.push(format!(
                "{} server marks have no exact location here",
                self.unlocated
            ));
        }
        if self.local_only > 0 {
            parts.push(format!(
                "{} local-only marks (unsupported position)",
                self.local_only
            ));
        }
        if self.uncertain > 0 {
            parts.push(format!(
                "{} uploads unconfirmed; Space a s to review/retry",
                self.uncertain
            ));
        }
        parts.join(" · ")
    }
}
fn load(v: serde_json::Value) -> std::io::Result<State> {
    if v.is_null() {
        Ok(State::default())
    } else {
        serde_json::from_value(v).map_err(std::io::Error::other)
    }
}
fn change<T>(
    store: &LocalState,
    scope: &str,
    f: impl FnOnce(&mut Vec<Note>, &mut State) -> T,
) -> Result<T> {
    Ok(store.update_annotations(scope, |notes, v| {
        let mut state = load(v.clone())?;
        let result = f(notes, &mut state);
        *v = serde_json::to_value(state)?;
        Ok(result)
    })?)
}
fn same_remote(a: &Annotation, b: &Annotation) -> bool {
    a.cfi == b.cfi
        && a.pdf == b.pdf
        && a.jump_file_id == b.jump_file_id
        && a.text == b.text
        && a.color == b.color
        && a.style == b.style
        && a.note == b.note
}
fn snapshot(store: &LocalState, scope: &str) -> Result<(Vec<Note>, State)> {
    let (notes, v) = store.annotation_state(scope)?;
    Ok((notes, load(v)?))
}
fn ack(store: &LocalState, scope: &str, note: Note, remote: Annotation) -> Result<()> {
    change(store, scope, |_, s| {
        s.pending.remove(&note.id);
        s.links.insert(
            note.id.clone(),
            Link {
                local: note,
                remote,
            },
        );
    })
}
/// Three-way reconciliation. Remote calls never hold the notes/editor lock.
/// A local edit during a request remains different from the acknowledged baseline.
pub fn sync(
    store: &LocalState,
    scope: &str,
    file: u64,
    mapper: &mut Mapper,
    api: &impl Remote,
    retry: bool,
) -> Result<Report> {
    let _lock = store.annotation_sync_lock()?;
    let mut report = Report::default();
    let mut remote = api.list()?; // A failed/unsupported endpoint is NEVER an empty list.
    let (_, state) = snapshot(store, scope)?;
    for (id, pending) in state.pending {
        let matches: Vec<_> = remote
            .iter()
            .filter(|r| !pending.before.contains(&r.id) && pending.new.matches(r))
            .collect();
        if matches.len() == 1 {
            ack(store, scope, pending.local, matches[0].clone())?;
        } else if retry && matches.is_empty() {
            change(store, scope, |_, s| {
                s.pending.remove(&id);
            })?;
        } else {
            report.uncertain += 1;
        }
    }
    let (notes, state) = snapshot(store, scope)?;
    for (id, link) in state.links {
        let local = notes.iter().find(|n| n.id == id);
        let current = remote.iter().find(|r| r.id == link.remote.id).cloned();
        match (local, current) {
            (None, None) => {
                change(store, scope, |_, s| {
                    s.links.remove(&id);
                })?;
            }
            (Some(local), None) => {
                let unchanged = local == &link.local;
                change(store, scope, |notes, s| {
                    if unchanged {
                        notes.retain(|n| n.id != id || n != local);
                    }
                    s.links.remove(&id);
                })?;
                if !unchanged {
                    report.conflicts += 1;
                }
            }
            (local, Some(current)) => {
                let remote_changed = !same_remote(&current, &link.remote);
                let local_changed = local != Some(&link.local);
                if remote_changed {
                    let Some(mut imported) = mapper.import(&current, id.clone(), file) else {
                        report.unlocated += 1;
                        continue;
                    };
                    change(store, scope, |notes, s| {
                        // Re-read under lock: edits made while GET was in flight also survive.
                        if let Some(at) = notes.iter().position(|n| n.id == id) {
                            if notes[at] != link.local {
                                let mut copy = notes[at].clone();
                                copy.id = format!("conflict-{}-{}", copy.id, copy.updated);
                                if !notes.iter().any(|n| n.id == copy.id) {
                                    notes.push(copy);
                                }
                                report.conflicts += 1;
                            }
                            notes.retain(|n| n.id != id);
                        } else if local_changed {
                            report.conflicts += 1;
                        }
                        // Preserve local no-fill comments if only their remote body changed.
                        if current.color == link.remote.color {
                            imported.color = link.local.color;
                        }
                        notes.push(imported.clone());
                        s.links.insert(
                            id.clone(),
                            Link {
                                local: imported,
                                remote: current,
                            },
                        );
                    })?;
                } else if let Some(local) = local {
                    if local_changed {
                        // PATCH cannot move a range; leave unsupported geometry edits local.
                        if local.range() != link.local.range()
                            || local.pdf_regions != link.local.pdf_regions
                            || local.quote != link.local.quote
                        {
                            report.local_only += 1;
                            continue;
                        }
                        let color = Mapper::edit_color(local, &link.local, &current);
                        let saved = api.edit(current.id, &local.body, &color)?;
                        ack(store, scope, local.clone(), saved)?;
                    }
                } else {
                    match api.delete(current.id) {
                        Ok(()) => (),
                        Err(e) if botui_bookorbit::is_not_found(e.as_ref()) => {
                            if api.list()?.iter().any(|r| r.id == current.id) {
                                return Err(e);
                            }
                        }
                        Err(e) => return Err(e),
                    }
                    change(store, scope, |_, s| {
                        s.links.remove(&id);
                    })?;
                    remote.retain(|r| r.id != current.id);
                }
            }
        }
    }
    // Adopt server records before exporting local ones; links are never inferred from just a quote.
    let (_, state) = snapshot(store, scope)?;
    let known: BTreeSet<_> = state.links.values().map(|l| l.remote.id).collect();
    let pending_ids: BTreeSet<_> = state
        .pending
        .values()
        .flat_map(|p| {
            remote
                .iter()
                .filter(|r| !p.before.contains(&r.id) && p.new.matches(r))
                .map(|r| r.id)
        })
        .collect();
    for r in &remote {
        if known.contains(&r.id) || pending_ids.contains(&r.id) {
            continue;
        }
        let id = format!("bookorbit-{scope}-{}", r.id);
        let Some(imported) = mapper.import(r, id.clone(), file) else {
            report.unlocated += 1;
            continue;
        };
        change(store, scope, |notes, s| {
            if !notes.iter().any(|n| n.id == id) {
                notes.push(imported.clone());
            }
            s.links.insert(
                id,
                Link {
                    local: imported,
                    remote: r.clone(),
                },
            );
        })?;
    }
    let (notes, state) = snapshot(store, scope)?;
    for local in notes {
        if state.links.contains_key(&local.id) || state.pending.contains_key(&local.id) {
            continue;
        }
        // Do not publish another account's downloaded marks into this account.
        if local.id.starts_with("bookorbit-")
            && !local.id.starts_with(&format!("bookorbit-{scope}-"))
        {
            continue;
        }
        let Some(new) = mapper.export(&local, file) else {
            report.local_only += 1;
            continue;
        };
        let pending = Pending {
            local: local.clone(),
            new: new.clone(),
            before: remote.iter().map(|r| r.id).collect(),
        };
        let accepted = change(store, scope, |notes, s| {
            if !notes.contains(&local) {
                return false;
            }
            s.pending.insert(local.id.clone(), pending);
            true
        })?;
        if !accepted {
            continue;
        }
        match api.create(&new) {
            Ok(saved) => {
                ack(store, scope, local, saved.clone())?;
                remote.push(saved);
            }
            Err(e) => {
                // No automatic second POST after an ambiguous timeout. The durable journal
                // recovers a matching committed server row on the next successful GET.
                if e.downcast_ref::<reqwest::Error>()
                    .is_some_and(|e| e.is_connect())
                {
                    change(store, scope, |_, s| {
                        s.pending.remove(&local.id);
                    })?;
                }
                return Err(e);
            }
        }
    }
    Ok(report)
}
