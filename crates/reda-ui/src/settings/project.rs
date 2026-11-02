//! Project Settings: the settings stored in `.reda`, on four pages (Properties, ERC, Fab
//! Presets, Libraries). Everything it changes is on the Project's undo stack.

use reda_engine::model::Project;

use super::SettingsDialog;
use crate::tr;

mod erc;
mod fab_presets;
mod libraries;
mod properties;

pub use erc::ErcPage;
pub use fab_presets::{FabPresetsPage, FieldChange, PresetCopy, Preview, Status};
pub use libraries::LibrariesPage;
pub use properties::PropertiesPage;

/// The Project Settings dialog on `project`.
pub fn dialog(project: &Project) -> SettingsDialog {
    SettingsDialog::new(
        tr!("project-settings-title"),
        vec![
            Box::new(PropertiesPage::default()),
            Box::new(ErcPage::default()),
            Box::new(FabPresetsPage::default()),
            Box::new(LibrariesPage::default()),
        ],
        project,
    )
}
