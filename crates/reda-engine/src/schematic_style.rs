//! How this user's Preferences and the Project's derived data draw its sheets.

use reda_model::Project;
use reda_render::SchematicStyle;

use crate::preferences::Preferences;

/// Where a sheet is drawn. The canvas draws marks that never reach an output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawn {
    /// The schematic canvas.
    Canvas,
    /// A PDF, a print or another output.
    Output,
}

/// The [`SchematicStyle`] `project`'s sheets are drawn in with `preferences`.
pub fn schematic_style(project: &Project, preferences: &Preferences, drawn: Drawn) -> SchematicStyle {
    let _ = (project, drawn);
    let schematic = &preferences.schematic;
    SchematicStyle {
        unit_suffix: schematic.unit_suffix,
        single_negation: schematic.single_negation,
    }
}
