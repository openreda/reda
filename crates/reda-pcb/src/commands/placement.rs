//! Arranging what is placed on the board: Footprints, Free Pads, Dimensions, the Output
//! Origin, and the Board Outline and Board Cutouts (Move Board Shape).
//!
//! Every Command here resolves its `objects` to [`Item`]s, works out one [`Op`] per item
//! and writes the results back in one transaction. Locked Footprints refuse every Op.

pub(crate) mod arrange;
mod board;
mod designator;
mod turn;

use reda_command::{CommandError, Context, Registry};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::geom::{Vertex, contour_bounds};
use reda_model::{
    BoardCutout, BoardOutline, Corners, Dimension, DimensionTextPosition, FootprintInstance,
    FreePad, InDocument, Object, OutputOrigin, PcbDocument, Side, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::json;

use crate::placed::{Bounds, footprint_bounds, free_pad_bounds, rotate};

pub(super) fn register_all(registry: &mut Registry) {
    turn::register_all(registry);
    arrange::register_all(registry);
    designator::register_all(registry);
    board::register_all(registry);
}

/// What a placement Command returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Placed {
    /// The objects it moved, in the order given.
    pub objects: Vec<Uuid>,
}

/// One object that placement Commands move.
#[derive(Clone, Debug)]
pub(super) enum Item {
    Footprint(Box<FootprintInstance>),
    FreePad(Box<FreePad>),
    Origin(OutputOrigin),
    Dimension(Box<Dimension>),
    /// The Board Outline, moved alone by Move Board Shape.
    Outline(BoardOutline),
    Cutout(BoardCutout),
}

/// A change of position applied to placed objects: what `pcb.move`, `pcb.rotate` and
/// `pcb.flip` each do once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Translate(Point),
    /// Counter-clockwise about a point.
    Rotate(Point, Millideg),
    /// Mirror left-right about the vertical line at this x, swapping sides.
    Flip(Nm),
}

impl Item {
    fn uuid(&self) -> Uuid {
        match self {
            Item::Footprint(f) => f.uuid,
            Item::FreePad(p) => p.uuid,
            Item::Origin(o) => o.uuid,
            Item::Dimension(d) => d.uuid,
            Item::Outline(o) => o.uuid,
            Item::Cutout(c) => c.uuid,
        }
    }

    fn at(&self) -> Point {
        match self {
            Item::Footprint(f) => f.at,
            Item::FreePad(p) => p.at,
            Item::Origin(o) => o.at,
            Item::Dimension(d) => d.a.point,
            Item::Outline(_) | Item::Cutout(_) => self.contour_bounds().center(),
        }
    }

    /// A contour's box: the Board Outline's or a Board Cutout's.
    fn contour_bounds(&self) -> Bounds {
        let vertices = match self {
            Item::Outline(o) => &o.vertices,
            Item::Cutout(c) => &c.vertices,
            _ => unreachable!("only contours"),
        };
        contour_bounds(vertices).expect("a contour has vertices")
    }

    fn bounds(&self, pcb: &PcbDocument) -> Bounds {
        match self {
            Item::Footprint(f) => footprint_bounds(pcb, f),
            Item::FreePad(p) => free_pad_bounds(p),
            Item::Origin(o) => Bounds::of_point(o.at),
            Item::Dimension(d) => Bounds::of_point(d.a.point).union(Bounds::of_point(d.b.point)),
            Item::Outline(_) | Item::Cutout(_) => self.contour_bounds(),
        }
    }

    fn apply(&mut self, op: Op) {
        match self {
            Item::Dimension(d) => return move_dimension(d, op),
            Item::Outline(BoardOutline { vertices, .. })
            | Item::Cutout(BoardCutout { vertices, .. }) => return move_contour(vertices, op),
            _ => {}
        }
        let (at, rotation, side) = match self {
            Item::Footprint(f) => (&mut f.at, Some(&mut f.rotation), Some(&mut f.side)),
            Item::FreePad(p) => {
                if let Op::Flip(_) = op {
                    mirror_free_pad(p);
                }
                (&mut p.at, Some(&mut p.rotation), None)
            }
            Item::Origin(o) => (&mut o.at, None, None),
            Item::Dimension(_) | Item::Outline(_) | Item::Cutout(_) => {
                unreachable!("moved above")
            }
        };
        match op {
            Op::Translate(by) => *at = *at + by,
            Op::Rotate(about, angle) => {
                *at = rotate(*at, about, angle);
                if let Some(r) = rotation {
                    *r = (*r + angle).normalized();
                }
            }
            Op::Flip(x) => {
                at.x = x * 2 - at.x;
                if let Some(r) = rotation {
                    *r = (-*r).normalized();
                }
                if let Some(side) = side {
                    *side = match side {
                        Side::Top => Side::Bottom,
                        Side::Bottom => Side::Top,
                    };
                }
            }
        }
    }

