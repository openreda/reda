//! Schematic Sheet behaviour: placing Components, the sheet frame and its title block, the
//! schematic Commands, and the connectivity derived from the drawing ([`connectivity`]).
//! ERC ([`erc`]) checks it.

mod component;
pub mod connectivity;
pub mod cross_ref;
mod edit;
pub mod erc;
mod fields;
mod find;
mod library_element;
mod natural;
mod normalise;
mod options;
mod pins;
mod ports;
mod properties;
mod route;
mod sheet;
mod similar;
mod unit_cycle;
mod unit_move;
mod update;
mod wiring;

use reda_command::{CommandError, Registry};
use reda_model::{Project, SchematicSheet, Uuid};

pub use component::{
    At, ComponentPlace, ComponentPlaceParams, PlacedResult, UnitPlace, UnitPlaceParams,
    next_free_designator,
};
pub use edit::Gap as BreakGap;
pub use edit::{
    Align, AlignParams, AlignTo, Axis, Clipboard, ConvertCross, ConvertCrossParams,
    ConvertCrossResult, ConvertedJunction, Copy, CopyParams, Cut, CutParams, Delete, DeleteParams,
    DeleteResult, Direction, Distribute, DistributeParams, Drag, Duplicate, DuplicateParams,
    EditResult, FlightStep, GraphicResult, GraphicSet, GraphicSetParams, LineAdd, LineAddParams,
    Mirror, MirrorParams, Move, MoveToSheet, MoveToSheetParams, MoveToSheetResult, NetChange,
    Paste, PasteParams, PasteResult, RectAdd, RectAddParams, Rotate, RotateParams, ShiftParams,
    ShiftPreview, SnapParams, SnapToGrid, TextAdd, TextAddParams, TextResult, TextSet,
    TextSetParams, WireBreak, WireBreakParams, WireBreakResult, WireSegment, WireSetVertices,
    WireSetVerticesParams, WireSetVerticesResult, break_gap, flight_preview, selectable,
    shift_preview,
};
pub use fields::{FieldMove, FieldMoveParams, FieldResetParams, FieldResetPosition, FieldResult};
pub use find::{Find, FindField, FindMatch, FindParams, FindResult, Matcher, find};
pub use library_element::{
    ElementFile, LibraryElement, LibraryElementParams, LibraryElementResult,
};
pub use natural::natural_cmp;
pub use normalise::{DEFAULT_SNAP_GRID, WIRING_COMMANDS, Wiring};
pub use options::{OptionsSet, OptionsSetParams};
pub use pins::{ComponentPins, ComponentPinsParams, ComponentPinsQuery, PinPads, component_pins};
pub use ports::{
    POWER_PORT_PRESETS, PortAdd, PortAddParams, PortAddResult, PortSet, PortSetParams,
    PowerPortAdd, PowerPortAddParams, PowerPortAddResult, PowerPortSet, PowerPortSetParams,
    ProjectNets, ProjectNetsResult, SetResult, default_power_port_style,
};
pub use properties::{ComponentSet, ComponentSetParams, ComponentSetResult};
pub use route::{
    Avoid, Obstacles, Options, Route, RouteError, WireRoute, WireRouteParams, WireRouteResult,
    route,
};
pub use sheet::{
    FrameSizeParam, SheetFrameView, SheetRef, SheetSetFrame, SheetSetFrameParams, SheetTitleBlock,
    TitleBlock, ZoneCounts, title_block,
};
pub use similar::{
    SelectSimilar, SelectSimilarParams, SelectSimilarResult, SimilarField, SimilarScope,
    select_similar,
};
pub use unit_cycle::{UnitCycle, UnitCycleParams, UnitCycleResult};
pub use unit_move::{UnitMove, UnitMoveParams, UnitMoveResult};
pub use update::{
    Aspect, Cell, CellAction, CellCase, CellRef, ComponentAspects, ComponentStatus,
    ComponentUpdate, FootprintChange, FromTo, LibraryStatus, LibraryStatusParams,
    LibraryStatusQuery, LibraryStatusResult, LostLocalEdits, PartChange, PartsOf, Problem,
    SettingsParams, UpdateApplied, UpdateFromLibrary, UpdateFromLibraryParams, UpdateParams,
    UpdatePreview, UpdateResult, footprint_change,
};
pub use update::{
    KeepFields, ReplaceParams, ReplacePart, ReplacePartParams, ReplacePreview,
    ReplacePreviewResult, ReplaceRow, Replaced,
};
pub use wiring::{
    NetLabelAdd, NetLabelAddParams, NetLabelAddResult, NetsParams, NoConnectAdd,
    NoConnectAddParams, NoConnectAddResult, SchematicNets, WireAdd, WireAddParams, WireAddResult,
};

/// Adds every Command defined in `reda-schematic`.
pub fn register_all(registry: &mut Registry) {
    register_all_with_user(
        registry,
        reda_command::os_user_name,
        update::no_lost_local_edits,
    );
}

/// Adds the Commands with a source for the Preference-backed Waiver author, and the PCB
/// reader that tells which local edits an update removes (it lives in `reda-engine`).
pub fn register_all_with_user(
    registry: &mut Registry,
    user_name: reda_command::UserName,
    lost_local_edits: LostLocalEdits,
) {
    registry.add(ComponentPlace);
    registry.add(UnitPlace);
    registry.add(ComponentSet);
    registry.add(ComponentPinsQuery);
    registry.add(Find);
    registry.add(SelectSimilar);
    registry.add(LibraryStatusQuery);
    registry.add(UpdatePreview { lost_local_edits });
    registry.add(UpdateFromLibrary { lost_local_edits });
    registry.add(ReplacePreview { lost_local_edits });
    registry.add(ReplacePart { lost_local_edits });
    registry.add(LibraryElement);
    registry.add(SheetSetFrame);
    registry.add(SheetTitleBlock);
    registry.add(WireAdd);
    registry.add(WireRoute);
    registry.add(WireBreak);
    registry.add(WireSetVertices);
    registry.add(ConvertCross);
    registry.add(NetLabelAdd);
    registry.add(UnitMove);
    registry.add(UnitCycle);
    registry.add(FieldMove);
    registry.add(FieldResetPosition);
    registry.add(NoConnectAdd);
    registry.add(SchematicNets);
    registry.add(PortAdd);
    registry.add(PortSet);
    registry.add(PowerPortAdd);
    registry.add(PowerPortSet);
    registry.add(ProjectNets);
    registry.add(OptionsSet);
    registry.add(Delete);
    registry.add(Copy);
    registry.add(Cut);
    registry.add(Paste);
    registry.add(Duplicate);
    registry.add(Move);
    registry.add(MoveToSheet);
    registry.add(Drag);
    registry.add(Rotate);
    registry.add(Mirror);
    registry.add(Align);
    registry.add(Distribute);
    registry.add(SnapToGrid);
    registry.add(TextAdd);
    registry.add(TextSet);
    registry.add(LineAdd);
    registry.add(RectAdd);
    registry.add(GraphicSet);
    erc::register_all(registry, user_name);
}

/// The Schematic Sheet with this UUID, or `not_found`.
pub(crate) fn sheet(project: &Project, uuid: Uuid) -> Result<&SchematicSheet, CommandError> {
    project
        .schematic_sheet(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Schematic Sheet has UUID {uuid}")))
}
