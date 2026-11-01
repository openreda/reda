//! Objects and change sets: the before/after record of a transaction, keyed by UUID.
//!
//! The same change set feeds undo (apply it inverted), dirty tracking (its scopes), the edit
//! result envelope (its summary) and, later, incremental derived data and the Journal.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{DocumentEntry, ProjectProps, Uuid};

/// Any persisted object addressable by UUID. New object kinds add a variant here, a table
/// in [`crate::Project`], and arms in `Project::object` and `Project::put`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Object {
    Project(ProjectProps),
    Document(DocumentEntry),
    #[cfg(feature = "test-objects")]
    Probe(crate::Probe),
}

impl Object {
    pub fn uuid(&self) -> Uuid {
        match self {
            Object::Project(p) => p.uuid,
            Object::Document(d) => d.uuid,
            #[cfg(feature = "test-objects")]
            Object::Probe(p) => p.uuid,
        }
    }

    /// The undo stack and dirty flag this object belongs to.
    pub fn scope(&self) -> Scope {
        match self {
            Object::Project(_) | Object::Document(_) => Scope::Project,
            #[cfg(feature = "test-objects")]
            Object::Probe(p) => p.document.map_or(Scope::Project, Scope::Document),
        }
    }

    pub(crate) fn same_kind(&self, other: &Object) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// Where a change lands: the Project file (`.reda`) or one Document. Each has its own undo
/// stack and dirty state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    Project,
    Document(Uuid),
}

/// One object's state before and after a transaction. `None` means it did not exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub scope: Scope,
    pub before: Option<Object>,
    pub after: Option<Object>,
}

/// The UUIDs a change set created, modified and deleted in one scope, each sorted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ScopeSummary {
    pub created: Vec<Uuid>,
    pub modified: Vec<Uuid>,
    pub deleted: Vec<Uuid>,
}

/// The before/after record of one transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet {
    changes: BTreeMap<Uuid, Change>,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }

    pub fn get(&self, uuid: Uuid) -> Option<&Change> {
        self.changes.get(&uuid)
    }

    /// Every change, sorted by UUID.
    pub fn iter(&self) -> impl Iterator<Item = (&Uuid, &Change)> {
        self.changes.iter()
    }

    /// The scopes this change set touches.
    pub fn scopes(&self) -> std::collections::BTreeSet<Scope> {
        self.changes.values().map(|c| c.scope).collect()
    }

    /// Every change in the order that applies it safely: Project-scope upserts first, so a
    /// created Document's body exists before its contents land; then Document-scope
    /// changes; then Project-scope removals, so a Document's contents are gone before it
    /// is dropped.
    pub fn in_apply_order(&self) -> Vec<(&Uuid, &Change)> {
        let rank = |c: &Change| match (c.scope, &c.after) {
            (Scope::Project, Some(_)) => 0,
            (Scope::Document(_), _) => 1,
            (Scope::Project, None) => 2,
        };
        let mut ordered: Vec<_> = self.changes.iter().collect();
        ordered.sort_by_key(|(_, c)| rank(c));
        ordered
    }

    /// The change set split into one per scope, each holding only that scope's changes.
    pub fn split_by_scope(&self) -> BTreeMap<Scope, ChangeSet> {
        let mut out: BTreeMap<Scope, ChangeSet> = BTreeMap::new();
        for (uuid, change) in &self.changes {
            out.entry(change.scope)
                .or_default()
                .changes
                .insert(*uuid, change.clone());
        }
        out
    }

    /// The change set that reverts this one.
    pub fn inverted(&self) -> ChangeSet {
        let changes = self
            .changes
            .iter()
            .map(|(uuid, c)| {
                let inverse = Change {
                    scope: c.scope,
                    before: c.after.clone(),
                    after: c.before.clone(),
                };
                (*uuid, inverse)
            })
            .collect();
        ChangeSet { changes }
    }

    /// Created, modified and deleted UUIDs per scope.
    pub fn summary(&self) -> BTreeMap<Scope, ScopeSummary> {
        let mut out: BTreeMap<Scope, ScopeSummary> = BTreeMap::new();
        for (uuid, change) in &self.changes {
            let entry = out.entry(change.scope).or_default();
            match (&change.before, &change.after) {
                (None, Some(_)) => entry.created.push(*uuid),
                (Some(_), Some(_)) => entry.modified.push(*uuid),
                (Some(_), None) => entry.deleted.push(*uuid),
                (None, None) => {}
            }
        }
        out
    }

    /// Records that `uuid` went from `before` to `after`, merging with an earlier record of
    /// the same object: the first `before` and the last `after` win, and a change that ends
    /// where it started disappears.
    pub(crate) fn record(&mut self, uuid: Uuid, before: Option<Object>, after: Option<Object>) {
        let scope = after
            .as_ref()
            .or(before.as_ref())
            .map(Object::scope)
            .expect("a change has a before or an after");
        let before = match self.changes.remove(&uuid) {
            Some(earlier) => earlier.before,
            None => before,
        };
        if before != after {
            self.changes.insert(
                uuid,
                Change {
                    scope,
                    before,
                    after,
                },
            );
        }
    }
}
