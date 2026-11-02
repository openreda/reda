//! `schematic.move` (detached) and `schematic.drag` (Wires rubber-band).

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::selection::{one_sheet, resolve};
use super::transform::{Placement, place_anew};
use crate::component::At;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShiftParams {
    /// The objects to move, by UUID, all on one sheet: Units, Wires, Net Labels,
    /// No-Connect Markers, text and graphics.
    pub objects: Vec<Uuid>,
    /// How far to move them.
    pub by: At,
}

/// The sheet an editing Command changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EditResult {
    pub document: Uuid,
}

pub struct Move;

impl Command for Move {
    const META: Meta = Meta {
        id: "schematic.move",
        title: "Move",
        description: "Moves the selected objects by an offset, detached: the Wires attached \
                      to them stay where they are. Pins that land partway along a Wire \
                      split it and connect, as when placing.",
        category: "schematic",
        icon: "move",
        kind: Kind::Edit,
    };
    type Params = ShiftParams;
    type Output = EditResult;

    fn run(&self, cx: &mut Context<'_>, p: ShiftParams) -> Result<EditResult, CommandError> {
        shift(cx, p, false)
    }
}

pub struct Drag;

impl Command for Drag {
    const META: Meta = Meta {
        id: "schematic.drag",
        title: "Drag",
        description: "Moves the selected objects by an offset and keeps their connections: \
                      the ends of unselected Wires on a moved Pin, Wire end or Net Label \
                      follow it, turning a corner where a Wire would go diagonal, and Net \
                      Labels and No-Connect Markers on a moved Pin come along.",
        category: "schematic",
        icon: "drag",
        kind: Kind::Edit,
    };
    type Params = ShiftParams;
    type Output = EditResult;

    fn run(&self, cx: &mut Context<'_>, p: ShiftParams) -> Result<EditResult, CommandError> {
        shift(cx, p, true)
    }
}

fn shift(
    cx: &mut Context<'_>,
    p: ShiftParams,
    keep_connections: bool,
) -> Result<EditResult, CommandError> {
    let objects = resolve(cx.project()?, &p.objects, "/objects")?;
    let document = one_sheet(&objects, "/objects")?;
    let how = Placement::Translate(p.by.point());
    let moves: Vec<_> = objects.into_iter().map(|o| (o, how)).collect();
    place_anew(cx, document, &moves, keep_connections)?;
    Ok(EditResult { document })
}
