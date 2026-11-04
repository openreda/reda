//! The connection checks: unrouted connections, copper on no Net, and dangling Tracks and
//! Vias.

use reda_model::{DrcCheck, PadKind};

use super::{Board, Report, item_objects};
use crate::copper::Item;

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    unrouted(board, report);
    copper_without_net(board, report);
    dangling(board, report);
}

/// One Violation per Ratsnest line (Zone fills count as copper): its two ends' objects.
fn unrouted(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::Unrouted) {
        return;
    }
    for line in board.copper.ratsnest() {
        report.add(DrcCheck::Unrouted, |v| {
            v.arg("net", board.net_name(Some(line.net)))
                .objects(item_objects(line.from_item))
                .objects(item_objects(line.to_item))
        });
    }
}

/// Free Pads with copper, Vias, Tracks and Zones on no Net, in that order. Pads of
/// Footprints are left out: an unconnected Pin's Pad is on no Net by design.
fn copper_without_net(board: &Board<'_>, report: &mut Report) {
    let pcb = board.pcb;
    let free_pads = (pcb.free_pads())
        .filter(|p| p.kind != PadKind::NonPlated && p.net.is_none())
        .map(|p| p.uuid);
    let vias = pcb.vias().filter(|v| v.net.is_none()).map(|v| v.uuid);
    let tracks = pcb.tracks().filter(|t| t.net.is_none()).map(|t| t.uuid);
    let zones = pcb.zones().filter(|z| z.net.is_none()).map(|z| z.uuid);
    for uuid in free_pads.chain(vias).chain(tracks).chain(zones) {
        report.add(DrcCheck::CopperWithoutNet, |v| v.object(uuid));
    }
}

/// Track ends that touch no other copper (one Violation per end, at it), and Vias that
/// connect copper on fewer than two Layers, so they join nothing.
fn dangling(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::Dangling) {
        return;
    }
    let copper = board.copper;
    for via in board.pcb.vias() {
        let item = Item::Via { uuid: via.uuid };
        let layers = (board.pcb.span_layers(via.span).into_iter())
            .filter(|layer| copper.touches_on(item, *layer))
            .count();
        if layers < 2 {
            report.add(DrcCheck::Dangling, |v| {
                v.arg("x", via.at.x).arg("y", via.at.y).object(via.uuid)
            });
        }
    }
    for track in board.pcb.tracks() {
        let item = Item::Track { uuid: track.uuid };
        let (from, to) = track.geometry.ends();
        for end in [from, to] {
            let alone = (copper.items_at(track.layer, end).into_iter()).all(|other| other == item);
            if alone {
                report.add(DrcCheck::Dangling, |v| {
                    v.arg("x", end.x).arg("y", end.y).object(track.uuid)
                });
            }
        }
    }
}
