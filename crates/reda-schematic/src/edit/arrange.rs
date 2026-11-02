//! `schematic.align`, `schematic.distribute` and `schematic.snap_to_grid`.

use reda_command::{Command, CommandError, Context, Kind, Length, Meta};
use reda_model::geom::{Nm, Point};
use reda_model::{Object, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::bounds::{Bounds, bounds};
use super::moving::EditResult;
use super::selection::{one_sheet, resolve};
use super::transform::{Placement, place_anew};

/// The default snap grid: 50 mil.
const DEFAULT_GRID: Nm = Nm::from_mil(50);

/// What [`Align`] lines up. Up is +y.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlignTo {
    /// Left edges on the leftmost.
    Left,
    /// Right edges on the rightmost.
    Right,
    /// Top edges on the topmost.
    Top,
    /// Bottom edges on the bottommost.
    Bottom,
    /// Centres on one vertical line through the middle of the selection's extent.
    CenterX,
    /// Centres on one horizontal line through the middle of the selection's extent.
    CenterY,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignParams {
    /// The objects to align, by UUID, all on one sheet. Each one's extent is its Pins
    /// and body drawing (Units), its ends (Wires, graphics) or its point (labels,
    /// markers, text).
    pub objects: Vec<Uuid>,
    pub to: AlignTo,
    /// Keep connections as a drag does. Off by default (detached).
    #[serde(default)]
    pub keep_connections: bool,
}

pub struct Align;

impl Command for Align {
    const META: Meta = Meta {
        id: "schematic.align",
        title: "Align",
        description: "Aligns the selected objects' left, right, top or bottom edges, or \
                      their centres, moving each as a whole. Up is +y.",
        category: "schematic",
        icon: "align",
        kind: Kind::Edit,
    };
    type Params = AlignParams;
    type Output = EditResult;

    fn run(&self, cx: &mut Context<'_>, p: AlignParams) -> Result<EditResult, CommandError> {
        let to = p.to;
        arrange(cx, &p.objects, p.keep_connections, |all, each| {
            let zero = Nm::ZERO;
            let (dx, dy) = match to {
                AlignTo::Left => (all.min.x - each.min.x, zero),
                AlignTo::Right => (all.max.x - each.max.x, zero),
                AlignTo::Top => (zero, all.max.y - each.max.y),
                AlignTo::Bottom => (zero, all.min.y - each.min.y),
                AlignTo::CenterX => (all.center().x - each.center().x, zero),
                AlignTo::CenterY => (zero, all.center().y - each.center().y),
            };
            Point::new(dx, dy)
        })
    }
}

/// Which way [`Distribute`] spaces objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DistributeParams {
    /// The objects to space out, by UUID, all on one sheet.
    pub objects: Vec<Uuid>,
    pub direction: Direction,
    /// Keep connections as a drag does. Off by default (detached).
    #[serde(default)]
    pub keep_connections: bool,
}

pub struct Distribute;

impl Command for Distribute {
    const META: Meta = Meta {
        id: "schematic.distribute",
        title: "Distribute",
        description: "Spaces the selected objects' centres evenly, horizontally or \
                      vertically, between the two outermost, which stay put.",
        category: "schematic",
        icon: "distribute",
        kind: Kind::Edit,
    };
    type Params = DistributeParams;
    type Output = EditResult;

    fn run(&self, cx: &mut Context<'_>, p: DistributeParams) -> Result<EditResult, CommandError> {
        let horizontal = p.direction == Direction::Horizontal;
        let along = move |b: &Bounds| {
            let c = b.center();
            if horizontal { c.x.0 } else { c.y.0 }
        };
        arrange_each(cx, &p.objects, p.keep_connections, |boxes| {
            let mut order: Vec<usize> = (0..boxes.len()).collect();
            order.sort_by_key(|&i| along(&boxes[i]));
            let mut out = vec![Point::default(); boxes.len()];
            let n = boxes.len();
            if n < 3 {
                return out;
            }
            let first = i128::from(along(&boxes[order[0]]));
            let last = i128::from(along(&boxes[order[n - 1]]));
            for (k, &i) in order.iter().enumerate() {
                let span = (last - first) * k as i128;
                let want = first + span.div_euclid((n - 1) as i128);
                let d = Nm((want - i128::from(along(&boxes[i]))) as i64);
                out[i] = if horizontal {
                    Point::new(d, Nm::ZERO)
                } else {
                    Point::new(Nm::ZERO, d)
                };
            }
            out
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnapParams {
    /// The objects to snap, by UUID, all on one sheet.
    pub objects: Vec<Uuid>,
    /// The grid pitch; 50 mil by default.
    pub grid: Option<Length>,
    /// Keep connections as a drag does. Off by default (detached).
    #[serde(default)]
    pub keep_connections: bool,
}

pub struct SnapToGrid;

impl Command for SnapToGrid {
    const META: Meta = Meta {
        id: "schematic.snap_to_grid",
        title: "Snap to Grid",
        description: "Moves each selected object's anchor points to the nearest grid point: \
                      a Unit's origin, a Wire's or line's ends, a rectangle's corners, a \
                      label's, marker's or text's point. 50 mil by default.",
        category: "schematic",
        icon: "grid",
        kind: Kind::Edit,
    };
    type Params = SnapParams;
    type Output = EditResult;

    fn run(&self, cx: &mut Context<'_>, p: SnapParams) -> Result<EditResult, CommandError> {
        let grid = p.grid.map_or(DEFAULT_GRID, Length::nm);
        if grid <= Nm::ZERO {
            return Err(CommandError::invalid_params(
                "/grid",
                "the grid pitch must be positive",
            ));
        }
        let objects = resolve(cx.project()?, &p.objects, "/objects")?;
        let document = one_sheet(&objects, "/objects")?;
        let moves: Vec<_> = objects
            .into_iter()
            .map(|o| (o, Placement::Snap(grid)))
            .collect();
        place_anew(cx, document, &moves, p.keep_connections)?;
        Ok(EditResult { document })
    }
}

/// Moves each object by the offset `offset` gives from the union of every box and the
/// object's own box.
fn arrange(
    cx: &mut Context<'_>,
    uuids: &[Uuid],
    keep_connections: bool,
    offset: impl Fn(Bounds, Bounds) -> Point,
) -> Result<EditResult, CommandError> {
    arrange_each(cx, uuids, keep_connections, |boxes| {
        let all = boxes
            .iter()
            .copied()
            .reduce(Bounds::union)
            .expect("a selection is never empty");
        boxes.iter().map(|&b| offset(all, b)).collect()
    })
}

/// Moves each object by the offset `offsets` gives it from all their boxes, in order.
fn arrange_each(
    cx: &mut Context<'_>,
    uuids: &[Uuid],
    keep_connections: bool,
    offsets: impl FnOnce(&[Bounds]) -> Vec<Point>,
) -> Result<EditResult, CommandError> {
    let project = cx.project()?;
    let objects = resolve(project, uuids, "/objects")?;
    let document = one_sheet(&objects, "/objects")?;
    let body = crate::sheet(project, document)?;
    let boxes: Vec<Bounds> = objects.iter().map(|o| bounds(body, o)).collect();
    let offsets = offsets(&boxes);
    let moves: Vec<(Object, Placement)> = objects
        .into_iter()
        .zip(offsets)
        .map(|(o, by)| (o, Placement::Translate(by)))
        .collect();
    place_anew(cx, document, &moves, keep_connections)?;
    Ok(EditResult { document })
}
