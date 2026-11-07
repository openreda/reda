//! The board-edge check: copper closer to the Board Outline or a Board Cutout than the
//! edge clearance, or outside the board altogether.

use reda_model::geom::{Millideg, Nm, Vertex};
use reda_model::{DrcCheck, Limit, RuleField, RuleKind, Uuid};

use super::{Board, Report, item_objects};
use crate::copper::Item;
use crate::copper::shape::{Core, P, Prim, Shape, arc_points};
use crate::rules::{net_rules, resolve};

/// One edge of the board: the outline or a cutout, as a closed chain of corners (arcs as
/// chords within 1 µm).
struct Edge {
    uuid: Uuid,
    corners: Vec<P>,
    /// Whether copper belongs inside it (the outline) or outside it (a cutout).
    keeps_inside: bool,
}

impl Edge {
    fn new(uuid: Uuid, vertices: &[Vertex], keeps_inside: bool) -> Edge {
        let n = vertices.len();
        let mut corners = Vec::new();
        for (i, v) in vertices.iter().enumerate() {
            let next = vertices[(i + 1) % n].point();
            if v.sweep == Millideg::ZERO {
                corners.push([v.x.0, v.y.0]);
            } else {
                let points = arc_points(v.point(), next, v.sweep);
                corners.extend(points[..points.len() - 1].iter().map(|p| [p.x.0, p.y.0]));
            }
        }
        Edge {
            uuid,
            corners,
            keeps_inside,
        }
    }

    /// The edge's segments, each as a line of no width.
    fn segments(&self) -> impl Iterator<Item = Shape> + '_ {
        let n = self.corners.len();
        (0..n).map(move |i| Shape {
            prims: vec![Prim {
                core: Core::Seg(self.corners[i], self.corners[(i + 1) % n]),
                radius: 0,
            }],
        })
    }

    /// Whether `p` is inside the closed chain (even-odd).
    fn contains(&self, p: P) -> bool {
        let n = self.corners.len();
        let mut inside = false;
        for i in 0..n {
            let (a, b) = (self.corners[i], self.corners[(i + 1) % n]);
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let t = (p[1] - a[1]) as f64 / (b[1] - a[1]) as f64;
                if (p[0] as f64) < a[0] as f64 + t * (b[0] - a[0]) as f64 {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::EdgeClearance) {
        return;
    }
    let pcb = board.pcb;
    let edges: Vec<Edge> = (pcb.outline().map(|o| Edge::new(o.uuid, &o.vertices, true)))
        .into_iter()
        .chain(pcb.cutouts().map(|c| Edge::new(c.uuid, &c.vertices, false)))
        .filter(|e| e.corners.len() >= 3)
        .collect();
    if edges.is_empty() {
        return;
    }
    for item in board.copper.items() {
        // A current Zone fill keeps its edge clearance by construction.
        if matches!(item, Item::Zone { .. }) {
            continue;
        }
        let Some(required) = net_rules(pcb, board.copper.net(item)).edge_clearance else {
            continue;
        };
        let pieces = board.copper.item_pieces(item);
        let Some((_, shape)) = pieces.first() else {
            continue;
        };
        for edge in &edges {
            let nearest = edge
                .segments()
                .min_by(|a, b| shape.distance(a).total_cmp(&shape.distance(b)))
                .expect("edge has segments");
            let gap = shape.distance(&nearest);
            let [a, b] = shape.closest_points(&nearest);
            let anchor = match &shape.prims[0].core {
                Core::Seg(a, _) => *a,
                Core::Poly(points) => points[0],
            };
            let wrong_side = edge.contains(anchor) != edge.keeps_inside;
            let actual = if wrong_side {
                Nm::ZERO
            } else if gap < required.0 as f64 {
                Nm(gap.max(0.0).floor() as i64)
            } else {
                continue;
            };
            report.add(DrcCheck::EdgeClearance, |v| {
                v.length(
                    actual,
                    required,
                    Limit::Min,
                    resolve(
                        pcb,
                        board.copper.net(item),
                        RuleKind::EdgeClearance,
                        RuleField::Value,
                    )
                    .expect("edge limit applies")
                    .rule,
                )
                .distance_points(a, b)
                .on_layers(pieces.iter().map(|(l, _)| *l))
                .arg("required", required)
                .arg("actual", actual)
                .objects(item_objects(item))
                .object(edge.uuid)
            });
        }
    }
}
