//! The connection checks: unrouted connections, copper on no Net, dangling Tracks and
//! Vias, poor connections and Vias under SMD Pads.

use reda_model::geom::{Nm, Point};
use reda_model::{DrcCheck, PadKind, RuleField, RuleKind, Track, Uuid};

use super::{Board, Report, item_objects};
use crate::copper::Item;
use crate::copper::shape::Shape;
use crate::rules::resolve;

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    unrouted(board, report);
    copper_without_net(board, report);
    dangling(board, report);
    poor_connection(board, report);
    via_under_smd(board, report);
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

/// Track ends whose copper touches no other copper (one Violation per end, at it), and
/// Vias that connect copper on fewer than two Layers, so they join nothing. A stub (the
/// run of Tracks from a dangling end back to where it branches or meets other copper) no
/// longer than the `dangling` tolerance passes.
fn dangling(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::Dangling) {
        return;
    }
    let pcb = board.pcb;
    let copper = board.copper;
    let tolerance =
        |net: Option<Uuid>| match resolve(pcb, net, RuleKind::Dangling, RuleField::Tolerance) {
            Some(s) => (s.value.length().unwrap_or(Nm::ZERO), s.rule),
            None => (Nm::ZERO, pcb.board_rule(RuleKind::Dangling).uuid),
        };
    for via in pcb.vias() {
        let item = Item::Via { uuid: via.uuid };
        let layers = (pcb.span_layers(via.span).into_iter())
            .filter(|layer| copper.touches_on(item, *layer))
            .count();
        if layers < 2 {
            let (_, rule) = tolerance(via.net);
            report.add(DrcCheck::Dangling, |v| {
                v.with_rule(rule)
                    .arg("x", via.at.x)
                    .arg("y", via.at.y)
                    .object(via.uuid)
            });
        }
    }
    for track in pcb.tracks() {
        let (from, to) = track.geometry.ends();
        for end in [from, to] {
            if !alone(board, track, end) {
                continue;
            }
            let (tolerance, rule) = tolerance(track.net);
            if tolerance > Nm::ZERO && stub_length(board, track, end) <= tolerance.0 as f64 {
                continue;
            }
            report.add(DrcCheck::Dangling, |v| {
                v.with_rule(rule)
                    .on_layers([track.layer])
                    .arg("x", end.x)
                    .arg("y", end.y)
                    .object(track.uuid)
            });
        }
    }
}

/// Whether the round end of `track` at `end` touches no other copper.
fn alone(board: &Board<'_>, track: &Track, end: Point) -> bool {
    let item = Item::Track { uuid: track.uuid };
    let cap = Shape::disc(end, track.width);
    board
        .copper
        .items_touching(track.layer, &cap)
        .into_iter()
        .all(|other| other == item)
}

/// The length of the stub that dangles at `end` of `track`: the Track, then on through
/// every Track that continues it alone, until the run branches or meets other copper.
fn stub_length(board: &Board<'_>, track: &Track, end: Point) -> f64 {
    let mut length = track.geometry.length();
    let mut current = track;
    let (a, b) = track.geometry.ends();
    let mut at = if a == end { b } else { a };
    let mut seen = vec![track.uuid];
    loop {
        let others: Vec<Item> = board
            .copper
            .items_at(current.layer, at)
            .into_iter()
            .filter(|i| *i != Item::Track { uuid: current.uuid })
            .collect();
        let [Item::Track { uuid }] = others.as_slice() else {
            return length;
        };
        let Some(next) = board.pcb.track(*uuid) else {
            return length;
        };
        let (a, b) = next.geometry.ends();
        if seen.contains(uuid) || (a != at && b != at) {
            return length;
        }
        seen.push(*uuid);
        length += next.geometry.length();
        at = if a == at { b } else { a };
        current = next;
    }
}

/// Track ends whose round end touches copper of their Net while the end's centre lies
/// outside it: one Violation per end and piece of copper touched, naming the Track then
/// that copper. Zones are left out: their fill follows the copper, not the other way.
fn poor_connection(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::PoorConnection) {
        return;
    }
    let copper = board.copper;
    for track in board.pcb.tracks() {
        let item = Item::Track { uuid: track.uuid };
        let (from, to) = track.geometry.ends();
        if from == to {
            continue;
        }
        for end in [from, to] {
            let on = copper.items_at(track.layer, end);
            if on.iter().any(|other| *other != item) {
                continue;
            }
            let cap = Shape::disc(end, track.width);
            for other in copper.items_touching(track.layer, &cap) {
                if other == item
                    || matches!(other, Item::Zone { .. })
                    || copper.net(other) != track.net
                {
                    continue;
                }
                report.add(DrcCheck::PoorConnection, |v| {
                    v.located(board.pcb.uuid, end)
                        .on_layers([track.layer])
                        .arg("x", end.x)
                        .arg("y", end.y)
                        .object(track.uuid)
                        .objects(item_objects(other))
                });
            }
        }
    }
}

/// Vias whose centre lies on an SMD Pad's copper, on a Layer the Via joins: one Violation
/// per Via and Pad, naming the Pad then the Via.
fn via_under_smd(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::ViaUnderSmd) {
        return;
    }
    let pcb = board.pcb;
    for via in pcb.vias() {
        for layer in pcb.span_layers(via.span) {
            for other in board.copper.items_at(layer, via.at) {
                let smd = match other {
                    Item::Pad { footprint, pad } => pcb
                        .footprint(footprint)
                        .and_then(|f| pcb.footprint_copy(f.footprint))
                        .and_then(|c| c.footprint.pads.iter().find(|p| p.uuid == pad))
                        .is_some_and(|p| p.kind == PadKind::Smd),
                    Item::FreePad { uuid } => {
                        pcb.free_pad(uuid).is_some_and(|p| p.kind == PadKind::Smd)
                    }
                    _ => false,
                };
                if smd {
                    report.add(DrcCheck::ViaUnderSmd, |v| {
                        v.located(pcb.uuid, via.at)
                            .on_layers([layer])
                            .objects(item_objects(other))
                            .object(via.uuid)
                    });
                }
            }
        }
    }
}
