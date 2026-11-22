//! The Plated Edge checks (plated-edges.md#drc): Plated Edges meeting with different
//! Nets, Edge Connector Pads on a Plated Edge, a Plated Edge with no Net, and one that no
//! copper of its Net touches on the outer Layers.

use reda_model::geom::{Millideg, Nm, Point, arc_centre};
use reda_model::{DrcCheck, PadRole};

use crate::plated::{PlatedEdgeRef, board_edges, edges_of};

use super::{Board, Report};
use crate::copper::Item;

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    nets_differ(board, report);
    without_net(board, report);
    unconnected(board, report);
    edge_connector_on_plating(board, report);
}

/// One error per Edge Connector Pad and Plated Edge its copper touches, at the contact,
/// naming the Footprint instance, the Pad and the contour.
fn edge_connector_on_plating(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::EdgeConnectorOnPlatedEdge) || board_edges(board.pcb).is_empty() {
        return;
    }
    let pcb = board.pcb;
    let copper = board.copper;
    for instance in pcb.footprints() {
        let Some(copy) = pcb.footprint_copy(instance.footprint) else {
            continue;
        };
        for pad in (copy.footprint.pads.iter()).filter(|p| p.role == Some(PadRole::EdgeConnector)) {
            let item = Item::Pad {
                footprint: instance.uuid,
                pad: pad.uuid,
            };
            for plating in copper.contacts(item).filter(|i| i.is_plated_edge()) {
                let Item::PlatedEdge { contour, edge } = plating else {
                    continue;
                };
                let edge_pieces = copper.item_pieces(plating);
                let Some((layer, at)) = copper.item_pieces(item).into_iter().find_map(|(l, s)| {
                    let (_, e) = edge_pieces
                        .iter()
                        .find(|(el, e)| *el == l && e.touches(s))?;
                    Some((l, s.closest_points(e)[0]))
                }) else {
                    continue;
                };
                report.add(DrcCheck::EdgeConnectorOnPlatedEdge, |v| {
                    v.located(pcb.uuid, at)
                        .on_layers([layer])
                        .arg("edge", i64::from(edge))
                        .objects([instance.uuid, pad.uuid, contour])
                });
            }
        }
    }
}

/// One warning per Plated Edge with a Net that no copper of that Net touches on the
/// outer copper Layers, at its middle. Plated Edges of one Net meeting at a corner are one
/// piece of metal, so copper touching any of them joins them all.
fn unconnected(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::PlatedEdgeUnconnected) {
        return;
    }
    let pcb = board.pcb;
    let copper = board.copper;
    let outer: Vec<_> = {
        let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
        match layers.as_slice() {
            [] => Vec::new(),
            [only] => vec![*only],
            [first, .., last] => vec![*first, *last],
        }
    };
    let item_of = |edge: &PlatedEdgeRef| Item::PlatedEdge {
        contour: edge.contour,
        edge: edge.edge as u32,
    };
    // Whether copper of the edge's Net, other than plating, touches it on an outer Layer.
    let touched = |item: Item, net| {
        copper.item_pieces(item).into_iter().any(|(layer, shape)| {
            outer.contains(&layer)
                && copper
                    .items_touching(layer, shape)
                    .into_iter()
                    .any(|other| !other.is_plated_edge() && copper.net(other) == Some(net))
        })
    };
    for edge in board_edges(pcb) {
        let Some(net) = edge.net else {
            continue;
        };
        // The plating this edge is one piece of metal with: its Net's Plated Edges
        // reached through each other.
        let mut metal = vec![item_of(&edge)];
        let mut i = 0;
        while i < metal.len() {
            for next in copper.contacts(metal[i]) {
                if next.is_plated_edge() && copper.net(next) == Some(net) && !metal.contains(&next)
                {
                    metal.push(next);
                }
            }
            i += 1;
        }
        if metal.iter().any(|item| touched(*item, net)) {
            continue;
        }
        report.add(DrcCheck::PlatedEdgeUnconnected, |v| {
            v.located(pcb.uuid, middle(&edge))
                .on_layers(outer.iter().copied())
                .arg("edge", edge.edge as i64)
                .arg("net", board.net_name(Some(net)))
                .object(edge.contour)
        });
    }
}

/// The middle of a Plated Edge: of its chord, or of its arc.
fn middle(edge: &PlatedEdgeRef) -> Point {
    if edge.sweep == Millideg::ZERO {
        return Point::new(
            Nm((edge.from.x.0 + edge.to.x.0) / 2),
            Nm((edge.from.y.0 + edge.to.y.0) / 2),
        );
    }
    let c = arc_centre(edge.from, edge.to, edge.sweep);
    let half = (edge.sweep.0 as f64 / 1000.0).to_radians() / 2.0;
    let (dx, dy) = (
        (edge.from.x.0 - c.x.0) as f64,
        (edge.from.y.0 - c.y.0) as f64,
    );
    let (sin, cos) = half.sin_cos();
    Point::new(
        Nm(c.x.0 + (dx * cos - dy * sin).round() as i64),
        Nm(c.y.0 + (dx * sin + dy * cos).round() as i64),
    )
}

/// One warning per Plated Edge with no Net, at its middle.
fn without_net(board: &Board<'_>, report: &mut Report) {
    let copper_layers: Vec<_> = board.pcb.copper_layers().map(|l| l.uuid).collect();
    for edge in board_edges(board.pcb) {
        if edge.net.is_some() {
            continue;
        }
        report.add(DrcCheck::PlatedEdgeWithoutNet, |v| {
            v.located(board.pcb.uuid, middle(&edge))
                .on_layers(copper_layers.iter().copied())
                .arg("edge", edge.edge as i64)
                .object(edge.contour)
        });
    }
}

/// The Plated Edges of each contour, in vertex order, with the contour's vertex count.
fn by_contour(board: &Board<'_>) -> Vec<(usize, Vec<PlatedEdgeRef>)> {
    let pcb = board.pcb;
    let outline = pcb
        .outline()
        .map(|o| (o.vertices.len(), edges_of(o.uuid, &o.vertices, &o.plated)));
    outline
        .into_iter()
        .chain(
            pcb.cutouts()
                .map(|c| (c.vertices.len(), edges_of(c.uuid, &c.vertices, &c.plated))),
        )
        .filter(|(_, edges)| !edges.is_empty())
        .collect()
}

/// One error per vertex where two Plated Edges with different Nets meet, at the vertex.
/// An edge with no Net has its own warning, so it differs from none.
fn nets_differ(board: &Board<'_>, report: &mut Report) {
    let copper_layers: Vec<_> = board.pcb.copper_layers().map(|l| l.uuid).collect();
    for (n, edges) in by_contour(board) {
        for a in &edges {
            let next = (a.edge + 1) % n;
            let Some(b) = edges.iter().find(|b| b.edge == next) else {
                continue;
            };
            let (Some(na), Some(nb)) = (a.net, b.net) else {
                continue;
            };
            if na == nb || n < 2 {
                continue;
            }
            report.add(DrcCheck::PlatedEdgeNetsDiffer, |v| {
                v.located(board.pcb.uuid, a.to)
                    .on_layers(copper_layers.iter().copied())
                    .arg("net_a", board.net_name(Some(na)))
                    .arg("net_b", board.net_name(Some(nb)))
                    .arg("edge_a", a.edge as i64)
                    .arg("edge_b", b.edge as i64)
                    .object(a.contour)
            });
        }
    }
}
