//! The Project value: its properties, its ordered Document list and the Documents it owns.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;
use crate::change::{ChangeSet, Object};
use crate::document::{DocumentEntry, DocumentKind, SchematicSheet};

/// Project-wide properties, stored in `<name>.reda`.
///
/// The Document order lives here because it is a Project property: it drives sheet
/// numbering and PDF output, and reordering is an edit on the Project's own undo stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectProps {
    pub uuid: Uuid,
    pub name: String,
    /// Every Document's UUID, in Project order.
    pub documents: Vec<Uuid>,
}

/// One electronic design and everything that belongs to it.
///
/// Data changes only through [`Project::transaction`] or by applying a [`ChangeSet`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    props: ProjectProps,
    entries: BTreeMap<Uuid, DocumentEntry>,
    schematic_sheets: BTreeMap<Uuid, SchematicSheet>,
    #[cfg(feature = "test-objects")]
    probes: BTreeMap<Uuid, crate::Probe>,
}

impl Project {
    /// A Project with no Documents. `props.documents` must be empty.
    pub fn new(props: ProjectProps) -> Project {
        assert!(
            props.documents.is_empty(),
            "Documents are added through a transaction"
        );
        Project {
            props,
            entries: BTreeMap::new(),
            schematic_sheets: BTreeMap::new(),
            #[cfg(feature = "test-objects")]
            probes: BTreeMap::new(),
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.props.uuid
    }

    pub fn props(&self) -> &ProjectProps {
        &self.props
    }

    /// The Documents in Project order.
    pub fn documents(&self) -> impl Iterator<Item = &DocumentEntry> {
        self.props.documents.iter().map(|uuid| &self.entries[uuid])
    }

    pub fn document(&self, uuid: Uuid) -> Option<&DocumentEntry> {
        self.entries.get(&uuid)
    }

    pub fn schematic_sheet(&self, uuid: Uuid) -> Option<&SchematicSheet> {
        self.schematic_sheets.get(&uuid)
    }

    /// The object with this UUID, cloned, wherever it lives.
    pub fn object(&self, uuid: Uuid) -> Option<Object> {
        if uuid == self.props.uuid {
            return Some(Object::Project(self.props.clone()));
        }
        #[cfg(feature = "test-objects")]
        if let Some(probe) = self.probes.get(&uuid) {
            return Some(Object::Probe(probe.clone()));
        }
        self.entries.get(&uuid).cloned().map(Object::Document)
    }

    /// Every object, cloned, sorted by UUID. A new object kind adds its table here.
    pub fn objects(&self) -> Vec<Object> {
        let mut out: Vec<Object> = std::iter::once(Object::Project(self.props.clone()))
            .chain(self.entries.values().cloned().map(Object::Document))
            .collect();
        #[cfg(feature = "test-objects")]
        out.extend(self.probes.values().cloned().map(Object::Probe));
        out.sort_by_key(Object::uuid);
        out
    }

    /// The test-only Probe with this UUID.
    #[cfg(feature = "test-objects")]
    pub fn probe(&self, uuid: Uuid) -> Option<&crate::Probe> {
        self.probes.get(&uuid)
    }

    /// Applies every change's `after` state. Apply [`ChangeSet::inverted`] to undo.
    pub fn apply(&mut self, changes: &ChangeSet) {
        for (uuid, change) in changes.in_apply_order() {
            self.put(*uuid, change.after.clone());
        }
    }

    /// Sets the object stored under `uuid`, or removes it with `None`. No bookkeeping.
    pub(crate) fn put(&mut self, uuid: Uuid, object: Option<Object>) {
        match object {
            Some(Object::Project(props)) => self.props = props,
            Some(Object::Document(entry)) => {
                if !self.entries.contains_key(&uuid) {
                    self.create_body(&entry);
                }
                self.entries.insert(uuid, entry);
            }
            #[cfg(feature = "test-objects")]
            Some(Object::Probe(probe)) => {
                self.probes.insert(uuid, probe);
            }
            None => {
                assert_ne!(uuid, self.props.uuid, "the Project itself is never removed");
                if self.entries.remove(&uuid).is_some() {
                    self.schematic_sheets.remove(&uuid);
                }
                #[cfg(feature = "test-objects")]
                self.probes.remove(&uuid);
            }
        }
    }

    fn create_body(&mut self, entry: &DocumentEntry) {
        match entry.kind {
            DocumentKind::Schematic => {
                self.schematic_sheets
                    .insert(entry.uuid, SchematicSheet { uuid: entry.uuid });
            }
        }
    }

    /// Checks the invariants a committed transaction must leave behind.
    pub(crate) fn check(&self) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for uuid in &self.props.documents {
            if !self.entries.contains_key(uuid) {
                return Err(format!("the Document order names unknown Document {uuid}"));
            }
            if !seen.insert(*uuid) {
                return Err(format!("Document {uuid} appears twice in the order"));
            }
        }
        if let Some(orphan) = self.entries.keys().find(|uuid| !seen.contains(uuid)) {
            return Err(format!(
                "Document {orphan} is missing from the Document order"
            ));
        }
        Ok(())
    }
}
