//! The Edit Cursor (docs/spec/v0.3/input-and-defaults.md#the-edit-cursor): the snapped
//! point where a Tool acts, on the PCB, Footprint, Schematic and Symbol canvases. It
//! follows the mouse pointer, and the arrow keys step it.
//!
//! - `view.cursor.step {dx, dy}` (the arrows) moves it by whole grid steps and
//!   `view.cursor.click` (Enter where the Tool has not bound it, Shift+Enter everywhere)
//!   clicks at it. Both are view Commands on [`EditCursors`] in the engine's view state, so
//!   their results say where the Edit Cursor is.
//! - [`canvas`] carries them out on the canvas's next frame: while the keys hold the Edit
//!   Cursor the Tools get it in place of the pointer, a key click is a click there, the
//!   view follows it under Auto-pan and the system pointer is moved to it where the
//!   `canvas.arrows_move_pointer` Preference asks and the desktop lets REDA
//!   ([`moves_pointer`]). The mouse takes it back once it moves more than [`DETACH`]
//!   points.
//! - While the keys hold it, snapping ignores Hotspots, Guides and Alignment Guides
//!   ([`keyed`], read by [`crate::snap::rules_on`]): the keys are for an exact count of
//!   steps.

use std::collections::BTreeMap;

use egui::{Modifiers, Pos2, Ui};
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::bars::DocumentType;
use crate::canvas::CanvasInputs;
use crate::canvas::auto_pan;
use crate::canvas::interact::Pointer;
use crate::grid::Grids;
use crate::shell::Shell;
use crate::tools::{ActiveTool, Tool};

/// How far (points) the mouse moves before the Edit Cursor follows it again.
pub const DETACH: f32 = 4.0;

/// One axis of the grid an arrow steps on: its lines run through `origin` every `pitch`
/// nm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridAxis {
    pub origin: i64,
    pub pitch: i64,
}

impl GridAxis {
    /// The line nearest `at`.
    pub fn nearest(self, at: i64) -> i64 {
        let pitch = self.pitch.max(1);
        let off = (at - self.origin).rem_euclid(pitch);
        if 2 * off >= pitch {
            at - off + pitch
        } else {
            at - off
        }
    }
}

/// `at` moved `steps` grid steps along `axis`. From a point off the grid the first step
/// lands on the next line in that direction, and the rest step from there.
pub fn step_axis(at: i64, axis: GridAxis, steps: i64) -> i64 {
    let pitch = axis.pitch.max(1);
    let off = (at - axis.origin).rem_euclid(pitch);
    match steps.signum() {
        0 => at,
        _ if off == 0 => at + steps * pitch,
        1 => at - off + steps * pitch,
        _ => at - off + (steps + 1) * pitch,
    }
}

/// The grid an arrow steps on, per axis (X ≠ Y and Local Grids give each its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepGrid {
    pub x: GridAxis,
    pub y: GridAxis,
}

impl StepGrid {
    /// A square grid of `pitch` through `origin`.
    pub fn square(pitch: Nm, origin: Point) -> StepGrid {
        StepGrid {
            x: GridAxis {
                origin: origin.x.0,
                pitch: pitch.0,
            },
            y: GridAxis {
                origin: origin.y.0,
                pitch: pitch.0,
            },
        }
    }

    /// `at` moved `dx` and `dy` steps. An axis that does not move is put on its nearest
    /// line when `snapping` (the grid level is in force), as the Tool would snap it.
    pub fn step(self, at: Point, dx: i64, dy: i64, snapping: bool) -> Point {
        let axis = |at: i64, axis: GridAxis, steps: i64| match steps {
            0 if snapping => axis.nearest(at),
            steps => step_axis(at, axis, steps),
        };
        Point::new(Nm(axis(at.x.0, self.x, dx)), Nm(axis(at.y.0, self.y, dy)))
    }
}

