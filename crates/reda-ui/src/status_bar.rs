//! The status bar's Document segments (docs/spec/v0.1/app.md#window-shell). From the left,
//! all flat buttons:
//!
//! 1. "Active: <Document kind>"
//! 2. the cursor's X and Y in the Document's units, at the display precision
//! 3. the grid, which opens the grid menu
//! 4. the object snap mode, which cycles when clicked and is blank when off
//! 5. the active Layer swatch (boards)
//! 6. the Tool's prompt, or a summary of the object under the cursor with no Tool
//!
//! The window adds the notes, the out-of-sync badge, the bridge indicator and the Panels
//! button after these ([`crate::App`]). [`readout`] is the text, for tests; [`show`] draws.

use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::Uuid;
use reda_engine::preferences::Units;
use serde_json::{Value, json};

use crate::bars::{self, DocumentType};
use crate::grid;
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;
use crate::working_state::{SnapMode, WorkingState};

/// The status bar's readout of the active Document; all `None` with no Document active.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Readout {
    /// "Active: Schematic Sheet".
    pub kind: Option<String>,
    /// "X 1.270 mm  Y 2.540 mm", once the pointer has been over the canvas.
    pub cursor: Option<String>,
    /// "Grid: 50 mil".
    pub grid: Option<String>,
    /// The object snap mode: "Active Layer", or blank when off.
    pub snap: Option<String>,
    /// The active Layer on a board.
    pub layer: Option<String>,
    /// The Tool's prompt ("Wire: 45°", "Tool: Track"), or with no Tool the object under the
    /// cursor ("Unit R1").
    pub prompt: Option<String>,
}

/// What a click in the status bar asks the window to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// Run a Command.
    Run(String, Value),
    /// Call a Command from the GUI: one lacking params opens its dialog.
    Call(String, Value),
}

/// The units `document`'s coordinates show in: a board's own, else its grid's.
pub fn units(shell: &Shell, document: Uuid) -> Units {
    if shell.is_pcb(document) {
        shell.board_view().units
    } else if grid::is_imperial(shell.grid(document)) {
        Units::Mil
    } else {
        Units::Mm
    }
}

/// Where the cursor is on `document`, in nm, as the status bar and the HUD read it. A
/// board reads where the cursor last snapped, from its Output Origin; a sheet or Library
/// element reads the pointer's last position. X is Document X, so on a board seen from
/// below it runs right to left.
pub fn cursor(shell: &Shell, document: Uuid) -> Option<(f64, f64)> {
    if shell.is_pcb(document) {
        let snapped = match shell.tool() {
            Tool::Route | Tool::Via => shell.router().snap(),
            _ => shell.pcb_snap(),
        };
        snapped.map(|crate::snap::Snapped { at, .. }| {
            let at = shell.pcb_cursor_from_origin(at);
            (at.x.0 as f64, at.y.0 as f64)
        })
    } else {
        shell.document_tab(document).and_then(|t| t.cursor)
    }
}

/// One length (nm) in `units` at the display precision Preferences: 3 decimals for mm
/// and 1 for mil unless changed. Rounding is for display only.
pub fn length(shell: &Shell, nm: f64, units: Units) -> String {
    let precision = &shell.preferences().editing.display_precision;
    let text = match units {
        Units::Mm => format!("{:.*} mm", precision.mm as usize, nm / 1e6),
        Units::Mil => format!("{:.*} mil", precision.mil as usize, nm / 25_400.0),
    };
    // Rounding leaves no negative zero.
    match text.strip_prefix("-") {
        Some(rest) if rest.trim_start_matches(['0', '.']).starts_with(' ') => rest.to_owned(),
        _ => text,
    }
}

/// Whether `document` has a canvas (a Part's form has none).
fn has_canvas(shell: &Shell, document: Uuid) -> bool {
    crate::element_editor::has_canvas(shell.engine(), document)
        || !crate::element_editor::is_element(shell.engine(), document)
}

/// The readout of the active Document.
pub fn readout(shell: &Shell) -> Readout {
    let Some(tab) = shell.active_document() else {
        return Readout::default();
    };
    let document = tab.document;
    let ty = bars::document_type(shell, document);
    let kind = match ty {
        Some(DocumentType::Schematic) => tr!("document-kind-schematic"),
        Some(DocumentType::Pcb) => tr!("document-kind-pcb"),
        Some(DocumentType::Symbol) => tr!("document-kind-symbol"),
        Some(DocumentType::Footprint) => tr!("document-kind-footprint"),
        None => tr!("document-kind-part"),
    };
    let canvas = has_canvas(shell, document);
    let units = units(shell, document);
    let cursor = cursor(shell, document).map(|(x, y)| {
        tr!(
            "status-cursor",
            x = length(shell, x, units),
            y = length(shell, y, units)
        )
    });
    let snap = ty.filter(|_| canvas).map(|ty| {
        let mode = shell
            .engine()
            .view()
            .get::<WorkingState>()
            .map(|w| w.snap(document, ty).mode)
            .unwrap_or_else(|| SnapMode::offered(ty)[0]);
        match mode {
            SnapMode::Off => String::new(),
            mode => crate::snap::mode_label(mode),
        }
    });
    let layer = (ty == Some(DocumentType::Pcb))
        .then(|| crate::pcb::layer_control::active_layer_name(shell))
        .flatten();
    Readout {
        kind: Some(tr!("status-active", kind = kind)),
        cursor: cursor.filter(|_| canvas),
        grid: canvas.then(|| tr!("status-grid", grid = grid::label(shell.grid(document)))),
        snap,
        layer,
        prompt: prompt(shell, document).filter(|_| canvas),
    }
}

