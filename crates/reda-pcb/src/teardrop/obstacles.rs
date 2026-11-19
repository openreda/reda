//! The locally forbidden area. All circular growth uses the Zone filler's
//! circumscribed polygons so its approximation always keeps the rule clearance.
use super::{Junction, percent};
use crate::copper::{Copper, Item};
use crate::drill::DrillHole;
use crate::rules::{clearance_between_on, net_rules, resolve_pair_on};
use crate::zone::{poly, shape_copper};
use reda_command::Fingerprint;
use reda_model::{PcbDocument, Restriction, RuleKind, TeardropFill};

pub(super) struct Obstacles(poly::Area);

impl Obstacles {
    pub(super) fn clears(&self, area: &poly::Area) -> bool {
        !poly::overlaps(area, &self.0)
    }

    pub(super) fn fingerprint(&self, f: &mut Fingerprint) {
        for shape in &self.0 {
            f.add(&shape.len());
            for ring in shape {
                f.add(&ring.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>());
            }
        }
    }
}

pub(super) fn near(
    pcb: &PcbDocument,
    copper: &Copper,
    holes: &[DrillHole],
    j: &Junction<'_>,
    prior: &[TeardropFill],
) -> Obstacles {
    let net = j.track.net;
    let layer = j.track.layer;
    let other_net = |other| net.is_none() || net != other;
    let clearance = |other| {
        clearance_between_on(pcb, net, other, Some(layer))
            .unwrap_or_default()
            .0
            .max(1)
    };
    let edge = net_rules(pcb, net)
        .edge_clearance
        .unwrap_or_default()
        .0
        .max(1);
    let pad = percent(j.size, j.rule.width, j.rule.max_width)
        .0
        .max(j.track.width.0)
        / 2
        + 2;
    let (lo, hi) = j
        .path
        .iter()
        .fold(([i64::MAX; 2], [i64::MIN; 2]), |(lo, hi), p| {
            (
                [lo[0].min(p.x.0 - pad), lo[1].min(p.y.0 - pad)],
                [hi[0].max(p.x.0 + pad), hi[1].max(p.y.0 + pad)],
            )
        });
    let window = poly::polygon(
        vec![lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]]
            .into_iter()
            .map(poly::ip)
            .collect(),
    );
    let mut excluded = Vec::new();
    for (item, shape) in copper.pieces_on(layer) {
        if item == j.target || matches!(item,Item::Track{uuid} if j.path_tracks.contains(&uuid)) {
            continue;
        }
        let other = copper.net(item);
        if other_net(other) {
            let gap = clearance(other);
            if poly::boxes_within((lo, hi), shape.bounds(), gap) {
                excluded.extend(poly::grown(shape, gap));
            }
        }
    }
    for d in prior
        .iter()
        .filter(|d| d.location.layer == layer && d.location.track != j.track.uuid)
    {
        let other = pcb.track(d.location.track).and_then(|t| t.net);
        if other_net(other) {
            let gap = clearance(other);
            for shape in &d.shapes {
                let copper = shape_copper(shape);
                if poly::boxes_within((lo, hi), copper.bounds(), gap) {
                    excluded.extend(poly::grown(&copper, gap));
                }
            }
        }
    }
    for h in holes.iter().filter(|h| h.layers(pcb).contains(&layer)) {
        if h.item == j.target {
            continue;
        }
        let gap = if h.ring.is_none() {
            edge
        } else {
            if !other_net(h.net) && copper.item_pieces(h.item).iter().any(|(l, _)| *l == layer) {
                continue;
            }
            let Some(gap) =
                resolve_pair_on(pcb, net, h.net, Some(layer), RuleKind::Clearance, h.field())
                    .and_then(|s| s.value.length())
            else {
                continue;
            };
            gap.0.max(1)
        };
        if poly::boxes_within((lo, hi), h.shape.bounds(), gap) {
            excluded.extend(poly::grown(&h.shape, gap));
        }
    }
    let gap = if crate::keepout::zero_clearance(pcb) {
        0
    } else {
        clearance(None)
    };
    for keepout in pcb
        .keepouts()
        .filter(|k| k.on_layer(layer) && k.restricts(Restriction::Tracks))
    {
        excluded.extend(crate::keepout::grown(keepout, gap));
    }
    if let Some(board) = pcb.outline() {
        let outline = poly::polygon(poly::contour(&board.vertices));
        excluded.extend(poly::difference(&window, &outline));
        excluded.extend(poly::edge_band(&outline, edge));
    }
    for cutout in pcb.cutouts() {
        let cut = poly::polygon(poly::contour(&cutout.vertices));
        excluded.extend(poly::dilated(&cut, edge));
    }
    Obstacles(poly::intersection(&window, &poly::union(excluded)))
}
