//! Moving a selection on its sheet: the geometry every move-like Command shares (move,
//! drag, rotate, mirror, align, distribute and snap), with or without rubber-banding the
//! Wires attached to it.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{CommandError, Context};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{InDocument, Object, SchematicSheet, SheetShape, UnitInstance, Uuid, Wire};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::connectivity::{pin_position, rotate};
use crate::wiring::split_wires_at_pins;

/// Which way a mirror flips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// Left to right: x coordinates flip about the centre.
    X,
    /// Top to bottom: y coordinates flip about the centre.
    Y,
}

/// What happens to one selected object.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Placement {
    /// Moved by this offset.
    Translate(Point),
    /// Turned counter-clockwise about a centre.
    Rotate { center: Point, angle: Millideg },
    /// Flipped about a centre.
    Mirror { center: Point, axis: Axis },
    /// Each of its anchor points moved to the nearest grid point (a Unit's origin, a
    /// Wire's vertices, a line's ends, a label's or Port's point).
    Snap(Nm),
}

impl Placement {
    fn point(self, p: Point) -> Point {
        match self {
            Placement::Translate(by) => p + by,
            Placement::Rotate { center, angle } => {
                let d = p - center;
                let (x, y) = rotate(d.x.0, d.y.0, angle);
                center + Point::new(Nm(x), Nm(y))
            }
            Placement::Mirror { center, axis } => match axis {
                Axis::X => Point::new(center.x * 2 - p.x, p.y),
                Axis::Y => Point::new(p.x, center.y * 2 - p.y),
            },
            Placement::Snap(grid) => Point::new(snap(p.x, grid), snap(p.y, grid)),
        }
    }

    /// Whether horizontal and vertical lines stay horizontal or vertical.
    fn keeps_axes(self) -> bool {
        match self {
            Placement::Rotate { angle, .. } => angle.normalized().0 % 90_000 == 0,
            _ => true,
        }
    }

    /// The new direction of something drawn at `rotation` with no mirror flag of its own
    /// (a label, a Port, a Power Port or a text).
    fn direction(self, rotation: Millideg) -> Millideg {
        match self {
            Placement::Rotate { angle, .. } => (rotation + angle).normalized(),
            Placement::Mirror { axis: Axis::X, .. } => {
                (Millideg::from_deg(180) - rotation).normalized()
            }
            Placement::Mirror { axis: Axis::Y, .. } => (-rotation).normalized(),
            Placement::Translate(_) | Placement::Snap(_) => rotation,
        }
    }

    /// `unit` placed anew: its origin moved, and its rotation and mirror flag composed
    /// with this placement.
    fn unit(self, unit: &UnitInstance) -> UnitInstance {
        let mut out = unit.clone();
        out.at = self.point(unit.at);
        match self {
            Placement::Rotate { angle, .. } => out.rotation = (unit.rotation + angle).normalized(),
            // Flipping the drawing composes with its own mirror (applied before rotating):
            // left-right gives mirror' = !mirror, rotation' = -rotation; top-bottom is that
            // turned by a half turn.
            Placement::Mirror { axis, .. } => {
                out.mirror = !unit.mirror;
                out.rotation = match axis {
                    Axis::X => (-unit.rotation).normalized(),
                    Axis::Y => (Millideg::from_deg(180) - unit.rotation).normalized(),
                };
            }
            Placement::Translate(_) | Placement::Snap(_) => {}
        }
        out
    }
}

/// `v` rounded to the nearest multiple of `grid`, halves away from zero.
fn snap(v: Nm, grid: Nm) -> Nm {
    let (v, g) = (v.0, grid.0);
    let (sign, v) = (v.signum(), v.abs());
    let r = v % g;
    let down = v - r;
    Nm(sign * if r * 2 >= g { down + g } else { down })
}

/// Places every selected object anew on `document`. With `keep_connections`, the Wire ends,
/// Net Labels, No-Connect Markers, Ports and Power Ports of the rest of the sheet that sat on a moved
/// connection point follow it, bending a Wire with a corner where it would go diagonal
/// (drag); without, they stay put (a detached move). Either way, moved Pins that land
/// partway along a Wire split it and connect.
pub(crate) fn place_anew(
    cx: &mut Context<'_>,
    document: Uuid,
    moves: &[(Object, Placement)],
    keep_connections: bool,
) -> Result<(), CommandError> {
    let body = &crate::sheet(cx.project()?, document)?.clone();
    let plan = plan(body, moves, keep_connections)?;
    let tx = cx.tx()?;
    for uuid in plan.removed {
        tx.remove(uuid)?;
    }
    for object in plan.replaced {
        tx.replace(object)?;
    }
    for unit in plan.units {
        split_wires_at_pins(cx, document, unit)?;
    }
    Ok(())
}

