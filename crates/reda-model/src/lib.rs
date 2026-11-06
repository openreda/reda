//! Every persisted design-data type. Data only.
//!
//! A [`Project`] owns per-Document tables keyed by [`Uuid`]. It changes only through a
//! [`Transaction`], which records a [`ChangeSet`] of before/after states per object. The
//! file format is not defined here: the engine maps these types to canonical JSON.

mod change;
mod check;
mod derived;
mod document;
mod library;
pub mod net_name;
mod output;
mod pad;
mod pcb;
#[cfg(feature = "test-objects")]
mod probe;
mod project;
mod schematic;
mod transaction;
mod uuid;

pub use change::{Change, ChangeSet, Object, ObjectType, Scope, ScopeSummary};
pub use check::{
    ErcCheck, ErcSeverities, Severity, Violation, ViolationArg, Waived, Waiver, apply_waivers,
    pin_conflict,
};
pub use document::{DocumentEntry, DocumentKind};
pub use library::{
    ComponentType, DESIGNATOR_FIELD, ElementSource, Field, Fields, Footprint, FootprintGenerator,
    FootprintGraphic, FootprintLayer, FootprintText, InsideEdge, LibraryElement, LibraryInfo,
    OutsideEdge, Part, Pin, PinMap, PinPads, PinType, Provenance, Shape, Symbol, SymbolGraphic,
    TextStyle, Unit, VALUE_FIELD, field_text,
};
pub use output::{
    Column, ColumnSource, FabPreset, FileNaming, GerberSettings, OutputJob, Outputs,
    ProjectFabPreset,
};
pub use pad::{
    ConnectionStyle, Corner, Corners, Hole, HoleTolerance, Pad, PadForm, PadKind, PadShape, Ratio,
    ZoneConnection,
};
pub use pcb::{
    BoardCutout, BoardOutline, BoardSetup, DrcCheck, DrcSeverities, FillShape, FootprintInstance,
    FreePad, GraphicShape, HERSHEY_FONT, InDocument, Keepout, Layer, LayerKind, LayerSet,
    LayerSpan, NOTO_SANS_FONT, Net, NetClass, PadConnection, PadNet, PcbDocument, PcbGraphic,
    PcbText, PlacementOrigin, Rules, Side, Track, TrackGeometry, Via, Zone, ZoneFills,
};
#[cfg(feature = "test-objects")]
pub use probe::Probe;
pub use project::{Project, ProjectProperties, ProjectProps, SchematicOptions};
pub use schematic::{
    ComponentRef, FootprintCopy, NetLabel, NoConnect, PartEntry, Port, PortDirection, PowerPort,
    PowerPortStyle, SchematicSheet, SheetFrame, SheetGraphic, SheetShape, SheetSize, SheetText,
    UnitInstance, Wire,
};
pub use transaction::{Transaction, TxError};
pub use uuid::{ParseUuidError, Uuid};

/// Geometry types, re-exported so model users need no direct `reda-geom` dependency.
pub use reda_geom as geom;