    fn into_object(self, document: Uuid) -> Object {
        match self {
            Item::Footprint(f) => Object::FootprintInstance(InDocument::new(document, *f)),
            Item::FreePad(p) => Object::FreePad(InDocument::new(document, *p)),
            Item::Origin(o) => Object::OutputOrigin(InDocument::new(document, o)),
            Item::Dimension(d) => Object::Dimension(InDocument::new(document, *d)),
            Item::Outline(o) => Object::BoardOutline(InDocument::new(document, o)),
            Item::Cutout(c) => Object::BoardCutout(InDocument::new(document, c)),
        }
    }
}

/// Moves, turns or mirrors a closed contour. Mirroring reverses each arc's direction.
fn move_contour(vertices: &mut [Vertex], op: Op) {
    for v in vertices {
        let p = match op {
            Op::Translate(by) => v.point() + by,
            Op::Rotate(about, angle) => rotate(v.point(), about, angle),
            Op::Flip(x) => {
                v.sweep = -v.sweep;
                Point::new(x * 2 - v.x, v.y)
            }
        };
        (v.x, v.y) = (p.x, p.y);
    }
}

/// Moves a Dimension's free ends and manual text by `op`, and turns or mirrors its
/// direction. Ends that follow a Hotspot are left to follow it.
fn move_dimension(d: &mut Dimension, op: Op) {
    let point = |p: Point| match op {
        Op::Translate(by) => p + by,
        Op::Rotate(about, angle) => rotate(p, about, angle),
        Op::Flip(x) => Point::new(x * 2 - p.x, p.y),
    };
    for end in [&mut d.a, &mut d.b] {
        if end.reference.is_none() {
            end.point = point(end.point);
        }
    }
    if let DimensionTextPosition::Manual(at) = &mut d.text_position {
        *at = point(*at);
    }
    match op {
        Op::Translate(_) => {}
        Op::Rotate(_, angle) => d.rotation = (d.rotation + angle).normalized(),
        Op::Flip(_) => {
            // The direction mirrors left-right, so the side the line sits on swaps.
            d.rotation = (Millideg::from_deg(180) - d.rotation).normalized();
            d.offset = -d.offset;
        }
    }
}

/// Mirrors a Free Pad's own frame left-right, as flipping it does: its corners, offset
/// and slot turn over, and an SMD Pad moves to the other side. The caller mirrors its
/// position and rotation.
fn mirror_free_pad(pad: &mut FreePad) {
    pad.corners = pad.corners.map(Corners::mirrored);
    pad.offset = pad.offset.map(|o| Point::new(-o.x, o.y));
    if let Some(hole) = &mut pad.hole {
        hole.rotation = Millideg((-hole.rotation.0).rem_euclid(180_000));
    }
    if let Some(side) = pad.form().copper_side() {
        pad.side = match side {
            Side::Top => Some(Side::Bottom),
            Side::Bottom => None,
        };
    }
}

/// The placeable objects named by `uuids` (each once, in order), or `invalid_params` at
/// `/objects/i` for one that is not a Footprint, Free Pad or the Output Origin.
pub(super) fn resolve(pcb: &PcbDocument, uuids: &[Uuid]) -> Result<Vec<Item>, CommandError> {
    if uuids.is_empty() {
        return Err(CommandError::invalid_params(
            "/objects",
            "nothing is selected",
        ));
    }
    let mut items: Vec<Item> = Vec::with_capacity(uuids.len());
    for (i, &uuid) in uuids.iter().enumerate() {
        if items.iter().any(|it| it.uuid() == uuid) {
            continue;
        }
        let item = if let Some(f) = pcb.footprint(uuid) {
            Item::Footprint(Box::new(f.clone()))
        } else if let Some(p) = pcb.free_pad(uuid) {
            Item::FreePad(Box::new(p.clone()))
        } else if let Some(o) = pcb.output_origin().filter(|o| o.uuid == uuid) {
            Item::Origin(o.clone())
        } else if let Some(d) = pcb.dimension(uuid) {
            Item::Dimension(Box::new(d.clone()))
        } else if let Some(o) = pcb.outline().filter(|o| o.uuid == uuid) {
            Item::Outline(o.clone())
        } else if let Some(c) = pcb.cutout(uuid) {
            Item::Cutout(c.clone())
        } else {
            return Err(CommandError::invalid_params(
                &format!("/objects/{i}"),
                format!(
                    "{uuid} is not a Footprint, Free Pad, Dimension, Output Origin, Board \
                     Outline or Board Cutout on the board"
                ),
            ));
        };
        items.push(item);
    }
    refuse_lone_dimensions(&items)?;
    Ok(items)
}

