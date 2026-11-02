//! Schematic Sheet behaviour: placing Components, the sheet frame and its title block, the
//! schematic Commands, and the connectivity derived from the drawing ([`connectivity`]).
//! ERC joins later.

mod component;
pub mod connectivity;
mod edit;
mod find;
mod library_element;
mod natural;
mod pins;
mod properties;
mod sheet;
mod similar;
mod unit_move;
mod update;
mod wiring;

use reda_command::{CommandError, Registry};
use reda_model::{Project, SchematicSheet, Uuid};

pub use component::{
    At, ComponentPlace, ComponentPlaceParams, PlacedResult, UnitPlace, UnitPlaceParams,
    next_free_designator,
};
pub use edit::{
    Align, AlignParams, AlignTo, Axis, Clipboard, Copy, CopyParams, Cut, Delete, DeleteParams,
    DeleteResult, Direction, Distribute, DistributeParams, Drag, Duplicate, DuplicateParams,
    EditResult, GraphicResult, GraphicSet, GraphicSetParams, LineAdd, LineAddParams, Mirror,
    MirrorParams, Move, Paste, PasteParams, PasteResult, RectAdd, RectAddParams, Rotate,
    RotateParams, ShiftParams, SnapParams, SnapToGrid, TextAdd, TextAddParams, TextResult, TextSet,
    TextSetParams,
};
pub use find::{Find, FindField, FindMatch, FindParams, FindResult, Matcher, find};
pub use library_element::{
    ElementFile, LibraryElement, LibraryElementParams, LibraryElementResult,
};
pub use natural::natural_cmp;
pub use pins::{ComponentPins, ComponentPinsParams, ComponentPinsQuery, PinPads, component_pins};
pub use properties::{ComponentSet, ComponentSetParams, ComponentSetResult};
pub use sheet::{
    SheetFrameView, SheetRef, SheetSetSize, SheetSetSizeParams, SheetTitleBlock, TitleBlock,
    title_block,
};
pub use similar::{
    SelectSimilar, SelectSimilarParams, SelectSimilarResult, SimilarField, SimilarScope,
    select_similar,
};
pub use unit_move::{UnitMove, UnitMoveParams, UnitMoveResult};
pub use update::{
    ComponentUpdate, FootprintChange, Outdated, OutdatedComponent, OutdatedResult, PartChange,
    UpdateFromLibrary, UpdateParams, UpdatePreview, UpdateResult,
};
pub use wiring::{
    NetLabelAdd, NetLabelAddParams, NetLabelAddResult, NetsParams, NoConnectAdd,
    NoConnectAddParams, NoConnectAddResult, SchematicNets, WireAdd, WireAddParams, WireAddResult,
};

/// Adds every Command defined in `reda-schematic`.
pub fn register_all(registry: &mut Registry) {
    registry.add(ComponentPlace);
    registry.add(UnitPlace);
    registry.add(ComponentSet);
    registry.add(ComponentPinsQuery);
    registry.add(Find);
    registry.add(SelectSimilar);
    registry.add(Outdated);
    registry.add(UpdatePreview);
    registry.add(UpdateFromLibrary);
    registry.add(LibraryElement);
    registry.add(SheetSetSize);
    registry.add(SheetTitleBlock);
    registry.add(WireAdd);
    registry.add(NetLabelAdd);
    registry.add(UnitMove);
    registry.add(NoConnectAdd);
    registry.add(SchematicNets);
    registry.add(Delete);
    registry.add(Copy);
    registry.add(Cut);
    registry.add(Paste);
    registry.add(Duplicate);
    registry.add(Move);
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
}

/// The Schematic Sheet with this UUID, or `not_found`.
pub(crate) fn sheet(project: &Project, uuid: Uuid) -> Result<&SchematicSheet, CommandError> {
    project
        .schematic_sheet(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Schematic Sheet has UUID {uuid}")))
}
