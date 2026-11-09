//! `schematic.delete`: removing a selection from its sheets.

use std::collections::BTreeSet;

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::geom::Point;
use reda_model::{InDocument, Object, Uuid, Wire};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::selection::{prune_part_table, resolve, sheet_of};
use crate::normalise::{Wiring, normalise};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteParams {
    /// The objects to delete, by UUID: Units, Wires, Net Labels, No-Connect Markers,
    /// Ports, Power Ports, text and graphics, on any sheets.
    pub objects: Vec<Uuid>,
    /// Single Wire segments to delete, from a segment selection. A deletion stops at a
    /// Junction on the segment: only the stretch between the Junctions either side of its
    /// midpoint goes. The Wire splits around the gap, and its first piece keeps the UUID.
    #[serde(default)]
    pub segments: Vec<WireSegment>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// One segment of a Wire: from vertex `index` to the next.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct WireSegment {
    /// The Wire, by UUID.
    pub wire: Uuid,
    /// The segment's first vertex, from 0.
    pub index: usize,
}

/// What `schematic.delete` removed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteResult {
    /// The objects deleted, including the part-table entries and Footprint copies that no
    /// Unit uses any more.
    pub deleted: Vec<Uuid>,
}

pub struct Delete;

impl Command for Delete {
    const META: Meta = Meta {
        id: "schematic.delete",
        title: "Delete",
        description: "Deletes the selected objects from their sheets. Part-table entries no \
                      remaining Unit uses, and Footprint copies no remaining entry lists, \
                      go with them.",
        category: "schematic",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = DeleteParams;
    type Output = DeleteResult;

    fn run(&self, cx: &mut Context<'_>, p: DeleteParams) -> Result<DeleteResult, CommandError> {
        let wiring = p.wiring.unwrap_or_default();
        if p.segments.is_empty() || !p.objects.is_empty() {
            let objects = resolve(cx.project()?, &p.objects, "/objects")?;
            if p.segments.is_empty() {
                return Ok(DeleteResult {
                    deleted: delete_objects(cx, &objects, &wiring)?,
                });
            }
        }
        // Segments of Wires deleted whole go with their Wire.
        let segments: Vec<WireSegment> = (p.segments.iter().copied())
            .filter(|s| !p.objects.contains(&s.wire))
            .collect();
        let mut deleted = delete_segments(cx, &segments, &wiring)?;
        if !p.objects.is_empty() {
            let objects = resolve(cx.project()?, &p.objects, "/objects")?;
            deleted.extend(delete_objects(cx, &objects, &wiring)?);
        }
        Ok(DeleteResult { deleted })
    }
}

/// Cuts `segments` out of their Wires and normalises each sheet. Returns the Wires left
/// with nothing, and those merged away.
fn delete_segments(
    cx: &mut Context<'_>,
    segments: &[WireSegment],
    wiring: &Wiring,
) -> Result<Vec<Uuid>, CommandError> {
    let mut by_wire: std::collections::BTreeMap<Uuid, BTreeSet<usize>> = Default::default();
    for (i, s) in segments.iter().enumerate() {
        let pointer = format!("/segments/{i}");
        let Some(Object::Wire(wire)) = cx.project()?.object(s.wire) else {
            return Err(CommandError::invalid_params(
                &pointer,
                format!("{} is not a Wire", s.wire),
            ));
        };
        if s.index + 1 >= wire.object.vertices.len() {
            return Err(CommandError::invalid_params(
                &pointer,
                format!("Wire {} has no segment {}", s.wire, s.index),
            ));
        }
        by_wire.entry(s.wire).or_default().insert(s.index);
    }
    let mut sheets: std::collections::BTreeMap<Uuid, reda_model::SchematicSheet> =
        Default::default();
    let mut deleted = Vec::new();
    for (uuid, indices) in by_wire {
        let Some(Object::Wire(wire)) = cx.project()?.object(uuid) else {
            unreachable!("checked above")
        };
        let document = wire.document;
        if !sheets.contains_key(&document) {
            let sheet = crate::sheet(cx.project()?, document)?.clone();
            sheets.insert(document, sheet);
        }
        let junctions = crate::connectivity::derive(&sheets[&document]).junctions;
        let mut pieces = (without_segments(&wire.object, &indices, &junctions).into_iter())
            .filter_map(|v| Wire::through(uuid, v));
        match pieces.next() {
            None => {
                cx.tx()?.remove(uuid)?;
                deleted.push(uuid);
            }
            Some(first) => {
                cx.tx()?
                    .replace(Object::Wire(InDocument::new(document, first)))?;
                for piece in pieces {
                    let uuid = cx.new_id();
                    cx.tx()?.insert(Object::Wire(InDocument::new(
                        document,
                        Wire {
                            uuid,
                            vertices: piece.vertices,
                        },
                    )))?;
                }
            }
        }
    }
    for (document, sheet) in &sheets {
        normalise(cx, *document, sheet, wiring)?;
        let left = crate::sheet(cx.project()?, *document)?;
        let merged: Vec<Uuid> = (sheet.wires())
            .map(|w| w.uuid)
            .filter(|u| left.wire(*u).is_none() && !deleted.contains(u))
            .collect();
        deleted.extend(merged);
    }
    Ok(deleted)
}

/// The pieces of `wire` left once its segments `indices` are cut out, each only between
/// the `junctions` either side of its midpoint (its vertices when there are none).
fn without_segments(
    wire: &Wire,
    indices: &BTreeSet<usize>,
    junctions: &[Point],
) -> Vec<Vec<Point>> {
    let v = &wire.vertices;
    let mut pieces = Vec::new();
    let mut piece = vec![v[0]];
    for i in 0..v.len() - 1 {
        let (a, b) = (v[i], v[i + 1]);
        if !indices.contains(&i) {
            piece.push(b);
            continue;
        }
        let (from, to) = stretch(a, b, junctions);
        if from != a {
            piece.push(from);
        }
        pieces.push(std::mem::replace(&mut piece, vec![to]));
        if to != b {
            piece.push(b);
        }
    }
    pieces.push(piece);
    pieces
}

/// The stretch of the segment from `a` to `b` between the Junctions either side of its
/// midpoint: `(a, b)` when none lies inside it.
fn stretch(a: Point, b: Point, junctions: &[Point]) -> (Point, Point) {
    let distance = |p: Point| {
        let (dx, dy) = ((p.x - a.x).0 as f64, (p.y - a.y).0 as f64);
        dx.hypot(dy)
    };
    let middle = distance(b) / 2.0;
    let inside = (junctions.iter().copied()).filter(|&j| j != a && j != b && j.on_segment(a, b));
    let mut from = (a, 0.0);
    let mut to = (b, distance(b));
    for j in inside {
        let d = distance(j);
        if d <= middle && d > from.1 {
            from = (j, d);
        } else if d > middle && d < to.1 {
            to = (j, d);
        }
    }
    (from.0, to.0)
}

/// Removes `objects`, prunes the part tables of the sheets they were on and normalises the
/// Wires left touching them. Returns every UUID removed, Wires merged away included.
pub(crate) fn delete_objects(
    cx: &mut Context<'_>,
    objects: &[reda_model::Object],
    wiring: &Wiring,
) -> Result<Vec<Uuid>, CommandError> {
    let mut deleted = Vec::with_capacity(objects.len());
    let sheets: BTreeSet<Uuid> = objects.iter().map(sheet_of).collect();
    let mut before = Vec::with_capacity(sheets.len());
    for &sheet in &sheets {
        before.push(crate::sheet(cx.project()?, sheet)?.clone());
    }
    for object in objects {
        cx.tx()?.remove(object.uuid())?;
        deleted.push(object.uuid());
    }
    for sheet in &before {
        deleted.extend(prune_part_table(cx, sheet.uuid)?);
        normalise(cx, sheet.uuid, sheet, wiring)?;
        let left = crate::sheet(cx.project()?, sheet.uuid)?;
        let merged: Vec<Uuid> = (sheet.wires())
            .map(|w| w.uuid)
            .filter(|u| left.wire(*u).is_none() && !deleted.contains(u))
            .collect();
        deleted.extend(merged);
    }
    Ok(deleted)
}
