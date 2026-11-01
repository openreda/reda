//! Schematic Sheet behaviour: placing Components, the sheet frame and its title block, and
//! the schematic Commands. Connectivity and ERC join later.

mod component;
mod find;
mod natural;
mod pins;
mod properties;
mod sheet;
mod similar;

use reda_command::{CommandError, Registry};
use reda_model::{Project, SchematicSheet, Uuid};

pub use component::{
    At, ComponentPlace, ComponentPlaceParams, PlacedResult, UnitPlace, UnitPlaceParams,
    next_free_designator,
};
pub use find::{Find, FindField, FindMatch, FindParams, FindResult, Matcher, find};
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

/// Adds every Command defined in `reda-schematic`.
pub fn register_all(registry: &mut Registry) {
    registry.add(ComponentPlace);
    registry.add(UnitPlace);
    registry.add(ComponentSet);
    registry.add(ComponentPinsQuery);
    registry.add(Find);
    registry.add(SelectSimilar);
    registry.add(SheetSetSize);
    registry.add(SheetTitleBlock);
}

/// The Schematic Sheet with this UUID, or `not_found`.
pub(crate) fn sheet(project: &Project, uuid: Uuid) -> Result<&SchematicSheet, CommandError> {
    project
        .schematic_sheet(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Schematic Sheet has UUID {uuid}")))
}
