//! `schematic.move` (detached) and `schematic.drag` (Wires rubber-band).

use reda_command::{Command, CommandError, Context, Kind, Meta};
use std::collections::BTreeSet;

use reda_model::geom::Point;
use reda_model::{Object, Project, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::selection::{one_sheet, resolve};
use super::transform::{Placement, place_anew, plan};
use crate::component::At;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShiftParams {
    /// The objects to move, by UUID, all on one sheet: Units, Wires, Net Labels,
    /// No-Connect Markers, Ports, Power Ports, text and graphics.
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
                      Labels, No-Connect Markers, Ports and Power Ports on a moved Pin come \
                      along.",
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

/// What a move or drag in flight shows, before it is committed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShiftPreview {
    /// Objects drawn whole at the offset: the selection, and the Net Labels, No-Connect
    /// Markers, Ports and Power Ports that follow it. Sorted.
    pub moved: Vec<Uuid>,
    /// Unselected Wires a drag reshapes or folds away, drawn instead as [`wires`](Self::wires).
    /// Sorted.
    pub hidden: Vec<Uuid>,
    /// The rubber-banded Wires as they would be, corners included, as (from, to).
    pub wires: Vec<(Point, Point)>,
}

/// Works out what `schematic.drag` (with `keep_connections`) or `schematic.move` of
/// `objects` by `by` would change, without changing anything. The same errors as the
/// Commands.
pub fn shift_preview(
    project: &Project,
    objects: &[Uuid],
    by: Point,
    keep_connections: bool,
) -> Result<ShiftPreview, CommandError> {
    let objects = resolve(project, objects, "/objects")?;
    let document = one_sheet(&objects, "/objects")?;
    let body = crate::sheet(project, document)?;
    let how = Placement::Translate(by);
    let moves: Vec<_> = objects.into_iter().map(|o| (o, how)).collect();
    let plan = plan(body, &moves, keep_connections)?;
    let mut preview = ShiftPreview::default();
    let selected: BTreeSet<Uuid> = moves.iter().map(|(o, _)| o.uuid()).collect();
    for object in plan.replaced {
        match object {
            Object::Wire(w) if !selected.contains(&w.uuid) => {
                preview.hidden.push(w.uuid);
                preview.wires.push((w.from, w.to));
            }
            other => preview.moved.push(other.uuid()),
        }
    }
    for uuid in plan.removed {
        if !selected.contains(&uuid) {
            preview.hidden.push(uuid);
        }
    }
    preview
        .wires
        .extend(plan.inserted.iter().map(|w| (w.from, w.to)));
    preview.moved.sort();
    preview.moved.dedup();
    preview.hidden.sort();
    preview.hidden.dedup();
    Ok(preview)
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
