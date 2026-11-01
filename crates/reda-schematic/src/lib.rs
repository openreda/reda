//! Schematic Sheet behaviour: placing Components, the sheet frame and its title block, and
//! the schematic Commands. Connectivity and ERC join later.

mod sheet;

use reda_command::{CommandError, Registry};
use reda_model::{Project, SchematicSheet, Uuid};

pub use sheet::{
    SheetFrameView, SheetRef, SheetSetSize, SheetSetSizeParams, SheetTitleBlock, TitleBlock,
    title_block,
};

/// Adds every Command defined in `reda-schematic`.
pub fn register_all(registry: &mut Registry) {
    registry.add(SheetSetSize);
    registry.add(SheetTitleBlock);
}

/// The Schematic Sheet with this UUID, or `not_found`.
pub(crate) fn sheet(project: &Project, uuid: Uuid) -> Result<&SchematicSheet, CommandError> {
    project
        .schematic_sheet(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Schematic Sheet has UUID {uuid}")))
}
