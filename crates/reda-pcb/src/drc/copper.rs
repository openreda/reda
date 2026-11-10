//! The copper checks: clearance, short circuits, Track width and Keepouts.

use reda_model::{ComponentType, DrcCheck, Limit, RuleField, RuleKind, footprint_copper};

use super::{Board, Report, item_objects};
use crate::copper::shape::{BoardShape as _, Shape};
use crate::keepout;
use crate::rules::{net_rules, resolve, resolve_pair};

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    clearance(board, report);
    short_circuit(board, report);
    footprint_copper(board, report);
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
/// Net is unassigned copper, not a short. One Violation per Net pair and Layer, at the first
/// contact, listing every object of the pair touching on that Layer (several islands of one
/// Zone count once).
fn short_circuit(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::ShortCircuit) {
        return;
    }
    let copper = board.copper;
    let ties = crate::copper::ties::Ties::of(board.pcb);
    let mut shorts: Vec<Short> = Vec::new();
    for (a, b) in copper.touching_pairs() {
        let (na, nb) = (copper.net(a), copper.net(b));
        let (Some(na), Some(nb)) = (na, nb) else {
            continue;
        };
        if na == nb || ties.same_tie(a, b) {
            continue;
        }
        for (layer, pa) in copper.item_pieces(a) {
            for (_, pb) in copper.item_pieces(b).into_iter().filter(|(l, pb)| {
                *l == layer && pa.touches(pb)
            }) {
                let key = (na.min(nb), na.max(nb), layer);
                let short = match shorts.iter_mut().position(|s| s.key() == key) {
                    Some(i) => &mut shorts[i],
                    None => {
                        shorts.push(Short {
                            nets: (na, nb),
                            layer,
                            at: pa.closest_points(pb)[0],
                            items: Vec::new(),
                        });
                        shorts.last_mut().expect("just pushed")
                    }
                };
                for item in [a, b] {
                    if !short.items.contains(&item) {
                        short.items.push(item);
                    }
                }
            }
        }
    }
    for short in shorts {
        report.add(DrcCheck::ShortCircuit, |v| {
            v.located(board.pcb.uuid, short.at)
                .on_layers([short.layer])
                .arg("net_a", board.net_name(Some(short.nets.0)))
                .arg("net_b", board.net_name(Some(short.nets.1)))
                .objects(short.items.into_iter().flat_map(item_objects))
        });
    }
}

/// The copper of one Net pair touching on one Layer.
struct Short {
    /// The Nets as the first contact found them.
    nets: (reda_model::Uuid, reda_model::Uuid),
    layer: reda_model::Uuid,
    at: reda_model::geom::Point,
    items: Vec<crate::copper::Item>,
}

impl Short {
    fn key(&self) -> (reda_model::Uuid, reda_model::Uuid, reda_model::Uuid) {
        let (a, b) = self.nets;
        (a.min(b), a.max(b), self.layer)
    }
}

/// `shorted_copper` and `net_tie_not_joined` on each placed Footprint, read from its copy
/// with the same geometry as the Library check, so a Footprint swap or a type change is
/// caught. One `shorted_copper` per touching pair, objects as for a short.
fn footprint_copper(board: &Board<'_>, report: &mut Report) {
    let pcb = board.pcb;
    for instance in pcb.footprints() {
        let Some(copy) = pcb.footprint_copy(instance.footprint) else {
            continue;
        };
        let footprint = &copy.footprint;
        if instance.component_type == ComponentType::NetTie {
            if let Some(problem) = footprint_copper::tie_problem(footprint) {
                report.add(DrcCheck::NetTieNotJoined, |v| {
                    v.arg("footprint", footprint.name.as_str())
                        .arg("reason", problem.id())
                        .object(instance.uuid)
                });
            }
            continue;
        }
        if report.is_off(DrcCheck::ShortedCopper) {
            continue;
        }
        for (a, b) in footprint_copper::shorted_pads(footprint) {
            let (a, b) = (&footprint.pads[a], &footprint.pads[b]);
            let [at, _] = Shape::pad(instance, a).closest_points(&Shape::pad(instance, b));
            report.add(DrcCheck::ShortedCopper, |v| {
                v.located(pcb.uuid, at)
                    .arg("pad_a", a.number.as_str())
                    .arg("pad_b", b.number.as_str())
                    .arg("footprint", footprint.name.as_str())
                    .objects([instance.uuid, a.uuid, instance.uuid, b.uuid])
            });
        }
    }
}

/// Tracks narrower than the `track_width` minimum or wider than the maximum that applies
/// to their Net.
fn track_width(board: &Board<'_>, report: &mut Report) {
    for track in board.pcb.tracks() {
        let range = net_rules(board.pcb, track.net).track_width;
        let Some(found) = range.breach(track.width) else {
            continue;
        };
        let field = match found.0 {
            Limit::Min => RuleField::Min,
            Limit::Max => RuleField::Max,
        };
        let source = resolve(board.pcb, track.net, RuleKind::TrackWidth, field)
            .expect("a breached limit applies");
        report.add(DrcCheck::TrackWidth, |v| {
            breach(v, track.width, found, source.rule).object(track.uuid)
        });
    }
}

/// A Violation of a size `actual` beyond a `(limit, bound)`: its measurement and the
/// `actual`, `limit` and `minimum` or `maximum` arguments.
pub(super) fn breach(
    v: reda_model::Violation,
    actual: reda_model::geom::Nm,
    (limit, bound): (Limit, reda_model::geom::Nm),
    rule: reda_model::Uuid,
) -> reda_model::Violation {
    let (name, word) = match limit {
        Limit::Min => ("minimum", "min"),
        Limit::Max => ("maximum", "max"),
    };
    v.length(actual, bound, limit, rule)
        .arg(name, bound)
        .arg("actual", actual)
        .arg("limit", word)
}

/// Objects of a kind a Keepout restricts, on one of its Layers, inside it or closer than
/// the clearance (none under `zero_keepout_clearance`). Tracks, Vias, Pads and Free Pads
/// are checked by their copper; a Footprint by its Courtyard, or fallback box, on the
/// Keepout's side, with no clearance. One Violation per object and Keepout. Zone fills
/// leave Keepouts out when they are made.
fn keepout(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::Keepout) || board.pcb.keepouts().next().is_none() {
        return;
    }
    let pcb = board.pcb;
    let copper = board.copper;
    for item in copper.items() {
        let Some(kind) = keepout::restriction(pcb, item) else {
            continue;
        };
        let pieces = copper.item_pieces(item);
        for hit in keepout::entered(pcb, &pieces, kind, copper.net(item)) {
            report.add(DrcCheck::Keepout, |v| {
                v.length(hit.actual, hit.required, Limit::Min, hit.rule)
                    .distance_points(hit.points[0], hit.points[1])
                    .arg("required", hit.required)
                    .arg("actual", hit.actual)
                    .objects(item_objects(item))
                    .object(hit.keepout)
                    .on_layers(hit.layers.iter().copied())
            });
        }
    }
    super::placement::footprints_in_keepouts(board, report);
}