/// One canvas's Edit Cursor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditCursor {
    /// The pointer's last Document point (nm) over the canvas.
    pub pointer: Option<(f64, f64)>,
    /// The canvas's centre last frame: where the keys start before the pointer has been
    /// over it.
    pub centre: Option<(f64, f64)>,
    /// Where the keys put the Edit Cursor, while it leaves the pointer behind.
    pub keyed: Option<Point>,
    /// Key clicks waiting for the canvas's next frame.
    clicks: u32,
    /// Counts the steps, so the canvas sees a new one.
    steps: u64,
    /// The last step the canvas carried out.
    seen: u64,
    /// Where the mouse pointer was (or was moved to) on screen when the keys took the
    /// Edit Cursor: moving it more than [`DETACH`] from here gives the Edit Cursor back.
    anchor: Option<Pos2>,
    /// Where the pointer was before REDA moved it, until it is seen where it was moved
    /// to: the desktop moves it a frame or more later, so staying here is no mouse move.
    warp_from: Option<Pos2>,
    /// Where the system pointer was last moved to, on screen.
    pub warped: Option<Pos2>,
}

impl EditCursor {
    /// Where the Edit Cursor is on the canvas: where the keys put it, else the pointer's
    /// last point over it.
    pub fn shown(&self) -> Option<(f64, f64)> {
        self.keyed
            .map(|p| (p.x.0 as f64, p.y.0 as f64))
            .or(self.pointer)
    }

    /// Where the Edit Cursor is: where the keys put it, else the pointer, else the
    /// canvas's centre.
    pub fn at(&self) -> Option<(f64, f64)> {
        self.keyed
            .map(|p| (p.x.0 as f64, p.y.0 as f64))
            .or(self.pointer)
            .or(self.centre)
    }
}

/// Every canvas's Edit Cursor, in the engine's view state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditCursors(pub BTreeMap<Uuid, EditCursor>);

/// The Edit Cursor of `document`'s canvas, if it has one yet.
pub fn of(shell: &Shell, document: Uuid) -> Option<&EditCursor> {
    shell.engine().view().get::<EditCursors>()?.0.get(&document)
}

/// Whether the keys hold `document`'s Edit Cursor, so snapping leaves out Hotspots,
/// Guides and Alignment Guides.
pub fn keyed(shell: &Shell, document: Uuid) -> bool {
    of(shell, document).is_some_and(|c| c.keyed.is_some())
}

