//! What the island settings leave of Zone fills: `dead_copper` for kept pieces of fill
//! that touch no copper of their Net (directly or through other fills), above the
//! `dead_copper` rule's `min_area`, and `empty_zone` for a Zone with a Net and nothing
//! left to fill. Zones with no Net are exempt from both.

use reda_model::{DrcCheck, FillShape, Limit, RuleField, RuleKind, ZoneFills};

use super::{Board, Report};
use crate::copper::Item;
use crate::rules::resolve;

pub(super) fn check(board: &Board<'_>, fills: &ZoneFills, report: &mut Report) {
    if report.is_off(DrcCheck::DeadCopper) && report.is_off(DrcCheck::EmptyZone) {
        return;
    }
    let pcb = board.pcb;
    for zone in pcb.zones().filter(|z| z.net.is_some()) {
        let shapes = fills
            .zones
            .get(&zone.uuid)
            .map_or(&[][..], |f| &f.shapes[..]);
        if shapes.is_empty() {
            report.add(DrcCheck::EmptyZone, |v| {
                v.object(zone.uuid)
                    .located(pcb.uuid, zone.vertices[0].point())
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
