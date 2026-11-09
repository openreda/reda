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
//!   Command `schematic.nudge`).

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

/// `Mod`+arrows: drags `objects` on `document` by `steps` snap grids, as one
/// `schematic.drag`.
pub fn nudge(
    shell: &mut Shell,
    document: Uuid,
    objects: Vec<Uuid>,
    direction: NudgeDirection,
    steps: i64,
) {
    let grid = shell.grid(document).0 * steps;
    let (x, y) = match direction {
        NudgeDirection::Left => (-grid, 0),
        NudgeDirection::Right => (grid, 0),
        NudgeDirection::Up => (0, grid),
        NudgeDirection::Down => (0, -grid),
    };
    let _ = shell.run_reporting(
        "schematic.drag",
        json!({"objects": objects, "by": {"x": x, "y": y}}),
    );
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

    fn run(&self, _cx: &mut Context<'_>, p: NudgeParams) -> Result<Nudged, CommandError> {
        if p.objects.is_empty() {
            return Err(CommandError::rejected("nothing is selected"));
        }
        if !(1..=100).contains(&p.steps) {
            return Err(CommandError::invalid_params(
                "/steps",
                "a nudge is 1 to 100 snap grids",
            ));
        }
        Ok(Nudged {
            objects: p.objects,
            direction: p.direction,
            steps: p.steps,
        })
    }
}