/// Adds the Commands to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(Step);
    registry.add(Click);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepParams {
    /// The Document, or open Library element, whose Edit Cursor to move; the active one
    /// when left out.
    #[serde(default)]
    document: Option<Uuid>,
    /// Grid steps to the right as the canvas shows it; negative goes left. On a board
    /// seen from below, right is towards smaller X.
    #[schemars(range(min = -1000, max = 1000))]
    dx: i64,
    /// Grid steps up; negative goes down.
    #[schemars(range(min = -1000, max = 1000))]
    dy: i64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClickParams {
    /// The Document, or open Library element, to click on; the active one when left out.
    #[serde(default)]
    document: Option<Uuid>,
}

/// Where the Edit Cursor is.
#[derive(Serialize, JsonSchema)]
pub struct CursorAt {
    document: Uuid,
    /// The Document point (nm) it is at.
    at: Point,
}

/// `view.cursor.step` (the arrow keys).
struct Step;

impl Command for Step {
    const META: Meta = Meta {
        id: "view.cursor.step",
        title: "Step Edit Cursor",
        description: "Moves the Edit Cursor of a canvas, the active one by default, by whole \
                      grid steps: `dx` to the right as the canvas shows it and `dy` up. From \
                      a point off the grid the first step lands on the next grid line. \
                      Hotspots and Guides are left out, and the Tool's object in flight \
                      moves with it.",
        category: "view",
        icon: "move",
        kind: Kind::View,
    };
    type Params = StepParams;
    type Output = CursorAt;

    fn run(&self, cx: &mut Context<'_>, p: StepParams) -> Result<CursorAt, CommandError> {
        let document = crate::navigation::canvas_of(cx, p.document)?;
        let (grid, snapping, mirrored) = step_grid(cx, document)?;
        let cursors = cx.view::<EditCursors>()?;
        let cursor = cursors.0.entry(document).or_default();
        let start = match (cursor.keyed, cursor.at()) {
            (Some(keyed), _) => keyed,
            (None, Some((x, y))) => {
                let p = Point::new(Nm(x.round() as i64), Nm(y.round() as i64));
                // The Edit Cursor follows the pointer on the grid level.
                match snapping {
                    true => grid.step(p, 0, 0, true),
                    false => p,
                }
            }
            (None, None) => Point::new(Nm(grid.x.origin), Nm(grid.y.origin)),
        };
        let dx = if mirrored { -p.dx } else { p.dx };
        let at = grid.step(start, dx, p.dy, snapping);
        cursor.keyed = Some(at);
        cursor.steps += 1;
        Ok(CursorAt { document, at })
    }
}

/// `view.cursor.click` (Enter, Shift+Enter).
struct Click;

impl Command for Click {
    const META: Meta = Meta {
        id: "view.cursor.click",
        title: "Click at Edit Cursor",
        description: "Acts as a left click at the Edit Cursor of a canvas, the active one by \
                      default: the Tool in use places, picks or adds a corner there.",
        category: "view",
        icon: "select",
        kind: Kind::View,
    };
    type Params = ClickParams;
    type Output = CursorAt;

    fn run(&self, cx: &mut Context<'_>, p: ClickParams) -> Result<CursorAt, CommandError> {
        let document = crate::navigation::canvas_of(cx, p.document)?;
        step_grid(cx, document)?;
        let cursors = cx.view::<EditCursors>()?;
        let cursor = cursors.0.entry(document).or_default();
        let Some((x, y)) = cursor.at() else {
            return Err(CommandError::unavailable(
                "the canvas has not been shown yet",
            ));
        };
        cursor.clicks += 1;
        let at = Point::new(Nm(x.round() as i64), Nm(y.round() as i64));
        Ok(CursorAt { document, at })
    }
}

/// The grid `document`'s arrows step on, whether its grid level is in force, and whether
/// it is a board seen from below (screen right is Document left).
pub(crate) fn step_grid(
    cx: &mut Context<'_>,
    document: Uuid,
) -> Result<(StepGrid, bool, bool), CommandError> {
    let project = cx.project()?;
    let ty = crate::grid::document_type(project, document)
        .ok_or_else(|| CommandError::not_found(format!("no canvas {document} with a grid")))?;
    let origin = crate::grid::origin(Some(project), document);
    let grids = cx.view::<Grids>()?.clone();
    let pitch = grids.of(cx.project().ok(), document);
    let pin = ty == DocumentType::Symbol && cx.view::<ActiveTool>().is_ok_and(|t| t.0 == Tool::Pin);
    let pitch = if pin {
        crate::symbol_editor::PIN_GRID
    } else {
        pitch
    };
    let gridless = cx
        .view::<crate::working_state::WorkingState>()
        .is_ok_and(|w| w.snap(document, ty).gridless);
    let mirrored = ty == DocumentType::Pcb
        && cx
            .view::<crate::pcb::BoardView>()
            .is_ok_and(|v| v.layers.from_bottom);
    Ok((StepGrid::square(pitch, origin), !gridless, mirrored))
}

/// Whether the desktop lets REDA move the system pointer, as kept on `ctx` by
/// [`remember_moves_pointer`]; yes until something says otherwise.
pub fn moves_pointer(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(moves_pointer_id()))
        .unwrap_or(true)
}

/// Keeps on `ctx` whether the desktop lets REDA move the system pointer.
pub fn remember_moves_pointer(ctx: &egui::Context, allowed: bool) {
    ctx.data_mut(|d| d.insert_temp(moves_pointer_id(), allowed));
}

fn moves_pointer_id() -> egui::Id {
    egui::Id::NULL.with("reda-edit-cursor-moves-pointer")
}

/// Whether this desktop lets an application move the system pointer: Wayland does not
/// (winit refuses there); Windows, macOS and X11 do.
pub fn desktop_moves_pointer() -> bool {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    {
        use crate::title_bar::wayland;
        if let wayland::Session::Wayland(_) = wayland::session(wayland::env) {
            return false;
        }
    }
    true
}

