//! What the status bar reads out about the active Document: its kind, where the cursor is
//! in the Document's units, and its grid (docs/spec/v0.1/app.md#window-shell). No drawing:
//! the window lays these out, then the out-of-sync badge, the Agents indicator and the
//! Panels button.

use reda_engine::preferences::Units;

use crate::bars::{self, DocumentType};
use crate::grid;
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;

/// The status bar's readout of the active Document; all `None` with no Document active.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Readout {
    /// "Active: Schematic Sheet".
    pub kind: Option<String>,
    /// "X 1.27 mm  Y 2.54 mm", once the pointer has been over the canvas.
    pub cursor: Option<String>,
    /// "Grid: 50 mil".
    pub grid: Option<String>,
    /// The Wire Tool's mode while it is active on a sheet: "Wire: 45°".
    pub tool: Option<String>,
}

/// The readout of the active Document. A board reads where the cursor last snapped,
/// from its Output Origin, in the board's units; a sheet or Library element reads the
/// pointer's last position in its grid's units.
pub fn readout(shell: &Shell) -> Readout {
    let Some(tab) = shell.active_document() else {
        return Readout::default();
    };
    let document = tab.document;
    let kind = match bars::document_type(shell, document) {
        Some(DocumentType::Schematic) => tr!("document-kind-schematic"),
        Some(DocumentType::Pcb) => tr!("document-kind-pcb"),
        Some(DocumentType::Symbol) => tr!("document-kind-symbol"),
        Some(DocumentType::Footprint) => tr!("document-kind-footprint"),
        None => tr!("document-kind-part"),
    };
    let canvas = crate::element_editor::has_canvas(shell.engine(), document)
        || !crate::element_editor::is_element(shell.engine(), document);
    let pitch = shell.grid(document);
    let cursor = if shell.is_pcb(document) {
        let snapped = match shell.tool() {
            Tool::Route | Tool::Via => shell.router().snap(),
            _ => shell.pcb_snap(),
        };
        snapped.map(|crate::snap::Snapped { at, .. }| {
            let at = shell.pcb_cursor_from_origin(at);
            (at.x.0 as f64, at.y.0 as f64, shell.board_view().units)
        })
    } else {
        let units = if grid::is_imperial(pitch) {
            Units::Mil
        } else {
            Units::Mm
        };
        tab.cursor.map(|(x, y)| (x, y, units))
    };
    let cursor = cursor.map(|(x, y, units)| {
        tr!(
            "status-cursor",
            x = coordinate(x, units),
            y = coordinate(y, units)
        )
    });
    let wiring = shell.tool() == Tool::Wire
        && shell
            .engine()
            .project()
            .is_some_and(|p| p.schematic_sheet(document).is_some());
    let tool = wiring.then(|| {
        let mode = shell
            .engine()
            .view()
            .get::<crate::tools::WireTool>()
            .map(|t| t.mode)
            .unwrap_or_default();
        tr!("status-wire-mode", mode = mode.label())
    });
    Readout {
        kind: Some(tr!("status-active", kind = kind)),
        tool,
        cursor: cursor.filter(|_| canvas),
        grid: canvas.then(|| tr!("status-grid", grid = grid::label(pitch))),
    }
}

/// One cursor coordinate (nm): mm to two decimals, or whole mil.
fn coordinate(nm: f64, units: Units) -> String {
    let text = match units {
        Units::Mm => format!("{:.2} mm", nm / 1e6),
        Units::Mil => format!("{:.0} mil", nm / 25_400.0),
    };
    // Rounding leaves no negative zero.
    match text.strip_prefix("-") {
        Some(rest) if rest.trim_start_matches(['0', '.']).starts_with(' ') => rest.to_owned(),
        _ => text,
    }
}
