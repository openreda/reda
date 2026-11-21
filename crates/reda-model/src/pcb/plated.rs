//! Plated Edges: edges of the Board Outline or a Board Cutout that the fab plates with
//! copper (docs/spec/v0.2.5/plated-edges.md).
//!
//! A stored vertex holds the edge that leaves it, so the mark is stored there as
//! `plated: {net?}`. In memory a contour keeps its geometry as plain [`Vertex`]es and its
//! marks beside them, keyed by the index of the vertex that starts the edge
//! ([`PlatedEdges`]); [`ContourRow`] is the file's form that joins the two.
//!
//! The functions here carry the marks through the contour edits of the spec's edit table:
//! inserting a vertex ([`after_insert`]), deleting one ([`after_delete`]), and cutting
//! corners ([`after_cut`]). Moving vertices keeps the marks as they are, and replacing the
//! whole contour keeps only the marks the new vertices carry.

use std::collections::BTreeMap;

use reda_geom::{Millideg, Nm, Point, Vertex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// The mark on a Plated Edge: the Net its plating carries, if any.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct PlatedEdge {
    /// The Net the plating carries; left out for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
}

/// The Plated Edges of one contour, by the index of the vertex that starts each.
pub type PlatedEdges = BTreeMap<usize, PlatedEdge>;

/// One vertex of a Board Outline or Board Cutout as the file stores it: a [`Vertex`] and
/// the mark of the edge that leaves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardVertex {
    pub x: Nm,
    pub y: Nm,
    /// The edge to the next vertex: left out (or 0) for a straight edge, otherwise an arc
    /// sweeping this angle, counter-clockwise when positive.
    #[serde(default, skip_serializing_if = "is_straight")]
    pub sweep: Millideg,
    /// Set when the edge to the next vertex is a Plated Edge; left out otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plated: Option<PlatedEdge>,
}

fn is_straight(sweep: &Millideg) -> bool {
    *sweep == Millideg::ZERO
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The file row of a Board Outline or a Board Cutout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContourRow {
    pub uuid: Uuid,
    /// Locked against geometry changes; left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// The contour; the last vertex joins the first.
    pub vertices: Vec<BoardVertex>,
}

/// The file's vertices of a contour with `marks`.
pub fn join(vertices: &[Vertex], marks: &PlatedEdges) -> Vec<BoardVertex> {
    vertices
        .iter()
        .enumerate()
        .map(|(i, v)| BoardVertex {
            x: v.x,
            y: v.y,
            sweep: v.sweep,
            plated: marks.get(&i).copied(),
        })
        .collect()
}

/// The geometry and the marks of the file's `vertices`.
pub fn split(vertices: &[BoardVertex]) -> (Vec<Vertex>, PlatedEdges) {
    let geometry = vertices
        .iter()
        .map(|v| Vertex::arc(Point::new(v.x, v.y), v.sweep))
        .collect();
    let marks = vertices
        .iter()
        .enumerate()
        .filter_map(|(i, v)| Some((i, v.plated?)))
        .collect();
    (geometry, marks)
}

/// The marks after a vertex is inserted at index `at`, splitting the edge that started at
/// `at - 1`: both halves keep its mark.
pub fn after_insert(marks: &PlatedEdges, at: usize, len_before: usize) -> PlatedEdges {
    let split = (at + len_before - 1) % len_before.max(1);
    let mut out: PlatedEdges = marks
        .iter()
        .map(|(&i, &m)| (if i >= at { i + 1 } else { i }, m))
        .collect();
    let before = if split >= at { split + 1 } else { split };
    if let Some(&m) = out.get(&before) {
        out.insert(at, m);
    }
    out
}

/// The marks after the vertex at `at` is deleted, joining the edges either side of it: the
/// joined edge takes the mark of the edge before the deleted vertex.
pub fn after_delete(marks: &PlatedEdges, at: usize) -> PlatedEdges {
    marks
        .iter()
        .filter(|(i, _)| **i != at)
        .map(|(&i, &m)| (if i > at { i - 1 } else { i }, m))
        .collect()
}

/// How far a point may sit off an edge and still be on it, in nm: corner cuts round their
/// new vertices to the nanometre.
const ON_EDGE: f64 = 2.0;

/// Whether `p` lies on the straight edge `a`–`b`.
fn on_straight(a: Point, b: Point, p: Point) -> bool {
    if p == a || p == b {
        return true;
    }
    let (dx, dy) = ((b.x.0 - a.x.0) as f64, (b.y.0 - a.y.0) as f64);
    let (px, py) = ((p.x.0 - a.x.0) as f64, (p.y.0 - a.y.0) as f64);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return false;
    }
    let off = (dx * py - dy * px) / len;
    let along = (dx * px + dy * py) / len;
    off.abs() <= ON_EDGE && along >= -ON_EDGE && along <= len + ON_EDGE
}

