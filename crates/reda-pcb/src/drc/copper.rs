//! The copper checks: clearance, short circuits, Track width and Keepouts.

use reda_model::{DrcCheck, Limit, RuleField, RuleKind};

use super::{Board, Report, item_objects};
use crate::copper::Item;
use crate::rules::{net_rules, resolve, resolve_pair};
use crate::zone::keepouts_entered;

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    clearance(board, report);
    short_circuit(board, report);
    track_width(board, report);
    keepout(board, report);
}

/// Copper of different Nets closer than the larger of their Net Classes' clearances. One
/// Violation per pair, objects the lesser item's then the other's.
fn clearance(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::Clearance) {
        return;
    }
    for gap in board.copper.gaps(board.pcb) {
        let source = resolve_pair(
            board.pcb,
            board.copper.net(gap.a),
            board.copper.net(gap.b),
            RuleKind::Clearance,
            RuleField::Value,
        )
        .expect("gap has a limit");
        let pairs: Vec<_> = board
            .copper
            .item_pieces(gap.a)
            .into_iter()
            .flat_map(|(layer, a)| {
                board
                    .copper
                    .item_pieces(gap.b)
                    .into_iter()
                    .filter(move |(l, _)| *l == layer)
                    .map(move |(_, b)| (layer, a, b))
            })
            .filter(|(_, a, b)| a.distance(b) < gap.required.0 as f64)
            .collect();
        let pair = pairs
            .iter()
            .min_by(|(_, a, b), (_, c, d)| a.distance(b).total_cmp(&c.distance(d)))
            .expect("gap shares a Layer");
        let [a, b] = pair.1.closest_points(pair.2);
        report.add(DrcCheck::Clearance, |v| {
            v.length(gap.actual, gap.required, Limit::Min, source.rule)
                .distance_points(a, b)
                .on_layers(pairs.iter().map(|(layer, _, _)| *layer))
                .arg("required", gap.required)
                .arg("actual", gap.actual)
                .objects(item_objects(gap.a))
                .objects(item_objects(gap.b))
        });
    }
}

/// Copper of two different Nets touching, Zone fills included. Copper on no Net touching a
/// Net is unassigned copper, not a short. Several islands of one Zone touching the same
/// copper are one short.
fn short_circuit(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::ShortCircuit) {
        return;
    }
    let copper = board.copper;
    let mut seen = std::collections::BTreeSet::new();
    for (a, b) in copper.touching_pairs() {
        let (na, nb) = (copper.net(a), copper.net(b));
        let objects = [item_objects(a), item_objects(b)].concat();
        if na.is_none() || nb.is_none() || na == nb || !seen.insert(objects.clone()) {
            continue;
        }
        let touching: Vec<_> = copper
            .item_pieces(a)
            .into_iter()
            .flat_map(|(layer, a)| {
                copper
                    .item_pieces(b)
                    .into_iter()
                    .filter(move |(l, b)| *l == layer && a.touches(b))
                    .map(move |(_, b)| (layer, a, b))
            })
            .collect();
        let contact = touching.first().expect("touching pair shares copper");
        let at = contact.1.closest_points(contact.2)[0];
        report.add(DrcCheck::ShortCircuit, |v| {
            v.located(board.pcb.uuid, at)
                .on_layers(touching.iter().map(|(l, _, _)| *l))
                .arg("net_a", board.net_name(na))
                .arg("net_b", board.net_name(nb))
                .objects(objects)
        });
    }
}

/// Tracks narrower than the `track_width` minimum that applies to their Net.
fn track_width(board: &Board<'_>, report: &mut Report) {
    for track in board.pcb.tracks() {
        let Some(minimum) = net_rules(board.pcb, track.net).track_width.min else {
            continue;
        };
        if track.width < minimum {
            let source = resolve(board.pcb, track.net, RuleKind::TrackWidth, RuleField::Min)
                .expect("minimum applies");
            report.add(DrcCheck::TrackWidth, |v| {
                v.length(track.width, minimum, Limit::Min, source.rule)
                    .arg("minimum", minimum)
                    .arg("actual", track.width)
                    .object(track.uuid)
            });
        }
    }
}

/// Pads, Free Pads, Tracks and Vias inside a Keepout that forbids them on one of their
/// Layers: copper, or for a Via, Vias. One Violation per object and Keepout. Zone fills
/// leave Keepouts out when they are made.
fn keepout(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::Keepout) || board.pcb.keepouts().next().is_none() {
        return;
    }
    let copper = board.copper;
    for item in copper.items() {
        if matches!(item, Item::Zone { .. }) {
            continue;
        }
        let pieces = copper.item_pieces(item);
        let Some((_, shape)) = pieces.first() else {
            continue;
        };
        let layers: Vec<_> = pieces.iter().map(|(l, _)| *l).collect();
        let is_via = matches!(item, Item::Via { .. });
        for keepout in keepouts_entered(board.pcb, shape, &layers, is_via) {
            let restricted = &board
                .pcb
                .keepout(keepout)
                .expect("entered Keepout exists")
                .layers;
            report.add(DrcCheck::Keepout, |v| {
                v.objects(item_objects(item)).object(keepout).on_layers(
                    layers
                        .iter()
                        .copied()
                        .filter(|layer| restricted.contains(layer)),
                )
            });
        }
    }
}