/// Carries out `document`'s Edit Cursor on its canvas this frame, before the editor gets
/// `input`'s events: records the pointer, gives the Edit Cursor back to a mouse that
/// moved, follows a new step with the view and the system pointer, puts the Edit Cursor
/// in the pointer's place while the keys hold it, and adds the key clicks.
pub fn canvas(ui: &Ui, shell: &mut Shell, document: Uuid, input: &mut CanvasInputs) {
    let prefs = shell.preferences();
    let warp = prefs.canvas.arrows_move_pointer && moves_pointer(ui.ctx());
    let auto_pan = prefs.editing.auto_pan.clone();
    let latest = ui.input(|i| i.pointer.latest_pos());
    let Some(cursors) = shell.engine.view_mut().get_mut::<EditCursors>() else {
        return;
    };
    let cursor = cursors.0.entry(document).or_default();
    cursor.centre = Some(input.camera.center);
    if let Some(at) = input.events.last().map(|e| match *e {
        Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragEnd(at) => at,
        Pointer::Click { at, .. } | Pointer::DragStart { at, .. } => at,
    }) {
        cursor.pointer = Some(at);
    }
    // The mouse takes the Edit Cursor back once it moves a few points.
    if cursor.keyed.is_some() && cursor.seen == cursor.steps {
        let near = |a: Option<Pos2>, p: Pos2| a.is_some_and(|a| (p - a).length() <= DETACH);
        match (cursor.anchor, latest) {
            (Some(_), Some(p)) if near(cursor.anchor, p) => cursor.warp_from = None,
            // Still where it was before it was moved: the desktop has not moved it yet.
            (Some(_), Some(p)) if near(cursor.warp_from, p) => {}
            (Some(_), Some(_)) => {
                cursor.keyed = None;
                cursor.anchor = None;
                cursor.warp_from = None;
            }
            (None, Some(p)) => cursor.anchor = Some(p),
            _ => {}
        }
    }
    let mut camera = None;
    if let Some(keyed) = cursor.keyed
        && cursor.seen != cursor.steps
    {
        cursor.seen = cursor.steps;
        let at = (keyed.x.0 as f64, keyed.y.0 as f64);
        // Leaving the view: it follows under the Auto-pan Preference.
        let shown = input.camera.screen_point(input.rect, at);
        let mut moved = input.camera;
        match auto_pan::follow(&auto_pan, input.rect, shown) {
            auto_pan::Pan::By(v) => moved.pan(-v),
            auto_pan::Pan::Center(_) => moved.center = at,
            auto_pan::Pan::Still => {}
        }
        if moved != input.camera {
            input.camera = moved;
            camera = Some(moved);
        }
        let screen = input.screen_point(at);
        if warp && input.rect.contains(screen) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CursorPosition(screen));
            if cursor.warp_from.is_none() {
                cursor.warp_from = latest;
            }
            cursor.anchor = Some(screen);
            cursor.warped = Some(screen);
        } else if cursor.anchor.is_none() {
            cursor.anchor = latest;
        }
    }
    if let Some(keyed) = cursor.keyed {
        let at = (keyed.x.0 as f64, keyed.y.0 as f64);
        let mut hovered = false;
        for event in &mut input.events {
            match event {
                Pointer::Hover(p) => {
                    *p = at;
                    hovered = true;
                }
                Pointer::Click { at: p, .. } => *p = at,
                _ => {}
            }
        }
        if !hovered {
            input.events.insert(0, Pointer::Hover(at));
        }
    }
    let clicks = std::mem::take(&mut cursor.clicks);
    if let Some(at) = cursor.at() {
        for _ in 0..clicks {
            input.events.push(Pointer::Click {
                at,
                modifiers: Modifiers::NONE,
            });
        }
    }
    if clicks > 0 {
        ui.ctx().request_repaint();
    }
    if let Some(camera) = camera
        && let Some(tab) = shell.document_tab_mut(document)
    {
        tab.camera = Some(camera);
    }
}
