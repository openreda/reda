//! Every persisted design-data type. Data only.
//!
//! A [`Project`] owns per-Document tables keyed by [`Uuid`]. It changes only through a
//! [`Transaction`], which records a [`ChangeSet`] of before/after states per object. The
//! file format is not defined here: the engine maps these types to canonical JSON.

mod change;
mod document;
mod library;
mod pcb;
#[cfg(feature = "test-objects")]
mod probe;
mod project;
mod schematic;
mod transaction;
mod uuid;

pub use change::{Change, ChangeSet, Object, ObjectType, Scope, ScopeSummary};
pub use document::{DocumentEntry, DocumentKind};
pub use library::{
    ComponentType, ElementSource, Field, Fields, Footprint, FootprintGraphic, FootprintLayer,
    FootprintText, Hole, LibraryElement, LibraryInfo, Pad, PadKind, PadShape, Part, Pin, PinMap,
    PinPads, PinType, Shape, Symbol, SymbolGraphic, Unit,
};
pub use pcb::{
    BoardCutout, BoardOutline, BoardSetup, DrcCheck, DrcSeverities, InDocument, Layer, LayerKind,
    LayerSet, Net, NetClass, PcbDocument, Rules, Severity, Side,
};
#[cfg(feature = "test-objects")]
pub use probe::Probe;
pub use project::{Project, ProjectProperties, ProjectProps};
pub use schematic::{
    ComponentRef, FootprintCopy, PartEntry, SchematicSheet, SheetFrame, SheetSize, UnitInstance,
};
pub use transaction::{Transaction, TxError};
pub use uuid::{ParseUuidError, Uuid};

/// Geometry types, re-exported so model users need no direct `reda-geom` dependency.
pub use reda_geom as geom;
