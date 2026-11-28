//! How a moved object is held and steered (docs/spec/v0.1/schematic.md#editing):
//!
//! - **Grab point:** the Preferences Object's Electrical Hot Spot (on) and Center of
//!   Object (off). Hot Spot holds an electrical object at its connection point nearest the
//!   click and overrides Center for it; Center holds an object by its reference point or
//!   centre; with neither it is held where it was clicked ([`grab_point`]). The held
//!   point keeps its distance from the pointer and is what snaps as the drag goes, so
//!   nothing jumps on the press.
//! - **Alt** held during a drag keeps it horizontal or vertical ([`DragKeys`]).
//! - **`Mod`+Space** during a drag turns the rubber-banded Wires' corners the other way
//!   ([`flip_corners`], bound to `tool.cycle_corner`).
//! - **`Mod`+arrows** nudge the selection one snap grid (ten with Shift), rubber-banding
//!   like a drag, one `schematic.drag` and one undo step per press ([`nudge`], the view
//!   Command `schematic.nudge`). The step is measured at the first selected object, as on
//!   the other canvases ([`crate::nudge`]): from off the grid it lands on the next line.

use reda_engine::command::{Command, CommandError, Context, Kind, Meta};
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{SchematicSheet, Uuid};
use reda_engine::preferences::SelectionEditing;
use reda_engine::schematic::connectivity::placed_pins;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Interaction, restate};
use crate::shell::Shell;

/// The modifier keys held over a canvas, for a drag in flight: the window sets it each
/// frame ([`Shell::hold_snap_keys`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DragKeys {
    pub alt: bool,
}

/// Whether Alt is held.
pub(super) fn alt_held(shell: &Shell) -> bool {
    shell
        .engine()
        .view()
        .get::<DragKeys>()
        .is_some_and(|k| k.alt)
}

/// Where `object` on `sheet` is held when grabbed at `at`, per the grab Preferences;
/// `None` holds it where it was clicked.
pub fn grab_point(
    sheet: &SchematicSheet,
    prefs: &SelectionEditing,
    object: Uuid,
    at: Point,
) -> Option<Point> {
    let nearest = |points: Vec<Point>| {
        points.into_iter().min_by_key(|p| {
            let (dx, dy) = ((p.x.0 - at.x.0) as i128, (p.y.0 - at.y.0) as i128);
            dx * dx + dy * dy
        })
    };
    if prefs.grab_hot_spot {
        let hot = if sheet.unit(object).is_some() {
            nearest(
                (placed_pins(sheet).into_iter())
                    .filter(|(u, _, _)| u.uuid == object)
                    .map(|(_, _, p)| p)
                    .collect(),
            )
        } else if let Some(wire) = sheet.wire(object) {
            nearest(wire.vertices.clone())
        } else {
            anchor(sheet, object)
        };
        if hot.is_some() {
            return hot;
        }
    }
    if !prefs.grab_center {
        return None;
    }
    if let Some(unit) = sheet.unit(object) {
        return Some(unit.at);
    }
    if let Some(wire) = sheet.wire(object) {
        let [a, b] = wire.ends();
        return Some(Point::new(Nm((a.x.0 + b.x.0) / 2), Nm((a.y.0 + b.y.0) / 2)));
    }
    if let Some(text) = sheet.texts().find(|t| t.uuid == object) {
        return Some(text.at);
    }
    if let Some(graphic) = sheet.graphics().find(|g| g.uuid == object) {
        let (a, b) = graphic.shape.ends();
        return Some(Point::new(Nm((a.x.0 + b.x.0) / 2), Nm((a.y.0 + b.y.0) / 2)));
    }
    anchor(sheet, object)
}

/// The connection point of a Net Label, Port, Power Port or No-Connect Marker.
fn anchor(sheet: &SchematicSheet, object: Uuid) -> Option<Point> {
    (sheet.net_labels().find(|o| o.uuid == object).map(|o| o.at))
        .or_else(|| sheet.ports().find(|o| o.uuid == object).map(|o| o.at))
        .or_else(|| sheet.power_ports().find(|o| o.uuid == object).map(|o| o.at))
        .or_else(|| sheet.no_connects().find(|o| o.uuid == object).map(|o| o.at))
}

