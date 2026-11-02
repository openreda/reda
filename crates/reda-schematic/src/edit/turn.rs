//! `schematic.rotate` and `schematic.mirror`.

use reda_command::{Angle, Command, CommandError, Context, Kind, Meta};
use reda_model::geom::{Millideg, Point};
use reda_model::{Object, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::bounds::{anchor, union};
use super::moving::EditResult;
use super::selection::{one_sheet, resolve};
use super::transform::{Axis, Placement, place_anew};
use crate::component::At;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotateParams {
    /// The objects to turn, by UUID, all on one sheet.
    pub objects: Vec<Uuid>,
    /// Counter-clockwise; a quarter turn by default. Any angle turns Units, labels and
    /// text; Wires and rectangles turn only in quarter turns.
    pub angle: Option<Angle>,
    /// The point to turn about. By default a lone object's own origin (a Unit's origin, a
    /// label's or text's point), else the centre of the selection's extent.
    pub center: Option<At>,
    /// Keep connections as a drag does: attached Wires follow the moved Pins. Off by
    /// default (detached).
    #[serde(default)]
    pub keep_connections: bool,
}

pub struct Rotate;

impl Command for Rotate {
    const META: Meta = Meta {
        id: "schematic.rotate",
        title: "Rotate",
        description: "Turns the selected objects counter-clockwise about a centre, a quarter \
                      turn by default. Units, labels and text turn by any angle; Wires and \
                      rectangles only by quarter turns.",
        category: "schematic",
        icon: "rotate",
        kind: Kind::Edit,
    };
    type Params = RotateParams;
    type Output = EditResult;

    fn run(&self, cx: &mut Context<'_>, p: RotateParams) -> Result<EditResult, CommandError> {
        let angle = p.angle.map_or(Millideg::from_deg(90), Angle::millideg);
        turn(cx, &p.objects, p.center, p.keep_connections, |center| {
            Placement::Rotate { center, angle }
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MirrorParams {
    /// The objects to flip, by UUID, all on one sheet.
    pub objects: Vec<Uuid>,
    /// `x` flips left to right (mirror X), `y` top to bottom (mirror Y).
    pub axis: Axis,
    /// The point to flip about. By default a lone object's own origin, else the centre of
    /// the selection's extent.
    pub center: Option<At>,
    /// Keep connections as a drag does: attached Wires follow the moved Pins. Off by
    /// default (detached).
    #[serde(default)]
    pub keep_connections: bool,
}

pub struct Mirror;

impl Command for Mirror {
    const META: Meta = Meta {
        id: "schematic.mirror",
        title: "Mirror",
        description: "Flips the selected objects left to right (axis x) or top to bottom \
                      (axis y) about a centre.",
        category: "schematic",
        icon: "mirror",
        kind: Kind::Edit,
    };
    type Params = MirrorParams;
    type Output = EditResult;

    fn run(&self, cx: &mut Context<'_>, p: MirrorParams) -> Result<EditResult, CommandError> {
        let axis = p.axis;
        turn(cx, &p.objects, p.center, p.keep_connections, |center| {
            Placement::Mirror { center, axis }
        })
    }
}

fn turn(
    cx: &mut Context<'_>,
    uuids: &[Uuid],
    center: Option<At>,
    keep_connections: bool,
    how: impl Fn(Point) -> Placement,
) -> Result<EditResult, CommandError> {
    let project = cx.project()?;
    let objects = resolve(project, uuids, "/objects")?;
    let document = one_sheet(&objects, "/objects")?;
    let body = crate::sheet(project, document)?;
    let center = match center {
        Some(at) => at.point(),
        None => default_center(body, &objects),
    };
    let how = how(center);
    let moves: Vec<(Object, Placement)> = objects.into_iter().map(|o| (o, how)).collect();
    place_anew(cx, document, &moves, keep_connections)?;
    Ok(EditResult { document })
}

fn default_center(body: &reda_model::SchematicSheet, objects: &[Object]) -> Point {
    match objects {
        [one] => anchor(body, one),
        _ => union(body, objects).center(),
    }
}
