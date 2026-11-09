//! The placement Tool: a Library Part follows the pointer over a sheet (or, for a
//! Mechanical Part, the board) until a click places it.
//!
//! - `tool.place` (the Library panel's click or drag) picks the Part up and makes
//!   [`Tool::Place`](crate::tools::Tool::Place) active. What is in flight is view state
//!   ([`Placement`]), so the in-flight Commands change it the way keys and menus do:
//!   `tool.rotate` (Space), `tool.mirror_x`/`tool.mirror_y` (X, Y), `tool.flip_side` (L,
//!   the board side) and `tool.place.set` (the Properties panel, which Tab opens with
//!   `tool.edit_properties`).
//! - The pointer draws a preview: the Project as the place Command would leave it
//!   ([`Engine::preview`](reda_engine::Engine::preview)), drawn in place of the real one.
//!   Neither the Project nor the Journal changes until a click, which runs exactly one
//!   Command (`schematic.component.place` or `pcb.mechanical.place`).
//! - Placement repeats per the `schematic.repeat_placement` Preference, keeping the turn
//!   and the edits, with an edited Designator counting up to the next free one. Esc
//!   (`tool.activate:select`) ends it.

pub(crate) mod commands;

use std::collections::BTreeMap;

use reda_engine::command::PartCopy;
use reda_engine::library::ReadyParts;
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{Project, Side, Uuid};
use serde_json::{Value, json};

use crate::canvas::CanvasInputs;
use crate::canvas::interact::Pointer;
use crate::shell::Shell;

pub use commands::register_all;

/// Where a placement goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A Schematic Sheet: a new Component from a Part with a Symbol.
    Sheet,
    /// The PCB: a Mechanical Part.
    Board,
}

/// A Part in flight, with what was done to it so far.
#[derive(Clone, Debug)]
pub struct Placing {
    pub document: Uuid,
    pub target: Target,
    /// The Part as placing copies it, read from the Libraries once.
    pub copy: PartCopy,
    /// The Unit to place, by name; the first by default.
    pub unit: Option<String>,
    /// The alternate Footprint; the first by default.
    pub footprint: Option<Uuid>,
    /// Counter-clockwise, 0 to 360°.
    pub rotation: Millideg,
    /// Mirrored left to right before turning (sheets only).
    pub mirror: bool,
    /// The board side (the board only).
    pub side: Side,
    /// The Designator the next placement takes; the next free one by default.
    pub designator: Option<String>,
    /// Field values over the Part's defaults, by name.
    pub fields: BTreeMap<String, String>,
    /// A further Unit of this placed Component (Properties' **Place**), rather than a new
    /// Component: placed once with `schematic.unit.place`.
    pub component: Option<Uuid>,
}

impl Placing {
    /// Turns the placement 90° counter-clockwise.
    pub fn rotate(&mut self) {
        self.rotation = Millideg(self.rotation.0 + 90_000).normalized();
    }

    /// Mirrors left to right (`vertical_axis`) or top to bottom, on a sheet. A mirror
    /// about the vertical axis after a turn equals the mirrored shape turned the other
    /// way; top to bottom is that and a half turn.
    pub fn mirror(&mut self, vertical_axis: bool) {
        if self.target != Target::Sheet {
            return;
        }
        let turn = if vertical_axis { 0 } else { 180_000 };
        self.rotation = Millideg(turn - self.rotation.0).normalized();
        self.mirror = !self.mirror;
    }

    /// Flips a board placement to the other side.
    pub fn flip(&mut self) {
        if self.target == Target::Board {
            self.side = match self.side {
                Side::Top => Side::Bottom,
                Side::Bottom => Side::Top,
            };
        }
    }

    /// The Command that places the Part with its origin at `at`.
    pub fn command(&self, at: Point) -> (&'static str, Value) {
        if let Some(component) = self.component {
            let params = json!({
                "document": self.document,
                "component": component,
                "unit": self.unit,
                "at": {"x": at.x, "y": at.y},
                "rotation": self.rotation.0,
                "mirror": self.mirror,
            });
            return ("schematic.unit.place", params);
        }
        let mut params = json!({
            "part": self.copy.part.uuid,
            "at": {"x": at.x, "y": at.y},
            "rotation": self.rotation.0,
        });
        let p = params.as_object_mut().expect("an object");
        if let Some(f) = self.footprint {
            p.insert("footprint".into(), json!(f));
        }
        if let Some(d) = &self.designator {
            p.insert("designator".into(), json!(d));
        }
        if !self.fields.is_empty() {
            p.insert("fields".into(), json!(self.fields));
        }
        match self.target {
            Target::Sheet => {
                p.insert("document".into(), json!(self.document));
                p.insert("mirror".into(), json!(self.mirror));
                if let Some(u) = &self.unit {
                    p.insert("unit".into(), json!(u));
                }
                ("schematic.component.place", params)
            }
            Target::Board => {
                p.insert("side".into(), json!(self.side));
                ("pcb.mechanical.place", params)
            }
        }
    }
}

/// The placement Tool's view state.
#[derive(Debug, Default)]
pub struct Placement {
    /// The Part in flight, while the placement Tool is active.
    pub placing: Option<Placing>,
    /// Where the pointer last was over the Document, snapped.
    at: Option<Point>,
    /// The Document a preview is drawn on, and the Command it shows.
    shown: Option<(Uuid, Value)>,
}

/// The Part in flight, if the placement Tool is active.
pub fn placing(shell: &Shell) -> Option<&Placing> {
    shell.engine().view().get::<Placement>()?.placing.as_ref()
}

