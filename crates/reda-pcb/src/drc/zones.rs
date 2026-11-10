//! What the island settings leave of Zone fills: `dead_copper` for kept pieces of fill
//! that touch no copper of their Net (directly or through other fills), above the
//! `dead_copper` rule's `min_area`, and `empty_zone` for a Zone with a Net and nothing
//! left to fill. Zones with no Net are exempt from both. Also `shelved_zone` for each
//! shelved Zone, so one left on the shelf can't drop copper from outputs silently.

use reda_model::geom::Nm;
use reda_model::{DrcCheck, FillShape, Limit, RuleField, RuleKind, ZoneFills};

use super::{Board, Report};
use crate::copper::Item;
use crate::rules::resolve;

pub(super) fn check(board: &Board<'_>, fills: &ZoneFills, report: &mut Report) {
    let pcb = board.pcb;
    if !report.is_off(DrcCheck::ShelvedZone) {
        let names = crate::zone::names(pcb);
        for zone in pcb.zones().filter(|z| z.shelved) {
            let at = pcb
                .zone_vertices(zone)
                .first()
                .map_or_else(|| reda_model::geom::Point::new(Nm(0), Nm(0)), |v| v.point());
            report.add(DrcCheck::ShelvedZone, |v| {
                v.object(zone.uuid)
                    .located(pcb.uuid, at)
                    .on_layers([zone.layer])
                    .arg("zone", names[&zone.uuid].as_str())
            });
        }
    }
    if report.is_off(DrcCheck::DeadCopper) && report.is_off(DrcCheck::EmptyZone) {
        return;
    }
    for zone in pcb.active_zones().filter(|z| z.net.is_some()) {
        let Some(corner) = pcb.zone_vertices(zone).first().map(|v| v.point()) else {
            continue;
        };
        let shapes = fills
            .zones
            .get(&zone.uuid)
            .map_or(&[][..], |f| &f.shapes[..]);
        if shapes.is_empty() {
            report.add(DrcCheck::EmptyZone, |v| {
                v.object(zone.uuid)
                    .located(pcb.uuid, corner)
                    .on_layers([zone.layer])
                    .arg("net", board.net_name(zone.net).as_str())
            });
            continue;
        }
        if report.is_off(DrcCheck::DeadCopper) {
            continue;
        }
        let Some(limit) = resolve(pcb, zone.net, RuleKind::DeadCopper, RuleField::MinArea) else {
            continue;
        };
        let min_area = limit.value.area().expect("an area");
        for (i, shape) in shapes.iter().enumerate() {
            let island = Item::Zone {
                zone: zone.uuid,
                island: i as u32,
            };
            let joined = board
                .copper
                .connected(island)
                .iter()
                .any(|item| !matches!(item, Item::Zone { .. }));
            let area = area(shape);
            if joined || area <= min_area {
                continue;
            }
            report.add(DrcCheck::DeadCopper, |v| {
                v.object(zone.uuid)
                    .located(pcb.uuid, shape.outer[0])
                    .on_layers([zone.layer])
                    .area(area, min_area, Limit::Max, limit.rule)
                    .arg("net", board.net_name(zone.net).as_str())
            });
        }
    }
}

/// The area of one piece of fill, in square nanometres.
fn area(shape: &FillShape) -> i64 {
    let ring = |ring: &[reda_model::geom::Point]| {
        let n = ring.len();
        let twice: i128 = (0..n)
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                i128::from(a.x.0) * i128::from(b.y.0) - i128::from(b.x.0) * i128::from(a.y.0)
            })
            .sum();
        twice.abs() / 2
    };
    let holes: i128 = shape.holes.iter().map(|h| ring(h)).sum();
    (ring(&shape.outer) - holes) as i64
}