/// The Tool's prompt, or the object under the cursor when no Tool runs.
fn prompt(shell: &Shell, document: Uuid) -> Option<String> {
    let tool = shell.tool();
    let sheet = (shell.engine().project()).is_some_and(|p| p.schematic_sheet(document).is_some());
    if tool == Tool::Wire && sheet {
        let mode = shell
            .engine()
            .view()
            .get::<crate::tools::WireTool>()
            .map(|t| t.mode)
            .unwrap_or_default();
        return Some(tr!("status-wire-mode", mode = mode.label()));
    }
    if tool != Tool::Select {
        let id = format!("tool.activate:{}", tool.id());
        let title = bars::command_title(&id, tool.id());
        return Some(tr!("status-tool", tool = title));
    }
    hovered(shell, document)
}

/// A summary of the topmost object under the cursor: its kind and, for a Component, its
/// Designator.
fn hovered(shell: &Shell, document: Uuid) -> Option<String> {
    let tab = shell.document_tab(document)?;
    let at = if shell.is_pcb(document) {
        shell
            .pcb_snap()
            .map(|s| (s.at.x.0 as f64, s.at.y.0 as f64))?
    } else {
        tab.cursor?
    };
    let tolerance = tab.camera.map_or(100_000.0, |c| c.nm_per_point * 4.0);
    let uuid = crate::selection::picking::hit(shell, document, at, tolerance)?;
    let object = shell.engine().project()?.object(uuid)?;
    let value = serde_json::to_value(&object).ok()?;
    let kind = value["type"].as_str().unwrap_or("object");
    let title = kind
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    let name = ["designator", "name", "text"]
        .iter()
        .find_map(|k| value[*k].as_str().filter(|s| !s.is_empty()));
    Some(match name {
        Some(name) => format!("{title} {name}"),
        None => title,
    })
}

/// A flat status bar button whose screen reader name is `name`.
fn button(ui: &mut Ui, text: &str, name: String) -> egui::Response {
    let response = ui.add(
        egui::Button::new(text)
            .frame(false)
            .min_size(egui::vec2(12.0, 0.0)),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
    response
}

/// Draws segments 1 to 6 for the active Document, left to right, with a separator after
/// each shown one; returns what was clicked. Nothing with no Document active.
pub fn show(ui: &mut Ui, shell: &Shell) -> Vec<Out> {
    let mut out = Vec::new();
    let Some(document) = shell.active_document().map(|t| t.document) else {
        return out;
    };
    let readout = readout(shell);
    if let Some(kind) = &readout.kind {
        button(ui, kind, kind.clone());
    }
    if let Some(cursor) = &readout.cursor {
        ui.separator();
        button(ui, cursor, tr!("status-cursor-name", at = cursor.as_str()));
    }
    if let Some(grid_text) = &readout.grid {
        ui.separator();
        let face = button(ui, grid_text, grid_text.clone());
        egui::Popup::menu(&face).show(|ui| grid_menu(ui, shell, document, &mut out));
    }
    if let (Some(snap), Some(ty)) = (&readout.snap, bars::document_type(shell, document)) {
        ui.separator();
        let mode = if snap.is_empty() {
            tr!("snap-mode-off")
        } else {
            snap.clone()
        };
        let name = tr!("status-snap", mode = mode);
        if button(ui, snap, name).clicked() {
            let offered = SnapMode::offered(ty);
            let now = shell
                .engine()
                .view()
                .get::<WorkingState>()
                .map(|w| w.snap(document, ty).mode)
                .unwrap_or(offered[0]);
            let i = offered.iter().position(|m| *m == now).unwrap_or(0);
            let next = offered[(i + 1) % offered.len()];
            out.push(Out::Run(
                "view.snap.set".into(),
                json!({"document": document, "mode": next}),
            ));
        }
    }
    if readout.layer.is_some() {
        ui.separator();
        let mut calls = Vec::new();
        crate::pcb::layer_control::status_swatch(ui, shell, &mut calls);
        out.extend(calls.into_iter().map(|(id, p)| Out::Call(id, p)));
    }
    if let Some(prompt) = &readout.prompt {
        ui.separator();
        button(ui, prompt, prompt.clone());
    }
    out
}

/// The grid menu: the type's rows, Custom… and the visible grid's switch.
fn grid_menu(ui: &mut Ui, shell: &Shell, document: Uuid, out: &mut Vec<Out>) {
    let grids = shell.grids();
    let project = shell.engine().elements();
    let current = grids.of(project, document);
    let custom = grids.is_custom(document);
    for row in grids.rows(project, document) {
        let pitch = row.snap.0;
        if ui
            .selectable_label(!custom && pitch == current, grid::label(pitch))
            .clicked()
        {
            out.push(Out::Run(
                "view.grid.set".into(),
                json!({"document": document, "grid": pitch}),
            ));
            ui.close();
        }
    }
    ui.separator();
    if ui
        .selectable_label(custom, tr!("status-grid-custom"))
        .clicked()
    {
        out.push(Out::Call(
            "view.grid.custom".into(),
            json!({"document": document}),
        ));
        ui.close();
    }
    let mut shown = grids.visible(project, document).shown;
    if ui
        .checkbox(&mut shown, tr!("status-grid-visible"))
        .changed()
    {
        out.push(Out::Run(
            "view.grid.show".into(),
            json!({"document": document, "visible": shown}),
        ));
    }
}
