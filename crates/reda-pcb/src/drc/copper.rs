//! The copper checks: clearance, short circuits, Track width and Keepouts.

use reda_model::DrcCheck;

use super::{Board, Report, item_objects};
use crate::copper::Item;
use crate::net_rules::net_rules;
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
        report.add(DrcCheck::Clearance, |v| {
            v.arg("required", gap.required)
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
        report.add(DrcCheck::ShortCircuit, |v| {
            v.arg("net_a", board.net_name(na))
                .arg("net_b", board.net_name(nb))
                .objects(objects)
        });
    }
}

/// Tracks narrower than their Net Class's Track width (the board rule where the class
/// sets none).
fn track_width(board: &Board<'_>, report: &mut Report) {
    for track in board.pcb.tracks() {
        let minimum = net_rules(board.pcb, track.net).track_width;
        if track.width < minimum {
            report.add(DrcCheck::TrackWidth, |v| {
                v.arg("minimum", minimum)
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
            report.add(DrcCheck::Keepout, |v| {
                v.objects(item_objects(item)).object(keepout)
            });
        }
    }
}
