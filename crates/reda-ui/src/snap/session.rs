//! The [`Rules`] in force on an open Document, and the keys held in flight.

use reda_engine::model::LayerKind;
use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};

use super::{Grid, Objects, Rules, Snapped, Suspend, Target, snap};
use crate::bars::{self, DocumentType};
use crate::shell::Shell;
use crate::tools::Tool;
use crate::working_state::{SnapMode, WorkingState};

/// The Rules on `document`'s canvas: its working state's mode, Gridless and checklist,
/// its grid, the Snap Distance or Electrical Range, and the keys held in flight.
pub fn rules(shell: &Shell, document: Uuid) -> Rules {
    rules_on(shell, document, shell.grid(document))
}

/// [`rules`] on the grid `pitch` instead of the Document's (the Symbol editor's Pin Tool
/// keeps Pins on their own grid).
pub fn rules_on(shell: &Shell, document: Uuid, pitch: Nm) -> Rules {
    let ty = bars::document_type(shell, document).unwrap_or(DocumentType::Schematic);
    let options = shell
        .engine()
        .view()
        .get::<WorkingState>()
        .map(|w| w.snap(document, ty))
        .unwrap_or_else(|| crate::working_state::SnapOptions::default_for(ty));
    let suspend = suspend(shell);
    let capture = match ty {
        DocumentType::Pcb | DocumentType::Footprint => shell.preferences().pcb.snap_distance.nm(),
        DocumentType::Schematic | DocumentType::Symbol if pitch == shell.grid(document) => {
            shell.electrical_range(document)
        }
        DocumentType::Schematic | DocumentType::Symbol => crate::grid::default_range(pitch),
    };
    let grid = (!options.gridless && !suspend.grid()).then(|| Grid {
        pitch,
        origin: shell.grid_origin(document),
    });
    let objects = match (suspend.objects(), options.mode) {
        (true, _) | (_, SnapMode::Off) => Objects::Off,
        (_, SnapMode::On | SnapMode::AllLayers) => Objects::All,
        (_, SnapMode::ActiveLayer) => active_layer(shell, document).unwrap_or(Objects::All),
    };
    Rules {
        capture,
        grid,
        objects,
        off: options.hotspots_off,
        guides: !suspend.objects(),
    }
}

/// `at` on `document` snapped onto `targets` by its [`rules`].
pub fn snap_at(shell: &Shell, document: Uuid, at: (f64, f64), targets: &[Target]) -> Snapped {
    snap(at, targets, &[], &rules(shell, document))
}

/// `at` on `document`'s grid level alone (free when Gridless or suspended): where a
/// grabbed point or a dragged segment lands.
pub fn to_grid(shell: &Shell, document: Uuid, at: (f64, f64)) -> Point {
    rules(shell, document).to_grid(at)
}

/// The Active Layer mode on the open board: its active Layer.
fn active_layer(shell: &Shell, document: Uuid) -> Option<Objects> {
    let project = shell.engine().project()?;
    let pcb = project.pcb_document(document)?;
    let layer = shell.board_view().active_layer(project)?;
    let kind = pcb.layers().layer(layer)?.kind;
    Some(Objects::Layer {
        layer,
        copper: kind == LayerKind::Copper,
    })
}

/// The suspend keys as last fed.
pub fn suspend(shell: &Shell) -> Suspend {
    shell
        .engine()
        .view()
        .get::<Suspend>()
        .copied()
        .unwrap_or_default()
}

/// The suspend keys after this frame over `document`'s canvas: `Mod` and Shift held, and
/// whether the primary button is down. A Tool other than Select, a button held, or a move
/// carried means something is in flight. [`Shell::hold_snap_keys`] keeps the result.
pub fn held(shell: &Shell, document: Uuid, modifiers: egui::Modifiers, pressed: bool) -> Suspend {
    let in_flight = pressed || shell.tool() != Tool::Select || carrying(shell, document);
    let mut keys = suspend(shell);
    keys.update(in_flight, modifiers.command, modifiers.shift);
    keys
}

fn carrying(shell: &Shell, document: Uuid) -> bool {
    if shell.pcb_tool().moving() {
        return true;
    }
    shell.document_tab(document).is_some_and(|tab| {
        matches!(
            tab.interaction,
            crate::schematic_editor::Interaction::Flight(_)
                | crate::schematic_editor::Interaction::Placing(_)
        )
    })
}