/// `Mod`+Space: a drag in flight on `document`'s canvas turns its rubber-banded Wires'
/// corners the other way. Returns whether one was in flight.
pub fn flip_corners(shell: &mut Shell, document: Uuid) -> bool {
    let Some(Interaction::Flight(f)) = shell.document_tab(document).map(|t| &t.interaction) else {
        return false;
    };
    if !f.keep_connections || f.committing {
        return false;
    }
    let flip = !f.flip;
    let preview = shell.engine().project().and_then(|p| {
        reda_engine::schematic::shift_preview_flipped(
            p,
            &f.objects,
            f.offset,
            f.keep_connections,
            flip,
        )
        .ok()
    });
    if let Some(Interaction::Flight(f)) =
        shell.document_tab_mut(document).map(|t| &mut t.interaction)
    {
        f.flip = flip;
        if let Some(preview) = preview {
            f.preview = preview;
        }
    }
    restate(shell, document);
    true
}

/// Which way `schematic.nudge` moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NudgeDirection {
    Left,
    Right,
    Up,
    Down,
}

/// `Mod`+arrows: drags `objects` by `by`, as one `schematic.drag`.
pub fn nudge(shell: &mut Shell, objects: Vec<Uuid>, by: Point) {
    if by == Point::default() {
        return;
    }
    let _ = shell.run_reporting(
        "schematic.drag",
        json!({"objects": objects, "by": {"x": by.x, "y": by.y}}),
    );
}

/// The point of `object` on `sheet` that a nudge puts on the grid: a Unit's origin, a
/// Wire's first vertex, a text's or label's anchor, a graphic's first end.
fn nudge_anchor(sheet: &SchematicSheet, object: Uuid) -> Option<Point> {
    (sheet.unit(object).map(|u| u.at))
        .or_else(|| sheet.wire(object).and_then(|w| w.vertices.first().copied()))
        .or_else(|| sheet.texts().find(|t| t.uuid == object).map(|t| t.at))
        .or_else(|| {
            (sheet.graphics())
                .find(|g| g.uuid == object)
                .map(|g| g.shape.ends().0)
        })
        .or_else(|| anchor(sheet, object))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NudgeParams {
    /// The objects to nudge, by UUID, all on one sheet.
    pub objects: Vec<Uuid>,
    pub direction: NudgeDirection,
    /// How many snap grids; 1 by default.
    #[serde(default = "one")]
    pub steps: i64,
}

fn one() -> i64 {
    1
}

/// What `schematic.nudge` asks of the Shell, which runs the drag.
#[derive(Serialize, JsonSchema)]
pub struct Nudged {
    pub objects: Vec<Uuid>,
    pub direction: NudgeDirection,
    pub steps: i64,
    /// How far the drag moves them (nm): `steps` snap grids, measured at the first
    /// object.
    pub by: Point,
}

pub(super) struct Nudge;

impl Command for Nudge {
    const META: Meta = Meta {
        id: "schematic.nudge",
        title: "Nudge",
        description: "Moves the selected objects a number of snap grids left, right, up or \
                      down, rubber-banding like a drag: one schematic.drag, so one undo step \
                      per press. Mod+arrows nudge by 1 grid, Shift+Mod+arrows by 10.",
        category: "schematic",
        icon: "drag",
        kind: Kind::View,
    };
    type Params = NudgeParams;
    type Output = Nudged;

    fn run(&self, cx: &mut Context<'_>, p: NudgeParams) -> Result<Nudged, CommandError> {
        if p.objects.is_empty() {
            return Err(CommandError::rejected("nothing is selected"));
        }
        if !(1..=100).contains(&p.steps) {
            return Err(CommandError::invalid_params(
                "/steps",
                "a nudge is 1 to 100 snap grids",
            ));
        }
        let document = crate::navigation::canvas_of(cx, None)?;
        let sheet = cx
            .project()?
            .schematic_sheet(document)
            .ok_or_else(|| CommandError::unavailable("the active canvas is not a sheet"))?;
        // With no object to measure at, the step is from the grid's origin: whole grids.
        let at = (p.objects.iter())
            .find_map(|o| nudge_anchor(sheet, *o))
            .unwrap_or_default();
        let (dx, dy) = match p.direction {
            NudgeDirection::Left => (-p.steps, 0),
            NudgeDirection::Right => (p.steps, 0),
            NudgeDirection::Up => (0, p.steps),
            NudgeDirection::Down => (0, -p.steps),
        };
        let (grid, _, _) = crate::edit_cursor::step_grid(cx, document, None, false)?;
        let by = grid.step(at, dx, dy, false) - at;
        Ok(Nudged {
            objects: p.objects,
            direction: p.direction,
            steps: p.steps,
            by,
        })
    }
}