/// `rejected` when a Dimension would move without an object it references: it can only
/// move together with them.
fn refuse_lone_dimensions(items: &[Item]) -> Result<(), CommandError> {
    for item in items {
        let Item::Dimension(d) = item else {
            continue;
        };
        let left: Vec<Uuid> = d
            .references()
            .into_iter()
            .filter(|r| !items.iter().any(|i| i.uuid() == *r))
            .collect();
        if !left.is_empty() {
            let mut err = CommandError::rejected(
                "a Dimension with a reference can't move alone; select it with the \
                 objects it references to move them together",
            );
            err.details = Some(json!({ "dimension": d.uuid, "references": left }));
            return Err(err);
        }
    }
    Ok(())
}

/// `rejected` when any of `items` is a locked Footprint.
fn refuse_locked(items: &[Item]) -> Result<(), CommandError> {
    let locked: Vec<&FootprintInstance> = items
        .iter()
        .filter_map(|i| match i {
            Item::Footprint(f) if f.locked => Some(&**f),
            _ => None,
        })
        .collect();
    if locked.is_empty() {
        return Ok(());
    }
    let names: Vec<&str> = locked.iter().map(|f| f.designator.as_str()).collect();
    let mut err = CommandError::rejected(format!(
        "{} {} locked; unlock to move",
        names.join(", "),
        if locked.len() == 1 { "is" } else { "are" }
    ));
    err.details = Some(json!({ "locked": locked.iter().map(|f| f.uuid).collect::<Vec<_>>() }));
    Err(err)
}

/// The union of every item's box.
fn extent(pcb: &PcbDocument, items: &[Item]) -> Bounds {
    items
        .iter()
        .map(|i| i.bounds(pcb))
        .reduce(Bounds::union)
        .expect("a selection is never empty")
}

/// The point a rotate or flip turns about when the params leave it out: a lone item's
/// own position, else the middle of the selection's extent.
fn default_center(pcb: &PcbDocument, items: &[Item]) -> Point {
    match items {
        [one] => one.at(),
        _ => extent(pcb, items).center(),
    }
}

/// Where `objects` turn and flip about when no centre is given: a lone object's own
/// position, else the middle of their extent.
pub fn anchor(pcb: &PcbDocument, objects: &[Uuid]) -> Result<Point, CommandError> {
    Ok(default_center(pcb, &resolve(pcb, objects)?))
}

/// `objects` as they would be after `ops`, one after another, without changing the board:
/// for showing a move in flight. Refused as the Commands refuse (locked Footprints,
/// objects that cannot be placed).
pub fn preview(
    pcb: &PcbDocument,
    objects: &[Uuid],
    ops: &[Op],
) -> Result<Vec<Object>, CommandError> {
    let mut items = resolve(pcb, objects)?;
    refuse_locked(&items)?;
    for item in &mut items {
        for op in ops {
            item.apply(*op);
        }
    }
    Ok(items.into_iter().map(|i| i.into_object(pcb.uuid)).collect())
}

/// Applies one Op per item (`ops` in the same order), refusing locked Footprints first,
/// and writes the items back.
fn place(
    cx: &mut Context<'_>,
    mut items: Vec<Item>,
    ops: impl IntoIterator<Item = Op>,
) -> Result<Placed, CommandError> {
    refuse_locked(&items)?;
    let document = super::pcb(cx)?.uuid;
    for (item, op) in items.iter_mut().zip(ops) {
        item.apply(op);
    }
    let objects = items.iter().map(Item::uuid).collect();
    let tx = cx.tx()?;
    for item in items {
        tx.replace(item.into_object(document))?;
    }
    Ok(Placed { objects })
}