/// What placing a selection anew changes on its sheet, worked out without changing it.
pub(crate) struct Plan {
    /// Objects with their new contents: the selection and whatever follows it.
    pub replaced: Vec<Object>,
    /// Objects that fold away to nothing (Wires and lines of no length).
    pub removed: Vec<Uuid>,
    /// The moved Units, whose Pins may now split Wires.
    pub units: Vec<Uuid>,
}

/// Plans placing every selected object of `body` anew, as [`place_anew`] does.
pub(crate) fn plan(
    body: &SchematicSheet,
    moves: &[(Object, Placement)],
    keep_connections: bool,
) -> Result<Plan, CommandError> {
    let document = body.uuid;
    let mut followed: BTreeMap<Point, Point> = BTreeMap::new();
    let mut follow = |from: Point, to: Point| {
        followed.entry(from).or_insert(to);
    };
    let mut replaced = Vec::with_capacity(moves.len());
    let mut removed = vec![];
    let mut units = vec![];
    for (object, how) in moves {
        let how = *how;
        match object {
            Object::Unit(u) => {
                let new = how.unit(u);
                for (old, now) in pins(body, u).into_iter().zip(pins(body, &new)) {
                    follow(old, now);
                }
                units.push(u.uuid);
                replaced.push(Object::Unit(InDocument::new(document, new)));
            }
            Object::Wire(w) => {
                let vertices: Vec<Point> = w.vertices.iter().map(|&v| how.point(v)).collect();
                for (&old, &now) in w.vertices.iter().zip(&vertices) {
                    follow(old, now);
                }
                match Wire::through(w.uuid, vertices) {
                    Some(new) => replaced.push(Object::Wire(InDocument::new(document, new))),
                    None => removed.push(w.uuid),
                }
            }
            Object::NetLabel(l) => {
                let mut new = l.object.clone();
                new.at = how.point(l.at);
                new.rotation = how.direction(l.rotation);
                follow(l.at, new.at);
                replaced.push(Object::NetLabel(InDocument::new(document, new)));
            }
            Object::NoConnect(m) => {
                let mut new = m.object.clone();
                new.at = how.point(m.at);
                follow(m.at, new.at);
                replaced.push(Object::NoConnect(InDocument::new(document, new)));
            }
            Object::Port(p) => {
                let mut new = p.object.clone();
                new.at = how.point(p.at);
                new.rotation = how.direction(p.rotation);
                follow(p.at, new.at);
                replaced.push(Object::Port(InDocument::new(document, new)));
            }
            Object::PowerPort(p) => {
                let mut new = p.object.clone();
                new.at = how.point(p.at);
                new.rotation = how.direction(p.rotation);
                follow(p.at, new.at);
                replaced.push(Object::PowerPort(InDocument::new(document, new)));
            }
            Object::SheetText(t) => {
                let mut new = t.object.clone();
                new.at = how.point(t.at);
                new.rotation = how.direction(t.rotation);
                replaced.push(Object::SheetText(InDocument::new(document, new)));
            }
            Object::SheetGraphic(g) => {
                if matches!(g.shape, SheetShape::Rect { .. }) && !how.keeps_axes() {
                    return Err(CommandError::rejected(format!(
                        "rectangle {} would no longer be axis-aligned; rectangles turn only \
                         in quarter turns",
                        g.uuid
                    )));
                }
                let (a, b) = g.shape.ends();
                let mut new = g.object.clone();
                new.shape = g.shape.with_ends(how.point(a), how.point(b));
                if new.shape.is_empty() {
                    removed.push(g.uuid);
                } else {
                    replaced.push(Object::SheetGraphic(InDocument::new(document, new)));
                }
            }
            other => {
                return Err(CommandError::internal(format!(
                    "cannot move {:?}",
                    other.object_type()
                )));
            }
        }
    }
    followed.retain(|from, to| from != to);
    let selected: BTreeSet<Uuid> = moves.iter().map(|(o, _)| o.uuid()).collect();
    if keep_connections && !followed.is_empty() {
        rubber_band(body, &selected, &followed, &mut replaced, &mut removed);
    }
    Ok(Plan {
        replaced,
        removed,
        units,
    })
}