/// The marks of `new`, the contour `old` (with `marks`) with some corners cut by a fillet
/// or a chamfer. An edge that runs along an old edge keeps its mark. A cut across a corner
/// is plated when both edges it joins were, and takes the mark of the edge into the corner.
pub fn after_cut(old: &[Vertex], marks: &PlatedEdges, new: &[Vertex]) -> PlatedEdges {
    let n = old.len();
    let m = new.len();
    if n == 0 || m == 0 {
        return PlatedEdges::new();
    }
    let old_edge = |i: usize| (old[i], old[(i + 1) % n]);
    // The old edge holding the new edge from `a` (sweep `sweep`) to `b`, or the two old
    // edges a corner cut joins.
    let holding = |a: Point, b: Point, sweep: Millideg| -> Option<(usize, Option<usize>)> {
        for i in 0..n {
            let (s, e) = old_edge(i);
            if s.sweep != Millideg::ZERO {
                if s.point() == a && e.point() == b && s.sweep == sweep {
                    return Some((i, None));
                }
                continue;
            }
            if sweep == Millideg::ZERO
                && on_straight(s.point(), e.point(), a)
                && on_straight(s.point(), e.point(), b)
            {
                return Some((i, None));
            }
        }
        // A cut: `a` on the edge into a corner, `b` on the edge out of it.
        (0..n).find_map(|i| {
            let j = (i + 1) % n;
            let (s, e) = old_edge(i);
            let (_, f) = old_edge(j);
            (s.sweep == Millideg::ZERO
                && e.sweep == Millideg::ZERO
                && on_straight(s.point(), e.point(), a)
                && on_straight(e.point(), f.point(), b))
            .then_some((i, Some(j)))
        })
    };
    let mut out = PlatedEdges::new();
    for k in 0..m {
        let (a, b) = (new[k], new[(k + 1) % m]);
        let mark = match holding(a.point(), b.point(), a.sweep) {
            Some((i, None)) => marks.get(&i).copied(),
            Some((i, Some(j))) if marks.contains_key(&j) => marks.get(&i).copied(),
            _ => None,
        };
        if let Some(mark) = mark {
            out.insert(k, mark);
        }
    }
    out
}

/// One Plated Edge of the board, for drawing and for the fab outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatedEdgeRef {
    /// The Board Outline or Board Cutout.
    pub contour: Uuid,
    /// The index of the vertex that starts the edge.
    pub edge: usize,
    pub from: Point,
    pub to: Point,
    /// The edge's arc sweep; zero for a straight edge.
    pub sweep: Millideg,
    pub net: Option<Uuid>,
}

/// The Plated Edges of a contour, in vertex order.
pub fn edges_of(contour: Uuid, vertices: &[Vertex], marks: &PlatedEdges) -> Vec<PlatedEdgeRef> {
    let n = vertices.len();
    marks
        .iter()
        .filter(|(i, _)| **i < n)
        .map(|(&i, m)| PlatedEdgeRef {
            contour,
            edge: i,
            from: vertices[i].point(),
            to: vertices[(i + 1) % n].point(),
            sweep: vertices[i].sweep,
            net: m.net,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Vertex> {
        [(0, 0), (10, 0), (10, 10), (0, 10)]
            .into_iter()
            .map(|(x, y)| Vertex::at(Point::new(Nm(x * 1_000_000), Nm(y * 1_000_000))))
            .collect()
    }

    fn marked(edges: &[usize]) -> PlatedEdges {
        edges.iter().map(|&i| (i, PlatedEdge::default())).collect()
    }

    fn keys(marks: &PlatedEdges) -> Vec<usize> {
        marks.keys().copied().collect()
    }

    #[test]
    fn inserting_a_vertex_splits_the_edge_and_both_halves_keep_the_mark() {
        // Edge 1 (vertex 1 → 2) is plated; a vertex goes in at index 2.
        assert_eq!(keys(&after_insert(&marked(&[1]), 2, 4)), [1, 2]);
        // An unplated edge split leaves the plated one, shifted.
        assert_eq!(keys(&after_insert(&marked(&[2]), 1, 4)), [3]);
        // Splitting the closing edge (vertex 3 → 0) appends after vertex 3.
        assert_eq!(keys(&after_insert(&marked(&[3]), 4, 4)), [3, 4]);
    }

    #[test]
    fn deleting_a_vertex_gives_the_joined_edge_the_mark_of_the_edge_before() {
        // Edge 0 plated, edge 1 not: deleting vertex 1 joins them as a plated edge 0.
        assert_eq!(keys(&after_delete(&marked(&[0]), 1)), [0]);
        // Edge 1 plated, edge 0 not: the joined edge 0 is not plated.
        assert_eq!(keys(&after_delete(&marked(&[1]), 1)), Vec::<usize>::new());
        assert_eq!(keys(&after_delete(&marked(&[2, 3]), 1)), [1, 2]);
    }

    #[test]
    fn a_corner_cut_keeps_the_marks_along_its_edges_and_plates_the_cut_between_two_plated_edges() {
        let old = square();
        let new = reda_geom::chamfer(&old, Some(&[1]), Nm(1_000_000)).unwrap();
        // Vertices now: 0, cut start, cut end, 2, 3; edges 0 → cut start, cut, cut end → 2.
        assert_eq!(keys(&after_cut(&old, &marked(&[0, 1]), &new)), [0, 1, 2]);
        assert_eq!(keys(&after_cut(&old, &marked(&[0]), &new)), [0]);
        assert_eq!(keys(&after_cut(&old, &marked(&[3]), &new)), [4]);
        let filleted = reda_geom::fillet(&old, None, Nm(2_000_000)).unwrap();
        assert_eq!(filleted.len(), 8);
        assert_eq!(
            keys(&after_cut(&old, &marked(&[0, 1, 2, 3]), &filleted)),
            (0..8).collect::<Vec<_>>()
        );
    }
}
