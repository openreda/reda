//! Every persisted design-data type. Data only.
//!
//! A [`Project`] owns per-Document tables keyed by [`Uuid`]. It changes only through a
//! [`Transaction`], which records a [`ChangeSet`] of before/after states per object. The
//! file format is not defined here: the engine maps these types to canonical JSON.

pub mod annotate;
pub mod auto_increment;
mod change;
mod check;
mod derived;
mod document;
pub mod footprint_copper;
mod library;
pub mod library_update;
pub mod local_edit;
pub mod net_name;
mod openings;
mod output;
pub use openings::{Mask, MaskSide, Paste, PasteMode, PasteWindow, Tenting, opening_problem};
mod pad;
mod pcb;
#[cfg(feature = "test-objects")]
mod probe;
mod project;
mod schematic;
pub mod text;
mod transaction;
mod uuid;

pub use change::{Change, ChangeSet, Object, ObjectType, Scope, ScopeSummary};
pub use check::{
    Cell, ErcCheck, ErcSettings, ErcSeverities, LENGTH_ARGS, LibraryCheck, LibrarySeverities,
    Limit, MatrixType, Measurement, PinMatrix, Severity, Violation, ViolationArg, Waived, Waiver,
    apply_waivers,
};
pub use document::{DocumentEntry, DocumentKind, ListedDocument};
pub use library::{
    ComponentType, DESIGNATOR_FIELD, ElementSource, Field, Fields, Footprint, FootprintGenerator,
    FootprintGraphic, FootprintLayer, FootprintText, InsideEdge, LibraryElement, LibraryInfo,
    OutsideEdge, Part, Pin, PinMap, PinPads, PinType, Provenance, Shape, Symbol, SymbolGraphic,
    TextFrame, TextHJustify, TextJustify, TextKnockout, TextStyle, TextVJustify, Unit, VALUE_FIELD,
    field_text,
};
pub use output::{
    BomGroup, BomLayout, BomSettings, BottomPosition, BottomRotation, Column, ColumnSource,
    CplSettings, CplUnits, FabPreset, FabRules, FileNaming, FileOrigin, GerberSettings,
    LayerFileName, LineNumbers, MountValues, NAME_TEMPLATE_TOKENS, NameTemplate, NameTemplateError,
    NotFittedBom, NotFittedPaste, OutputJob, Outputs, PdfColour, PreferredSeeds, ProjectFabPreset,
    SideValues, SlotCommand, StackupSuggestions,
};
pub use pad::{
    ConnectionStyle, Corner, Corners, Hole, HoleTolerance, MaskExpansion, Pad, PadForm, PadKind,
    PadShape, PasteExpansion, Ratio, ZoneConnection,
};
pub use pcb::{
    ArrowPosition, BoardCutout, BoardOutline, BoardSetup, Bounds, ConnectionTarget,
    ConnectionValue, CopperEntry, DEFAULT_BOARD_THICKNESS, DEFAULT_COPPER, DesignRule,
    DielectricEntry, Dimension, DimensionEnd, DimensionFormat, DimensionTextPosition,
    DimensionUnits, DrcCheck, DrcSeverities, FillShape, FootprintInstance, FootprintTextRef,
    FreePad, GraphicShape, HERSHEY_FONT, Hotspot, HotspotRef, InDocument, Keepout, KeepoutRecord,
    KeepoutShape, KeepoutShapeKind, Layer, LayerKind, LayerSet, LayerSpan, NOTO_SANS_FONT, Net,
    NetClass, OutputOrigin, PadNet, PadRef, PcbDocument, PcbGraphic, PcbText, PortableClass,
    PortableRule, PourOver, Restriction, RuleField, RuleKind, RuleScope, RuleSet, RuleValue,
    RuleValues, Side, SilkMaskMode, Stackup, StackupEntry, StackupSeeds, StackupTotals,
    TextAutoposition, TextOverride, Track, TrackGeometry, Via, Zone, ZoneCutout, ZoneFill,
    ZoneFills, ZoneNameScheme,
};
#[cfg(feature = "test-objects")]
pub use probe::Probe;
pub use project::{
    CrossReferenceStyles, LocationStyle, NotFittedDisplay, Project, ProjectProperties,
    ProjectProps, SchematicOptions, SheetStyle,
};
pub use schematic::{
    BorderZones, ComponentRef, EditedFrom, FieldText, FootprintCopy, FrameSize, Justify,
    JustifyAxis, NetLabel, NoConnect, Orientation, PartEntry, Port, PortDirection, PowerPort,
    PowerPortStyle, SchematicSheet, SheetFrame, SheetGraphic, SheetShape, SheetSize, SheetText,
    UnitInstance, UnitSuffix, Wire,
};
pub use transaction::{Transaction, TxError};
pub use uuid::{ParseUuidError, Uuid};

/// Geometry types, re-exported so model users need no direct `reda-geom` dependency.
pub use reda_geom as geom;
