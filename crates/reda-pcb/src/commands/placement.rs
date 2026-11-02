//! Arranging what is placed on the board: Footprints, Free Pads and the Placement Origin.
//!
//! Every Command here resolves its `objects` to [`Item`]s, works out one [`Op`] per item
//! and writes the results back in one transaction. Locked Footprints refuse every Op.

mod arrange;
mod designator;
mod turn;

use reda_command::{CommandError, Context, Registry};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    FootprintInstance, FreePad, InDocument, Object, PcbDocument, PlacementOrigin, Side, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::json;

use crate::placed::{Bounds, footprint_bounds, free_pad_bounds, rotate};

pub(super) fn register_all(registry: &mut Registry) {
    turn::register_all(registry);
    arrange::register_all(registry);
    designator::register_all(registry);
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
    FreePad(FreePad),
    Origin(PlacementOrigin),
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
        }
    }

    fn at(&self) -> Point {
        match self {
            Item::Footprint(f) => f.at,
            Item::FreePad(p) => p.at,
            Item::Origin(o) => o.at,
        }
    }

    fn bounds(&self, pcb: &PcbDocument) -> Bounds {
        match self {
            Item::Footprint(f) => footprint_bounds(pcb, f),
            Item::FreePad(p) => free_pad_bounds(p),
            Item::Origin(o) => Bounds::of_point(o.at),
        }
    }

    fn apply(&mut self, op: Op) {
        let (at, rotation, side) = match self {
            Item::Footprint(f) => (&mut f.at, Some(&mut f.rotation), Some(&mut f.side)),
            Item::FreePad(p) => (&mut p.at, Some(&mut p.rotation), p.side.as_mut()),
            Item::Origin(o) => (&mut o.at, None, None),
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
            Item::FreePad(p) => Object::FreePad(InDocument::new(document, p)),
            Item::Origin(o) => Object::PlacementOrigin(InDocument::new(document, o)),
        }
    }
}

/// The placeable objects named by `uuids` (each once, in order), or `invalid_params` at
/// `/objects/i` for one that is not a Footprint, Free Pad or the Placement Origin.
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
            Item::FreePad(p.clone())
        } else if let Some(o) = pcb.placement_origin().filter(|o| o.uuid == uuid) {
            Item::Origin(o.clone())
        } else {
            return Err(CommandError::invalid_params(
                &format!("/objects/{i}"),
                format!("{uuid} is not a Footprint, Free Pad or Placement Origin on the board"),
            ));
        };
        items.push(item);
    }
    Ok(items)
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
