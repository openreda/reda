//! The second clearance measurement: from the edge of a plated bore, even on Layers
//! where the Pad Stack has no copper. The copper-to-copper limit applies separately.
use std::collections::BTreeMap;

use reda_model::{ConnectionStyle, DrcCheck, Limit, RuleField, RuleKind, geom::Nm};

use super::{Board, Report, item_objects};
use crate::copper::Item;
use crate::drill::DrillHole;
use crate::rules::{largest, resolve_pair_on};

pub(super) fn check(board: &Board<'_>, holes: &[DrillHole], report: &mut Report) {
    if report.is_off(DrcCheck::Clearance) {
        return;
    }
    let pcb = board.pcb;
    let reach = largest(pcb, RuleKind::Clearance, RuleField::HoleVia).max(largest(
        pcb,
        RuleKind::Clearance,
        RuleField::HolePad,
    ));
    if reach == Nm::ZERO {
        return;
    }
    let ties = crate::copper::ties::Ties::of(pcb);
    for hole in holes.iter().filter(|h| h.ring.is_some()) {
        let lands = board.copper.item_pieces(hole.item);
        let mut hits = BTreeMap::new();
        for layer in hole.layers(pcb) {
            let land = lands.iter().find(|(l, _)| *l == layer).map(|(_, s)| *s);
            for (item, shape) in board.copper.pieces_near(layer, &hole.shape, reach) {
                if item == hole.item {
                    continue;
                }
                let net = board.copper.net(item);
                if land.is_some()
                    && net.is_some()
                    && net == hole.net
                    && (!matches!(item, Item::Zone { .. })
                        || hole.connection(pcb, layer).style != ConnectionStyle::None)
                {
                    continue;
                }
                let Some(source) = resolve_pair_on(
                    pcb,
                    hole.net,
                    net,
                    Some(layer),
                    RuleKind::Clearance,
                    hole.field(),
                ) else {
                    continue;
                };
                let required = source.value.length().expect("clearance is a length");
                let gap = hole.shape.distance(shape);
                if gap >= required.0 as f64
                    || crate::copper::exempt_pair(pcb, &ties, (hole.item, hole.net), (item, net))
                {
                    continue;
                }
                // Touching assigned lands of different Nets have a short on this Layer.
                if net.is_some()
                    && hole.net.is_some()
                    && net != hole.net
                    && land.is_some_and(|s| s.touches(shape))
                {
                    continue;
                }
                let measured = Nm(gap.max(0.).floor() as i64);
                let points = hole.shape.closest_points(shape);
                let hit = hits.entry((item, source.rule, required)).or_insert((
                    measured,
                    points,
                    Vec::new(),
                ));
                if measured < hit.0 {
                    hit.0 = measured;
                    hit.1 = points;
                }
                hit.2.push(layer);
            }
        }
        for ((item, rule, required), (actual, [a, b], layers)) in hits {
            report.add(DrcCheck::Clearance, |v| {
                v.length(actual, required, Limit::Min, rule)
                    .distance_points(a, b)
                    .on_layers(layers)
                    .arg("required", required)
                    .arg("actual", actual)
                    .arg("part", hole.field().path())
                    .objects(hole.objects.iter().copied())
                    .objects(item_objects(item))
            });
        }
    }
}
