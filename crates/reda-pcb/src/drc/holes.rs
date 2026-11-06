//! The drill checks: hole size, annular ring and hole-to-hole spacing, over Vias, the Pads
//! of placed Footprints and Free Pads.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{DrcCheck, Hole, PadKind, PcbDocument, RuleField, RuleKind, Uuid};

use super::{Board, Report};
use crate::copper::shape::{Core, Prim, Shape};
use crate::placed::{self, PlacedPad};
use crate::rules::{hole_to_hole_between, largest, net_rules};

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    let holes = holes(board);
    for hole in &holes {
        let actual = hole.size.width.min(hole.size.height);
        let Some(minimum) = hole.minimum else {
            continue;
        };
        if actual < minimum {
            report.add(DrcCheck::HoleSize, |v| {
                v.arg("minimum", minimum)
                    .arg("actual", actual)
                    .objects(hole.objects.iter().copied())
            });
        }
    }
    for hole in &holes {
        let minimum = net_rules(board.pcb, hole.net).annular_ring;
        if let (Some(ring), Some(minimum)) = (hole.ring, minimum)
            && ring < minimum
        {
            report.add(DrcCheck::AnnularRing, |v| {
                v.arg("minimum", minimum)
                    .arg("actual", ring)
                    .objects(hole.objects.iter().copied())
            });
        }
    }
    if !report.is_off(DrcCheck::HoleToHole) {
        hole_to_hole(board.pcb, &holes, report);
    }
}

/// One drilled hole on the board.
struct DrillHole {
    /// The Via, or the Footprint instance then the Pad, or the Free Pad.
    objects: Vec<Uuid>,
    size: Hole,
    /// Its Net, which picks its rules.
    net: Option<Uuid>,
    /// The smallest hole the rules allow it; `None` where the limit is absent.
    minimum: Option<Nm>,
    /// The copper ring around a plated hole; `None` for a bare hole.
    ring: Option<Nm>,
    shape: Shape,
}

/// Every hole, Vias first, then Footprint Pads, then Free Pads, each in UUID order.
fn holes(board: &Board<'_>) -> Vec<DrillHole> {
    let pcb = board.pcb;
    let mut out = Vec::new();
    for via in pcb.vias() {
        let size = Hole {
            width: via.hole,
            height: via.hole,
        };
        out.push(DrillHole {
            objects: vec![via.uuid],
            size,
            net: via.net,
            minimum: net_rules(pcb, via.net).via_hole.min,
            ring: Some(Nm((via.diameter.0 - via.hole.0) / 2)),
            shape: hole_shape(via.at, Millideg::ZERO, size),
        });
    }
    let pad_hole = |objects: Vec<Uuid>, pad: &PlacedPad| {
        let size = pad.hole?;
        let ring = (pad.kind == PadKind::PlatedThroughHole).then(|| {
            Nm(((pad.width.0 - size.width.0) / 2).min((pad.height.0 - size.height.0) / 2))
        });
        Some(DrillHole {
            objects,
            size,
            net: pad.net,
            minimum: net_rules(pcb, pad.net).via_hole.min,
            ring,
            shape: hole_shape(pad.at, pad.rotation, size),
        })
    };
    for instance in pcb.footprints() {
        for pad in placed::placed_pads(pcb, instance) {
            out.extend(pad_hole(vec![instance.uuid, pad.pad], &pad));
        }
    }
    for pad in pcb.free_pads() {
        out.extend(pad_hole(vec![pad.uuid], &placed::placed_free_pad(pad)));
    }
    out
}

/// A hole's outline: a disc, or a slot along its longer side turned by `rotation`.
fn hole_shape(at: Point, rotation: Millideg, size: Hole) -> Shape {
    let (w, h) = (size.width.0, size.height.0);
    let radius = w.min(h) / 2;
    let half = (w.max(h) - w.min(h)) / 2;
    let (dx, dy) = if w >= h { (half, 0) } else { (0, half) };
    let end = |sign: i64| {
        let p = Point::new(at.x + Nm(sign * dx), at.y + Nm(sign * dy));
        let p = placed::rotate(p, at, rotation);
        [p.x.0, p.y.0]
    };
    Shape {
        prims: vec![Prim {
            core: Core::Seg(end(-1), end(1)),
            radius,
        }],
    }
}

/// Holes whose edges come closer than the spacing their Nets require (the larger of the
/// two), overlapping ones included. One Violation per pair, in the order of [`holes`].
fn hole_to_hole(pcb: &PcbDocument, holes: &[DrillHole], report: &mut Report) {
    let reach = largest(pcb, RuleKind::HoleToHole, RuleField::Min);
    let mut order: Vec<usize> = (0..holes.len()).collect();
    let left = |i: usize| holes[i].shape.envelope().lower()[0];
    let right = |i: usize| holes[i].shape.envelope().upper()[0];
    order.sort_by_key(|&i| left(i));
    let mut pairs = Vec::new();
    for (k, &i) in order.iter().enumerate() {
        for &j in &order[k + 1..] {
            if left(j) - right(i) >= reach.0 {
                break;
            }
            let Some(required) = hole_to_hole_between(pcb, holes[i].net, holes[j].net) else {
                continue;
            };
            let gap = holes[i].shape.distance(&holes[j].shape);
            if gap < required.0 as f64 {
                let actual = Nm(gap.max(0.0).floor() as i64);
                pairs.push((i.min(j), i.max(j), actual, required));
            }
        }
    }
    pairs.sort();
    for (i, j, actual, required) in pairs {
        report.add(DrcCheck::HoleToHole, |v| {
            v.arg("required", required)
                .arg("actual", actual)
                .objects(holes[i].objects.iter().copied())
                .objects(holes[j].objects.iter().copied())
        });
    }
}
