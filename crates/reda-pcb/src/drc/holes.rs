//! The drill checks: hole size, annular ring and hole-to-hole spacing, over Vias, the Pads
//! of placed Footprints and Free Pads.

use reda_model::geom::{Nm, Point};
use reda_model::{DrcCheck, Hole, Limit, PadKind, PcbDocument, RuleField, RuleKind, Uuid};

use super::{Board, Report};
use crate::copper::shape::{Core, Prim, Shape};
use crate::placed::{self, PlacedPad};
use crate::rules::{breach, hole_to_hole_between, largest, net_rules, resolve, resolve_pair};

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    via_size(board, report);
    let holes = holes(board);
    for hole in holes.iter().filter(|h| !h.via) {
        let rules = net_rules(board.pcb, hole.net);
        let (narrow, wide) = (
            hole.size.width.min(hole.size.height),
            hole.size.width.max(hole.size.height),
        );
        let found = breach(rules.hole_min, None, narrow)
            .map(|b| (narrow, b))
            .or_else(|| breach(None, rules.hole_max, wide).map(|b| (wide, b)));
        let Some((actual, found)) = found else {
            continue;
        };
        let field = match found.0 {
            Limit::Min => RuleField::Min,
            Limit::Max => RuleField::Max,
        };
        let rule = resolve(board.pcb, hole.net, RuleKind::HoleSize, field)
            .expect("a breached limit applies")
            .rule;
        report.add(DrcCheck::HoleSize, |v| {
            super::copper::breach(v, actual, found, rule)
                .located(board.pcb.uuid, hole.shape.closest_points(&hole.shape)[0])
                .on_layers(board.pcb.copper_layers().map(|l| l.uuid))
                .objects(hole.objects.iter().copied())
        });
    }
    for hole in &holes {
        let minimum = net_rules(board.pcb, hole.net).annular_ring;
        if let (Some(ring), Some(minimum)) = (hole.ring, minimum)
            && ring < minimum
        {
            report.add(DrcCheck::AnnularRing, |v| {
                v.length(
                    ring,
                    minimum,
                    Limit::Min,
                    resolve(board.pcb, hole.net, RuleKind::AnnularRing, RuleField::Min)
                        .expect("ring limit applies")
                        .rule,
                )
                .located(board.pcb.uuid, hole.shape.closest_points(&hole.shape)[0])
                .on_layers(board.pcb.copper_layers().map(|l| l.uuid))
                .arg("minimum", minimum)
                .arg("actual", ring)
                .objects(hole.objects.iter().copied())
            });
        }
    }
    if !report.is_off(DrcCheck::HoleToHole) {
        hole_to_hole(board.pcb, &holes, report);
    }
}

/// Vias whose diameter or hole lies outside the `via_size` range of their Net: one
/// Violation per Via and part, the diameter first, `part` naming which.
fn via_size(board: &Board<'_>, report: &mut Report) {
    use RuleField::*;
    let pcb = board.pcb;
    for via in pcb.vias() {
        let rules = net_rules(pcb, via.net);
        for (part, size, range, [min, max]) in [
            (
                "diameter",
                via.diameter,
                rules.via_diameter,
                [DiameterMin, DiameterMax],
            ),
            ("hole", via.hole, rules.via_hole, [HoleMin, HoleMax]),
        ] {
            let Some(found) = range.breach(size) else {
                continue;
            };
            let field = if found.0 == Limit::Min { min } else { max };
            let rule = resolve(pcb, via.net, RuleKind::ViaSize, field)
                .expect("a breached limit applies")
                .rule;
            report.add(DrcCheck::ViaSize, |v| {
                super::copper::breach(v, size, found, rule)
                    .located(pcb.uuid, via.at)
                    .on_layers(pcb.copper_layers().map(|l| l.uuid))
                    .arg("part", part)
                    .object(via.uuid)
            });
        }
    }
}

/// One drilled hole on the board.
struct DrillHole {
    /// The Via, or the Footprint instance then the Pad, or the Free Pad.
    objects: Vec<Uuid>,
    size: Hole,
    /// Its Net, which picks its rules.
    net: Option<Uuid>,
    /// Whether it is a Via's, which `via_size` limits rather than `hole_size`.
    via: bool,
    /// The copper ring around a plated hole; `None` for a bare hole.
    ring: Option<Nm>,
    shape: Shape,
}

/// Every hole, Vias first, then Footprint Pads, then Free Pads, each in UUID order.
fn holes(board: &Board<'_>) -> Vec<DrillHole> {
    let pcb = board.pcb;
    let mut out = Vec::new();
    for via in pcb.vias() {
        let size = Hole::new(via.hole, via.hole);
        out.push(DrillHole {
            objects: vec![via.uuid],
            size,
            net: via.net,
            via: true,
            ring: Some(Nm((via.diameter.0 - via.hole.0) / 2)),
            shape: hole_shape((via.at, via.at), size),
        });
    }
    let pad_hole = |objects: Vec<Uuid>, pad: &PlacedPad| {
        let size = pad.hole?;
        let ring = (pad.kind == PadKind::PlatedThroughHole).then(|| annular_ring(pad, size));
        Some(DrillHole {
            objects,
            size,
            net: pad.net,
            via: false,
            ring,
            shape: hole_shape(pad.hole_ends()?, size),
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

/// The narrowest copper between a Pad's hole and the edge of its copper, negative when
/// the hole reaches outside it. The copper is convex, so the narrowest point is at an end
/// of the slot's centre line.
pub(crate) fn annular_ring(pad: &PlacedPad, hole: Hole) -> Nm {
    let (a, b) = hole.ends();
    let inside = pad
        .outline
        .edge_distance(a)
        .min(pad.outline.edge_distance(b));
    Nm((inside - hole.diameter().0 as f64 / 2.0).floor() as i64)
}

/// A hole's outline: a disc, or a slot between the `ends` of its centre line.
fn hole_shape(ends: (Point, Point), size: Hole) -> Shape {
    let end = |p: Point| [p.x.0, p.y.0];
    Shape {
        prims: vec![Prim {
            core: Core::Seg(end(ends.0), end(ends.1)),
            radius: size.diameter().0 / 2,
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
        let source = resolve_pair(
            pcb,
            holes[i].net,
            holes[j].net,
            RuleKind::HoleToHole,
            RuleField::Min,
        )
        .expect("spacing limit applies");
        let [a, b] = holes[i].shape.closest_points(&holes[j].shape);
        report.add(DrcCheck::HoleToHole, |v| {
            v.length(actual, required, Limit::Min, source.rule)
                .distance_points(a, b)
                .on_layers(pcb.copper_layers().map(|l| l.uuid))
                .arg("required", required)
                .arg("actual", actual)
                .objects(holes[i].objects.iter().copied())
                .objects(holes[j].objects.iter().copied())
        });
    }
}
