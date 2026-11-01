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
mod transaction;
mod uuid;

pub use change::{Change, ChangeSet, Object, ObjectType, Scope, ScopeSummary};
pub use document::{DocumentEntry, DocumentKind, SchematicSheet};
pub use library::{
    ComponentType, Field, Fields, Footprint, Hole, LibraryInfo, Pad, PadKind, PadShape, Part, Pin,
    PinMap, PinPads, PinType, Symbol, Unit,
};
pub use pcb::{
    BoardCutout, BoardOutline, BoardSetup, DrcCheck, DrcSeverities, GraphicShape, HERSHEY_FONT,
    InDocument, Layer, LayerKind, LayerSet, NOTO_SANS_FONT, Net, NetClass, PcbDocument, PcbGraphic,
    PcbText, Rules, Severity, Side,
};
#[cfg(feature = "test-objects")]
pub use probe::Probe;
pub use project::{Project, ProjectProps};
pub use transaction::{Transaction, TxError};
pub use uuid::{ParseUuidError, Uuid};

/// Geometry types, re-exported so model users need no direct `reda-geom` dependency.
pub use reda_geom as geom;
