//! Every persisted design-data type. Data only.
//!
//! A [`Project`] owns per-Document tables keyed by [`Uuid`]. It changes only through a
//! [`Transaction`], which records a [`ChangeSet`] of before/after states per object. The
//! file format is not defined here: the engine maps these types to canonical JSON.

mod change;
mod check;
mod document;
mod library;
mod output;
mod pcb;
#[cfg(feature = "test-objects")]
mod probe;
mod project;
mod schematic;
mod transaction;
mod uuid;

pub use change::{Change, ChangeSet, Object, ObjectType, Scope, ScopeSummary};
pub use check::{Severity, Violation, ViolationArg};
pub use document::{DocumentEntry, DocumentKind};
pub use library::{
    ComponentType, ElementSource, Field, Fields, Footprint, FootprintGraphic, FootprintLayer,
    FootprintText, Hole, LibraryElement, LibraryInfo, Pad, PadKind, PadShape, Part, Pin, PinMap,
    PinPads, PinType, Provenance, Shape, Symbol, SymbolGraphic, Unit,
};
pub use output::{
    Column, ColumnSource, FabPreset, FileNaming, GerberSettings, OutputJob, Outputs,
    ProjectFabPreset,
};
pub use pcb::{
    BoardCutout, BoardOutline, BoardSetup, DrcCheck, DrcSeverities, GraphicShape, HERSHEY_FONT,
    InDocument, Layer, LayerKind, LayerSet, NOTO_SANS_FONT, Net, NetClass, PcbDocument, PcbGraphic,
    PcbText, Rules, Side,
};
#[cfg(feature = "test-objects")]
pub use probe::Probe;
pub use project::{Project, ProjectProperties, ProjectProps};
pub use schematic::{
    ComponentRef, FootprintCopy, NetLabel, NoConnect, PartEntry, Port, PortDirection, PowerPort,
    PowerPortStyle, SchematicSheet, SheetFrame, SheetGraphic, SheetShape, SheetSize, SheetText,
    UnitInstance, Wire,
};
pub use transaction::{Transaction, TxError};
pub use uuid::{ParseUuidError, Uuid};

/// Geometry types, re-exported so model users need no direct `reda-geom` dependency.
pub use reda_geom as geom;
