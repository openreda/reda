//! The copper checks: clearance, short circuits, Track width and Keepouts.

use reda_model::{ComponentType, DrcCheck, Limit, RuleField, RuleKind, footprint_copper};

use super::{Board, Report, item_objects};
use crate::copper::Item;
use crate::copper::shape::Shape;
use crate::keepout;
use crate::rules::{net_rules_on, resolve_on};

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
        let pieces: Vec<_> = board
            .copper
            .item_pieces(gap.a)
            .into_iter()
            .filter(|(layer, _)| gap.layers.contains(layer))
            .collect();
        let found = closest_gap(
            &pieces,
            &board.copper.item_pieces(gap.b),
            gap.required,
            gap.a.is_artwork() || gap.b.is_artwork(),
        )
        .expect("gap shares a Layer");
        report.add(DrcCheck::Clearance, |v| {
            v.length(gap.actual, gap.required, Limit::Min, gap.rule)
                .distance_points(found.points[0], found.points[1])
                .on_layers(found.layers)
                .arg("required", gap.required)
                .arg("actual", gap.actual)
                .objects(item_objects(gap.a))
                .objects(item_objects(gap.b))
        });
    }
}

/// Where two items' copper breaks a clearance: the closest points of the nearest pieces
/// and every shared Layer where they come closer than required.
#[derive(Debug, PartialEq)]
struct GapAt {
    points: [reda_model::geom::Point; 2],
    layers: Vec<reda_model::Uuid>,
}

/// The [`GapAt`] of two items' pieces that breach clearance on a shared Layer.
fn closest_gap(
    a: &[(reda_model::Uuid, &Shape)],
    b: &[(reda_model::Uuid, &Shape)],
    required: reda_model::geom::Nm,
    artwork: bool,
) -> Option<GapAt> {
    let pairs: Vec<_> = (a.iter())
        .flat_map(|&(layer, a)| {
            (b.iter())
                .filter(move |(l, _)| *l == layer)
                .map(move |&(_, b)| (layer, a, b))
        })
        .filter(|(_, a, b)| crate::copper::breaches_clearance(a.distance(b), required, artwork))
        .collect();
    let (_, a, b) = pairs
        .iter()
        .min_by(|(_, a, b), (_, c, d)| a.distance(b).total_cmp(&c.distance(d)))?;
    Some(GapAt {
        points: a.closest_points(b),
        layers: pairs.iter().map(|(layer, _, _)| *layer).collect(),
    })
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
        for (layer, pa, pb) in touching_pieces(copper, a, b) {
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

/// Both items' indexed copper on each physical Layer where it actually touches.
fn touching_pieces(
    copper: &crate::Copper,
    a: Item,
    b: Item,
) -> impl Iterator<Item = (reda_model::Uuid, &Shape, &Shape)> {
    let other = copper.item_pieces(b);
    copper
        .item_pieces(a)
        .into_iter()
        .filter_map(move |(layer, first)| {
            let (_, second) = other
                .iter()
                .find(|(l, shape)| *l == layer && first.touches(shape))?;
            Some((layer, first, *second))
        })
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
/// and its indexed copper. Each touching pair of differently numbered Pads produces
/// one `shorted_copper` on the Layers where the two lands meet.
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
        for a in &footprint.pads {
            let item = Item::Pad {
                footprint: instance.uuid,
                pad: a.uuid,
            };
            for other in board.copper.contacts(item) {
                let Item::Pad {
                    footprint: owner,
                    pad,
                } = other
                else {
                    continue;
                };
                if owner != instance.uuid || pad <= a.uuid {
                    continue;
                }
                let Some(b) = footprint
                    .pads
                    .iter()
                    .find(|p| p.uuid == pad && p.number != a.number)
                else {
                    continue;
                };
                let mut hits = touching_pieces(board.copper, item, other);
                let Some((layer, pa, pb)) = hits.next() else {
                    continue;
                };
                report.add(DrcCheck::ShortedCopper, |v| {
                    v.located(pcb.uuid, pa.closest_points(pb)[0])
                        .on_layers(std::iter::once(layer).chain(hits.map(|(l, _, _)| l)))
                        .arg("pad_a", a.number.as_str())
                        .arg("pad_b", b.number.as_str())
                        .arg("footprint", footprint.name.as_str())
                        .objects([instance.uuid, a.uuid, instance.uuid, b.uuid])
                });
            }
        }
    }
}

/// Tracks narrower than the `track_width` minimum or wider than the maximum that applies
/// to their Net.
fn track_width(board: &Board<'_>, report: &mut Report) {
    for track in board.pcb.tracks() {
        let range = net_rules_on(board.pcb, track.net, Some(track.layer)).track_width;
        let Some(found) = range.breach(track.width) else {
            continue;
        };
        let field = match found.0 {
            Limit::Min => RuleField::Min,
            Limit::Max => RuleField::Max,
        };
        let source = resolve_on(
            board.pcb,
            track.net,
            Some(track.layer),
            RuleKind::TrackWidth,
            field,
        )
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

#[cfg(test)]
mod tests {
    use super::{GapAt, Shape, closest_gap};
    use reda_model::Uuid;
    use reda_model::geom::{Nm, Point};

    fn at(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn touching_pieces_on_one_layer_leave_the_gap_on_another() {
        let (top, bottom) = (Uuid::from_u128(1), Uuid::from_u128(2));
        // Two 1 mm discs: overlapping on the top Layer, 0.1 mm apart on the bottom one.
        let a = Shape::disc(at(0, 0), Nm(1_000_000));
        let b_top = Shape::disc(at(900_000, 0), Nm(1_000_000));
        let b_bottom = Shape::disc(at(0, 1_100_000), Nm(1_000_000));
        let found = closest_gap(
            &[(top, &a), (bottom, &a)],
            &[(top, &b_top), (bottom, &b_bottom)],
            Nm(200_000),
            false,
        );
        assert_eq!(
            found,
            Some(GapAt {
                points: [at(0, 500_000), at(0, 600_000)],
                layers: vec![bottom],
            })
        );
    }
}