/// The connection points of placed Unit `unit` on `body`, in Pin order.
fn pins(body: &SchematicSheet, unit: &UnitInstance) -> Vec<Point> {
    let entry = body.part(unit.part).expect("a Unit's part entry exists");
    entry
        .symbol
        .unit_pins(unit.unit)
        .map(|pin| pin_position(unit, pin))
        .collect()
}

/// The vertices of unselected `wire` once its ends follow the moved connection points
/// `followed`, or `None` when neither end moved. An end segment that was horizontal or
/// vertical keeps its direction from the vertex that stayed and turns a corner to the moved
/// end, so the Wire stays orthogonal; one at another angle just goes straight to it.
fn rubber_banded(wire: &Wire, followed: &BTreeMap<Point, Point>) -> Option<Vec<Point>> {
    let [first, last] = wire.ends();
    let (to_first, to_last) = (followed.get(&first), followed.get(&last));
    if to_first.is_none() && to_last.is_none() {
        return None;
    }
    let original = &wire.vertices;
    let n = original.len();
    let mut vertices = original.clone();
    // A straight Wire with both ends moved bends at its far end, as if only that one moved
    // after the first.
    if let Some(&to) = to_first {
        vertices[0] = to;
        if n > 2 || to_last.is_none() {
            bend(&mut vertices, 0, 1, (original[0], original[1]));
        }
    }
    if let Some(&to) = to_last {
        let n = vertices.len();
        vertices[n - 1] = to;
        let next = n - 2;
        let o = original.len();
        bend(
            &mut vertices,
            n - 1,
            next,
            (original[o - 1], original[o - 2]),
        );
    }
    Some(vertices)
}

/// Turns a corner between the moved end `vertices[end]` and its neighbour `vertices[next]`
/// when the segment between them was orthogonal (`was`, as it was drawn) and no longer is.
fn bend(vertices: &mut Vec<Point>, end: usize, next: usize, was: (Point, Point)) {
    let (keep, go) = (vertices[next], vertices[end]);
    let horizontal = was.0.y == was.1.y && was.0.x != was.1.x;
    let vertical = was.0.x == was.1.x && was.0.y != was.1.y;
    if !(horizontal || vertical) || keep.x == go.x || keep.y == go.y {
        return;
    }
    let corner = if horizontal {
        Point::new(go.x, keep.y)
    } else {
        Point::new(keep.x, go.y)
    };
    vertices.insert(end.max(next), corner);
}

/// Makes the unselected objects on `body` follow the moved connection points `followed`.
fn rubber_band(
    body: &SchematicSheet,
    selected: &BTreeSet<Uuid>,
    followed: &BTreeMap<Point, Point>,
    replaced: &mut Vec<Object>,
    removed: &mut Vec<Uuid>,
) {
    let document = body.uuid;
    for wire in body.wires().filter(|w| !selected.contains(&w.uuid)) {
        let Some(vertices) = rubber_banded(wire, followed) else {
            continue;
        };
        match Wire::through(wire.uuid, vertices) {
            Some(new) => replaced.push(Object::Wire(InDocument::new(document, new))),
            None => removed.push(wire.uuid),
        }
    }
    for label in body.net_labels().filter(|l| !selected.contains(&l.uuid)) {
        if let Some(&at) = followed.get(&label.at) {
            let mut new = label.clone();
            new.at = at;
            replaced.push(Object::NetLabel(InDocument::new(document, new)));
        }
    }
    for marker in body.no_connects().filter(|m| !selected.contains(&m.uuid)) {
        if let Some(&at) = followed.get(&marker.at) {
            let mut new = marker.clone();
            new.at = at;
            replaced.push(Object::NoConnect(InDocument::new(document, new)));
        }
    }
    for port in body.ports().filter(|p| !selected.contains(&p.uuid)) {
        if let Some(&at) = followed.get(&port.at) {
            let mut new = port.clone();
            new.at = at;
            replaced.push(Object::Port(InDocument::new(document, new)));
        }
    }
    for port in body.power_ports().filter(|p| !selected.contains(&p.uuid)) {
        if let Some(&at) = followed.get(&port.at) {
            let mut new = port.clone();
            new.at = at;
            replaced.push(Object::PowerPort(InDocument::new(document, new)));
        }
    }
}