fn state(shell: &mut Shell) -> Option<&mut Placement> {
    shell.engine.view_mut().get_mut::<Placement>()
}

/// `at` on `document`'s snap grid.
fn snapped(shell: &Shell, document: Uuid, at: Point) -> Point {
    let xy = (at.x.0 as f64, at.y.0 as f64);
    shell.snap_to_grid(document, xy)
}

/// Whether the Part in flight goes on `document`.
fn placing_on(shell: &Shell, document: Uuid) -> bool {
    placing(shell).is_some_and(|p| p.document == document)
}

/// Feeds a frame of the canvas of `document` to the Tool: the pointer moving over it
/// (or dragging a Part from the Library panel over it) previews, a click or a drop
/// places.
pub fn canvas(shell: &mut Shell, document: Uuid, input: &CanvasInputs) {
    let point = |(x, y): (f64, f64)| Point::new(Nm(x.round() as i64), Nm(y.round() as i64));
    for &event in &input.events {
        match event {
            Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragStart { at, .. } => {
                hover(shell, document, point(at));
            }
            Pointer::Click { at, .. } | Pointer::DragEnd(at) => {
                click(shell, document, point(at));
            }
        }
    }
    // A Part dragged from the Library panel: the canvas is not what was pressed, so it
    // sees no pointer events, only egui's drag-and-drop payload.
    let pos = input.response.ctx.input(|i| i.pointer.latest_pos());
    let Some(pos) = pos.filter(|p| input.rect.contains(*p)) else {
        return;
    };
    let at = point(input.document_point(pos));
    if input
        .response
        .dnd_release_payload::<crate::panels::library::Dragged>()
        .is_some()
    {
        click(shell, document, at);
    } else if input
        .response
        .dnd_hover_payload::<crate::panels::library::Dragged>()
        .is_some()
    {
        hover(shell, document, at);
    }
}

/// The pointer is at `at` over `document`: the Part in flight follows it, snapped.
pub fn hover(shell: &mut Shell, document: Uuid, at: Point) {
    if !placing_on(shell, document) {
        return;
    }
    let at = snapped(shell, document, at);
    if let Some(state) = state(shell) {
        state.at = Some(at);
    }
    refresh(shell);
}

/// A click at `at` over `document`: places the Part in flight there, as one Command.
/// Placement then repeats per Preference, with the next Designator.
pub fn click(shell: &mut Shell, document: Uuid, at: Point) {
    if !placing_on(shell, document) {
        return;
    }
    let at = snapped(shell, document, at);
    let Some((id, params)) = placing(shell).map(|p| p.command(at)) else {
        return;
    };
    if shell.run_reporting(id, params).is_err() {
        return;
    }
    let one_unit = placing(shell).is_some_and(|p| p.component.is_some());
    if one_unit || !shell.preferences().schematic.repeat_placement {
        let _ = shell.run_reporting("tool.activate:select", json!({}));
        return;
    }
    let next = {
        let by = shell.preferences().schematic.auto_increment.primary;
        let project = shell.engine().project();
        let designator = placing(shell).and_then(|p| p.designator.clone());
        project
            .zip(designator)
            .and_then(|(p, d)| next_designator(p, &d, by))
    };
    if let Some(state) = state(shell) {
        if let Some(placing) = &mut state.placing {
            placing.designator = next;
        }
        state.at = Some(at);
        // The real Project is drawn now; show the next placement over it.
        state.shown = None;
    }
    refresh(shell);
}

/// Draws the preview of the Part in flight where the pointer last was, unless it shows
/// already; with nothing in flight, drops a preview left drawn.
pub fn refresh(shell: &mut Shell) {
    let Some(placement) = shell.engine().view().get::<Placement>() else {
        return;
    };
    let wanted = placement
        .placing
        .as_ref()
        .zip(placement.at)
        .map(|(p, at)| (p.document, p.command(at), p.copy.clone()));
    let shown = placement.shown.clone();
    let Some((document, (id, params), copy)) = wanted else {
        if let Some((document, _)) = shown {
            shell.redraw(document);
            if let Some(state) = state(shell) {
                state.shown = None;
            }
        }
        return;
    };
    if shown.as_ref() == Some(&(document, params.clone())) {
        return;
    }
    if let Some((other, _)) = &shown
        && *other != document
    {
        shell.redraw(*other);
    }
    let parts = ReadyParts::new(vec![copy]);
    match shell.engine().preview(id, params.clone(), &parts) {
        Ok((project, _)) => shell.show_preview(document, &project),
        // Say why it cannot go here (a Designator in use, say); the click would fail too.
        Err(e) => {
            shell.redraw(document);
            shell.report(crate::tr!("error-command", error = e.message));
        }
    }
    if let Some(state) = state(shell) {
        state.shown = Some((document, params));
    }
}

/// The Designator after `designator`: its trailing number stepped `by` (the
/// `schematic.auto_increment.primary` Preference), padding kept, skipping every one a
/// Component or board Footprint uses. `None` when it ends in no number (or counting down
/// finds no free one), so the next placement takes the next free one for its prefix.
pub fn next_designator(project: &Project, designator: &str, by: i64) -> Option<String> {
    let used = designators(project);
    reda_engine::model::auto_increment::step_designator(designator, by, |d| !used.contains(d))
}

/// Every Designator in use on the sheets and the board.
fn designators(project: &Project) -> std::collections::BTreeSet<String> {
    let mut used: std::collections::BTreeSet<String> = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units())
        .map(|u| u.designator.clone())
        .collect();
    if let Some(pcb) = project.pcb() {
        used.extend(pcb.footprints().map(|f| f.designator.clone()));
    }
    used
}
