//! Project Settings: the settings stored in `.reda`, on the pages Properties, ERC,
//! Schematic, Libraries and Fab Presets, then the Fab Preset pages of each Project copy
//! under its name. Everything it changes is on the Project's undo stack.

use reda_engine::model::Project;

use super::{Page, SettingsDialog};
use crate::tr;

mod erc;
mod fab_presets;
mod libraries;
mod preset_pages;
mod properties;
mod schematic;

pub use erc::ErcPage;
pub use fab_presets::{FabPresetsPage, FieldChange, PresetCopy, Preview, Status};
pub use libraries::LibrariesPage;
pub use preset_pages::CopyPage;
pub use properties::PropertiesPage;
pub use schematic::SchematicPage;

/// The Project Settings dialog on `project`, with the pages of the copies it has now.
pub fn dialog(project: &Project) -> SettingsDialog {
    let mut pages: Vec<Box<dyn Page>> = vec![
        Box::new(PropertiesPage::default()),
        Box::new(ErcPage::default()),
        Box::new(SchematicPage::default()),
        Box::new(LibrariesPage::default()),
        Box::new(FabPresetsPage::default()),
    ];
    for copy in project.fab_presets() {
        for page in CopyPage::all(project, copy.uuid) {
            pages.push(Box::new(page));
        }
    }
    SettingsDialog::new(tr!("project-settings-title"), pages, project)
}
