//! Objects and change sets: the before/after record of a transaction, keyed by UUID.
//!
//! The same change set feeds undo (apply it inverted), dirty tracking (its scopes), the edit
//! result envelope (its summary) and, later, incremental derived data and the Journal.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::output::{OutputJob, ProjectFabPreset};
use crate::pcb::{
    BoardCutout, BoardOutline, BoardSetup, DesignRule, FootprintInstance, FreePad, InDocument,
    Keepout, LayerSet, Net, NetClass, PcbGraphic, PcbText, PlacementOrigin, Track, Via, Zone,
};
use crate::{
    DocumentEntry, Footprint, FootprintCopy, LibraryElement, NetLabel, NoConnect, Part, PartEntry,
    Port, PowerPort, ProjectProps, SheetFrame, SheetGraphic, SheetText, Symbol, UnitInstance, Uuid,
    Waiver, Wire,
};

// New object kinds add a variant here, a table in `crate::Project`, and arms in
// `Project::object`, `Project::objects` and `Project::put`.
/// Any persisted object addressable by UUID, tagged with its `type`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Object {
    Project(ProjectProps),
    Document(DocumentEntry),
    FabPreset(ProjectFabPreset),
    OutputJob(OutputJob),
    /// An open Library element (see [`crate::Project::open_element`]).
    Symbol(Symbol),
    Footprint(Footprint),
    Part(Part),
    LayerSet(InDocument<LayerSet>),
    BoardSetup(InDocument<BoardSetup>),
    DesignRule(InDocument<DesignRule>),
    BoardOutline(InDocument<BoardOutline>),
    BoardCutout(InDocument<BoardCutout>),
    NetClass(InDocument<NetClass>),
    Net(InDocument<Net>),
    SheetFrame(InDocument<SheetFrame>),
    PartEntry(InDocument<PartEntry>),
    FootprintCopy(InDocument<FootprintCopy>),
    Unit(InDocument<UnitInstance>),
    Wire(InDocument<Wire>),
    NetLabel(InDocument<NetLabel>),
    NoConnect(InDocument<NoConnect>),
    Port(InDocument<Port>),
    PowerPort(InDocument<PowerPort>),
    SheetText(InDocument<SheetText>),
    SheetGraphic(InDocument<SheetGraphic>),
    Waiver(InDocument<Waiver>),
    PcbText(InDocument<PcbText>),
    PcbGraphic(InDocument<PcbGraphic>),
    FootprintInstance(InDocument<FootprintInstance>),
    Track(InDocument<Track>),
    Via(InDocument<Via>),
    Zone(InDocument<Zone>),
    Keepout(InDocument<Keepout>),
    FreePad(InDocument<FreePad>),
    PlacementOrigin(InDocument<PlacementOrigin>),
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
    /// The Project's copy of a Fab Preset.
    FabPreset,
    /// A named recipe for manufacturing files.
    OutputJob,
    /// An open Library Symbol.
    Symbol,
    /// An open Library Footprint.
    Footprint,
    /// An open Library Part.
    Part,
    /// The PCB Document's ordered Layers.
    LayerSet,
    /// The PCB Document's DRC severities.
    BoardSetup,
    /// One design rule record: a kind, a scope and the values it sets.
    DesignRule,
    /// The board's physical edge.
    BoardOutline,
    /// A hole in the board material.
    BoardCutout,
    /// A named group of Nets sharing design rules.
    NetClass,
    /// A PCB Net.
    Net,
    /// A Schematic Sheet's frame settings (its size).
    SheetFrame,
    /// An entry of a sheet's part table: a copied Part with its Symbol.
    PartEntry,
    /// A copied Footprint in a sheet's or the PCB Document's part table.
    FootprintCopy,
    /// A placed Unit of a Component.
    Unit,
    /// One orthogonal segment of a Wire run on a Schematic Sheet.
    Wire,
    /// A Net Label on a Schematic Sheet.
    NetLabel,
    /// A No-Connect Marker on a Pin.
    NoConnect,
    /// A Port on a Schematic Sheet, joining Nets across sheets.
    Port,
    /// A Power Port on a Schematic Sheet, joining Nets across the Project.
    PowerPort,
    /// Free text on a Schematic Sheet.
    SheetText,
    /// A free line or rectangle on a Schematic Sheet.
    SheetGraphic,
    /// A Waiver of one Violation, in the Document holding its objects.
    Waiver,
    /// Free text on a PCB Layer.
    PcbText,
    /// A line, arc or rectangle on a PCB Layer.
    PcbGraphic,
    /// A Footprint placed on the board, with its Component's data and a Net per Pad.
    FootprintInstance,
    /// One copper segment on a copper Layer.
    Track,
    /// A plated hole joining copper on several Layers.
    Via,
    /// A copper fill on a Net, on one copper Layer.
    Zone,
    /// An area where copper or Vias may not go.
    Keepout,
    /// A Pad placed directly on the board, belonging to no Footprint.
    FreePad,
    /// The point manufacturing outputs measure Footprint positions from.
    PlacementOrigin,
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
            Object::FabPreset(o) => o.uuid,
            Object::OutputJob(o) => o.uuid,
            Object::Symbol(s) => s.uuid,
            Object::Footprint(f) => f.uuid,
            Object::Part(p) => p.uuid,
            Object::LayerSet(o) => o.uuid,
            Object::BoardSetup(o) => o.uuid,
            Object::DesignRule(o) => o.uuid,
            Object::BoardOutline(o) => o.uuid,
            Object::BoardCutout(o) => o.uuid,
            Object::NetClass(o) => o.uuid,
            Object::Net(o) => o.uuid,
            Object::SheetFrame(o) => o.uuid,
            Object::PartEntry(o) => o.uuid,
            Object::FootprintCopy(o) => o.uuid,
            Object::Unit(o) => o.uuid,
            Object::Wire(o) => o.uuid,
            Object::NetLabel(o) => o.uuid,
            Object::NoConnect(o) => o.uuid,
            Object::Port(o) => o.uuid,
            Object::PowerPort(o) => o.uuid,
            Object::SheetText(o) => o.uuid,
            Object::SheetGraphic(o) => o.uuid,
            Object::Waiver(o) => o.uuid,
            Object::PcbText(o) => o.uuid,
            Object::PcbGraphic(o) => o.uuid,
            Object::FootprintInstance(o) => o.uuid,
            Object::Track(o) => o.uuid,
            Object::Via(o) => o.uuid,
            Object::Zone(o) => o.uuid,
            Object::Keepout(o) => o.uuid,
            Object::FreePad(o) => o.uuid,
            Object::PlacementOrigin(o) => o.uuid,
            #[cfg(feature = "test-objects")]
            Object::Probe(p) => p.uuid,
        }
    }

    pub fn object_type(&self) -> ObjectType {
        match self {
            Object::Project(_) => ObjectType::Project,
            Object::Document(_) => ObjectType::Document,
            Object::FabPreset(_) => ObjectType::FabPreset,
            Object::OutputJob(_) => ObjectType::OutputJob,
            Object::Symbol(_) => ObjectType::Symbol,
            Object::Footprint(_) => ObjectType::Footprint,
            Object::Part(_) => ObjectType::Part,
            Object::LayerSet(_) => ObjectType::LayerSet,
            Object::BoardSetup(_) => ObjectType::BoardSetup,
            Object::DesignRule(_) => ObjectType::DesignRule,
            Object::BoardOutline(_) => ObjectType::BoardOutline,
            Object::BoardCutout(_) => ObjectType::BoardCutout,
            Object::NetClass(_) => ObjectType::NetClass,
            Object::Net(_) => ObjectType::Net,
            Object::SheetFrame(_) => ObjectType::SheetFrame,
            Object::PartEntry(_) => ObjectType::PartEntry,
            Object::FootprintCopy(_) => ObjectType::FootprintCopy,
            Object::Unit(_) => ObjectType::Unit,
            Object::Wire(_) => ObjectType::Wire,
            Object::NetLabel(_) => ObjectType::NetLabel,
            Object::NoConnect(_) => ObjectType::NoConnect,
            Object::Port(_) => ObjectType::Port,
            Object::PowerPort(_) => ObjectType::PowerPort,
            Object::SheetText(_) => ObjectType::SheetText,
            Object::SheetGraphic(_) => ObjectType::SheetGraphic,
            Object::Waiver(_) => ObjectType::Waiver,
            Object::PcbText(_) => ObjectType::PcbText,
            Object::PcbGraphic(_) => ObjectType::PcbGraphic,
            Object::FootprintInstance(_) => ObjectType::FootprintInstance,
            Object::Track(_) => ObjectType::Track,
            Object::Via(_) => ObjectType::Via,
            Object::Zone(_) => ObjectType::Zone,
            Object::Keepout(_) => ObjectType::Keepout,
            Object::FreePad(_) => ObjectType::FreePad,
            Object::PlacementOrigin(_) => ObjectType::PlacementOrigin,
            #[cfg(feature = "test-objects")]
            Object::Probe(_) => ObjectType::Probe,
        }
    }

    /// The undo stack and dirty flag this object belongs to.
    pub fn scope(&self) -> Scope {
        match self {
            Object::Project(_)
            | Object::Document(_)
            | Object::FabPreset(_)
            | Object::OutputJob(_) => Scope::Project,
            Object::Symbol(_) | Object::Footprint(_) | Object::Part(_) => {
                Scope::Element(self.uuid())
            }
            Object::LayerSet(o) => Scope::Document(o.document),
            Object::BoardSetup(o) => Scope::Document(o.document),
            Object::DesignRule(o) => Scope::Document(o.document),
            Object::BoardOutline(o) => Scope::Document(o.document),
            Object::BoardCutout(o) => Scope::Document(o.document),
            Object::NetClass(o) => Scope::Document(o.document),
            Object::Net(o) => Scope::Document(o.document),
            Object::SheetFrame(o) => Scope::Document(o.document),
            Object::PartEntry(o) => Scope::Document(o.document),
            Object::FootprintCopy(o) => Scope::Document(o.document),
            Object::Unit(o) => Scope::Document(o.document),
            Object::Wire(o) => Scope::Document(o.document),
            Object::NetLabel(o) => Scope::Document(o.document),
            Object::NoConnect(o) => Scope::Document(o.document),
            Object::Port(o) => Scope::Document(o.document),
            Object::PowerPort(o) => Scope::Document(o.document),
            Object::SheetText(o) => Scope::Document(o.document),
            Object::SheetGraphic(o) => Scope::Document(o.document),
            Object::Waiver(o) => Scope::Document(o.document),
            Object::PcbText(o) => Scope::Document(o.document),
            Object::PcbGraphic(o) => Scope::Document(o.document),
            Object::FootprintInstance(o) => Scope::Document(o.document),
            Object::Track(o) => Scope::Document(o.document),
            Object::Via(o) => Scope::Document(o.document),
            Object::Zone(o) => Scope::Document(o.document),
            Object::Keepout(o) => Scope::Document(o.document),
            Object::FreePad(o) => Scope::Document(o.document),
            Object::PlacementOrigin(o) => Scope::Document(o.document),
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
