//! How this user's Preferences and the Project's derived data draw its sheets.

use reda_model::Project;
use reda_render::{NoteLink, SchematicStyle};

use crate::preferences::Preferences;

/// Where a sheet is drawn. The canvas draws marks that never reach an output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawn {
    /// The schematic canvas.
    Canvas,
    /// A PDF, a print or another output.
    Output,
}

/// The [`SchematicStyle`] `project`'s sheets are drawn in with `preferences`: the
/// drawing Preferences, and the Port Cross-References the Project shows.
pub fn schematic_style(
    project: &Project,
    preferences: &Preferences,
    drawn: Drawn,
) -> SchematicStyle {
    let schematic = &preferences.schematic;
    let styles =
        (project.props().schematic).cross_reference_styles(schematic.cross_reference_styles());
    SchematicStyle {
        unit_suffix: schematic.unit_suffix,
        single_negation: schematic.single_negation,
        canvas: drawn == Drawn::Canvas,
        cross_overs: drawn == Drawn::Canvas && schematic.display_cross_overs,
        cross_references: reda_schematic::cross_ref::notes(project, styles),
        cross_reference_links: reda_schematic::cross_ref::note_entries_by_port(project, styles)
            .into_iter()
            .map(|(port, entries)| {
                let links = entries
                    .into_iter()
                    .map(|e| NoteLink {
                        entry: e.text,
                        sheet: e.document,
                        at: e.at,
                    })
                    .collect();
                (port, links)
            })
            .collect(),
    }
}
