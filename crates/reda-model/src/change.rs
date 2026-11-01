//! Objects and change sets: the before/after record of a transaction, keyed by UUID.
//!
//! The same change set feeds undo (apply it inverted), dirty tracking (its scopes), the edit
//! result envelope (its summary) and, later, incremental derived data and the Journal.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pcb::{BoardCutout, BoardOutline, BoardSetup, InDocument, LayerSet, Net, NetClass};
use crate::{DocumentEntry, Footprint, LibraryElement, Part, ProjectProps, Symbol, Uuid};

// New object kinds add a variant here, a table in `crate::Project`, and arms in
// `Project::object`, `Project::objects` and `Project::put`.
/// Any persisted object addressable by UUID, tagged with its `type`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Object {
    Project(ProjectProps),
    Document(DocumentEntry),
    /// An open Library element (see [`crate::Project::open_element`]).
    Symbol(Symbol),
    Footprint(Footprint),
    Part(Part),
    LayerSet(InDocument<LayerSet>),
    BoardSetup(InDocument<BoardSetup>),
    BoardOutline(InDocument<BoardOutline>),
    BoardCutout(InDocument<BoardCutout>),
    NetClass(InDocument<NetClass>),
    Net(InDocument<Net>),
    // Left out of schemas so the catalogue is the same with and without the feature.
    #[cfg(feature = "test-objects")]
    #[schemars(skip)]
    Probe(crate::Probe),
}

// A new object kind adds a variant here with the same name as its `Object` variant.
/// The type of an object, as its `type` tag names it.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    /// The Project's own properties, in the Project file.
    Project,
    /// A Document's entry in the Project's Document list.
    Document,
    /// An open Library Symbol.
    Symbol,
    /// An open Library Footprint.
    Footprint,
    /// An open Library Part.
    Part,
    /// The PCB Document's ordered Layers.
    LayerSet,
    /// The PCB Document's rules, DRC severities and Default Net Class.
    BoardSetup,
    /// The board's physical edge.
    BoardOutline,
    /// A hole in the board material.
    BoardCutout,
    /// A named group of Nets sharing design rules.
    NetClass,
    /// A PCB Net.
    Net,
    /// The test-only Probe.
    #[cfg(feature = "test-objects")]
    #[schemars(skip)]
    Probe,
}

impl Object {
    pub fn uuid(&self) -> Uuid {
        match self {
            Object::Project(p) => p.uuid,
            Object::Document(d) => d.uuid,
            Object::Symbol(s) => s.uuid,
            Object::Footprint(f) => f.uuid,
            Object::Part(p) => p.uuid,
            Object::LayerSet(o) => o.uuid,
            Object::BoardSetup(o) => o.uuid,
            Object::BoardOutline(o) => o.uuid,
            Object::BoardCutout(o) => o.uuid,
            Object::NetClass(o) => o.uuid,
            Object::Net(o) => o.uuid,
            #[cfg(feature = "test-objects")]
            Object::Probe(p) => p.uuid,
        }
    }

    pub fn object_type(&self) -> ObjectType {
        match self {
            Object::Project(_) => ObjectType::Project,
            Object::Document(_) => ObjectType::Document,
            Object::Symbol(_) => ObjectType::Symbol,
            Object::Footprint(_) => ObjectType::Footprint,
            Object::Part(_) => ObjectType::Part,
            Object::LayerSet(_) => ObjectType::LayerSet,
            Object::BoardSetup(_) => ObjectType::BoardSetup,
            Object::BoardOutline(_) => ObjectType::BoardOutline,
            Object::BoardCutout(_) => ObjectType::BoardCutout,
            Object::NetClass(_) => ObjectType::NetClass,
            Object::Net(_) => ObjectType::Net,
            #[cfg(feature = "test-objects")]
            Object::Probe(_) => ObjectType::Probe,
        }
    }

    /// The undo stack and dirty flag this object belongs to.
    pub fn scope(&self) -> Scope {
        match self {
            Object::Project(_) | Object::Document(_) => Scope::Project,
            Object::Symbol(_) | Object::Footprint(_) | Object::Part(_) => {
                Scope::Element(self.uuid())
            }
            Object::LayerSet(o) => Scope::Document(o.document),
            Object::BoardSetup(o) => Scope::Document(o.document),
            Object::BoardOutline(o) => Scope::Document(o.document),
            Object::BoardCutout(o) => Scope::Document(o.document),
            Object::NetClass(o) => Scope::Document(o.document),
            Object::Net(o) => Scope::Document(o.document),
            #[cfg(feature = "test-objects")]
            Object::Probe(p) => p.document.map_or(Scope::Project, Scope::Document),
        }
    }

    pub(crate) fn same_kind(&self, other: &Object) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The Library element this object is, if it is one.
    pub fn into_element(self) -> Option<LibraryElement> {
        match self {
            Object::Symbol(s) => Some(LibraryElement::Symbol(s)),
            Object::Footprint(f) => Some(LibraryElement::Footprint(f)),
            Object::Part(p) => Some(LibraryElement::Part(p)),
            _ => None,
        }
    }
}

impl From<LibraryElement> for Object {
    fn from(element: LibraryElement) -> Object {
        match element {
            LibraryElement::Symbol(s) => Object::Symbol(s),
            LibraryElement::Footprint(f) => Object::Footprint(f),
            LibraryElement::Part(p) => Object::Part(p),
        }
    }
}

/// Where a change lands: the Project file (`.reda`), one Document, or one open Library
/// element. Each has its own undo stack and dirty state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    Project,
    Document(Uuid),
    /// An open Library element, saved to its own file in its Library.
    Element(Uuid),
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
            (Scope::Document(_) | Scope::Element(_), _) => 1,
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

    /// The change set split into one per undo stack: as [`ChangeSet::split_by_scope`],
    /// except that the changes inside a Document this change set creates or deletes go
    /// with the Project's part. Adding or deleting a Document, contents and all, is then
    /// one step on the Project's stack, which undoes and redoes whole.
    pub fn split_by_stack(&self) -> BTreeMap<Scope, ChangeSet> {
        let whole: std::collections::BTreeSet<Uuid> = self
            .created_documents()
            .into_iter()
            .chain(self.deleted_documents())
            .collect();
        let mut out: BTreeMap<Scope, ChangeSet> = BTreeMap::new();
        for (uuid, change) in &self.changes {
            let stack = match change.scope {
                Scope::Document(doc) if whole.contains(&doc) => Scope::Project,
                scope => scope,
            };
            out.entry(stack)
                .or_default()
                .changes
                .insert(*uuid, change.clone());
        }
        out
    }

    /// The Documents this change set creates.
    pub fn created_documents(&self) -> Vec<Uuid> {
        self.document_entries(|c| c.before.is_none())
    }

    /// The Documents this change set deletes.
    pub fn deleted_documents(&self) -> Vec<Uuid> {
        self.document_entries(|c| c.after.is_none())
    }

    fn document_entries(&self, pick: impl Fn(&Change) -> bool) -> Vec<Uuid> {
        self.changes
            .iter()
            .filter(|(_, c)| {
                let entry = c.before.as_ref().or(c.after.as_ref());
                matches!(entry, Some(Object::Document(_))) && pick(c)
            })
            .map(|(uuid, _)| *uuid)
            .collect()
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
