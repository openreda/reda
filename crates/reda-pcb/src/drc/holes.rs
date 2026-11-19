//! The drill checks: hole size, annular ring and hole-to-hole spacing, over Vias, the Pads
//! of placed Footprints and Free Pads.

use crate::copper::shape::BoardShape as _;
use reda_model::geom::Nm;
use reda_model::{DrcCheck, Limit, PcbDocument, RuleField, RuleKind};

use super::{Board, Report};
use crate::drill::{DrillHole, Rings, holes};
use crate::rules::{breach, hole_to_hole_between, largest, net_rules, resolve, resolve_pair};

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    via_size(board, report);
    let holes = holes(board.pcb);
    super::plated_clearance::check(board, &holes, report);
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
        let Some(rings) = hole.ring else {
            continue;
        };
        let lands = board.copper.item_pieces(hole.item);
        let mut breaches = std::collections::BTreeMap::<_, Vec<_>>::new();
        for (layer, _) in lands {
            let ring = match rings {
                Rings::Via => {
                    let via = board.pcb.via(hole.objects[0]).expect("Via hole");
                    Nm((via.diameter_on(layer).0 - via.hole.0) / 2)
                }
                Rings::Pad {
                    top,
                    middle,
                    bottom,
                } => match board.pcb.layers().layer(layer).expect("copper Layer").side {
                    Some(reda_model::Side::Top) => top,
                    None => middle,
                    Some(reda_model::Side::Bottom) => bottom,
                },
            };
            if let Some(source) = crate::rules::resolve_on(
                board.pcb,
                hole.net,
                Some(layer),
                RuleKind::AnnularRing,
                RuleField::Min,
            ) && let Some(min) = source.value.length()
                && ring < min
            {
                breaches
                    .entry((source.rule, min, ring))
                    .or_default()
                    .push(layer);
            }
        }
        for ((rule, minimum, ring), layers) in breaches {
            report.add(DrcCheck::AnnularRing, |v| {
                v.length(ring, minimum, Limit::Min, rule)
                    .located(board.pcb.uuid, hole.shape.closest_points(&hole.shape)[0])
                    .on_layers(layers)
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
                via.diameter.start,
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
