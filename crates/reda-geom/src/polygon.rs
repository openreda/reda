//! Edits on closed contours that the polygon editor shares: the self-intersection guard,
//! and rounding (fillet) or bevelling (chamfer) corners.

use std::fmt;

use crate::grown::arc_points;
use crate::{Millideg, Nm, Point, Vertex};

/// The first point where the contour crosses or touches itself, arcs taken as their
/// chords at the arc tolerance. `None` for a simple contour.
pub fn crossing(vertices: &[Vertex]) -> Option<Point> {
    let ring = flatten(vertices);
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let segment = |i: usize| (ring[i], ring[(i + 1) % n]);
    // Sweep along x: each segment meets only those starting before it ends.
    let mut order: Vec<usize> = (0..n).collect();
    let low = |i: usize| {
        let (a, b) = segment(i);
        a.x.0.min(b.x.0)
    };
    order.sort_by_key(|&i| low(i));
    for (k, &i) in order.iter().enumerate() {
        let (a, b) = segment(i);
        let high = a.x.0.max(b.x.0);
        for &j in &order[k + 1..] {
            if low(j) > high {
                break;
            }
            let (c, d) = segment(j);
            let adjacent = (i + 1) % n == j || (j + 1) % n == i;
            if adjacent {
                // Neighbours share one end; they clash only by folding back on each other.
                let (prev, shared, next) = if (i + 1) % n == j {
                    (a, b, d)
                } else {
                    (c, d, b)
                };
                if folds_back(prev, shared, next) {
                    return Some(shared);
                }
                continue;
            }
            if let Some(p) = meet(a, b, c, d) {
                return Some(p);
            }
        }
    }
    None
}

/// The contour as a ring of points, arcs as chords, with no repeated neighbours.
fn flatten(vertices: &[Vertex]) -> Vec<Point> {
    let n = vertices.len();
    let mut ring: Vec<Point> = Vec::new();
    for (i, v) in vertices.iter().enumerate() {
        let next = vertices[(i + 1) % n].point();
        if v.sweep == Millideg::ZERO {
            ring.push(v.point());
        } else {
            let points = arc_points(v.point(), next, v.sweep);
            ring.extend_from_slice(&points[..points.len() - 1]);
        }
    }
    ring.dedup();
    while ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    ring
}

fn cross(o: Point, a: Point, b: Point) -> i128 {
    let (ax, ay) = (i128::from(a.x.0 - o.x.0), i128::from(a.y.0 - o.y.0));
    let (bx, by) = (i128::from(b.x.0 - o.x.0), i128::from(b.y.0 - o.y.0));
    ax * by - ay * bx
}

fn dot(o: Point, a: Point, b: Point) -> i128 {
    let (ax, ay) = (i128::from(a.x.0 - o.x.0), i128::from(a.y.0 - o.y.0));
    let (bx, by) = (i128::from(b.x.0 - o.x.0), i128::from(b.y.0 - o.y.0));
    ax * bx + ay * by
}

/// Whether the edge `shared`→`next` runs back along `prev`→`shared`.
fn folds_back(prev: Point, shared: Point, next: Point) -> bool {
    cross(shared, prev, next) == 0 && dot(shared, prev, next) > 0
}

/// Whether `p`, known to be on the line through `a` and `b`, lies within the segment.
fn within(a: Point, b: Point, p: Point) -> bool {
    p.x.0 >= a.x.0.min(b.x.0)
        && p.x.0 <= a.x.0.max(b.x.0)
        && p.y.0 >= a.y.0.min(b.y.0)
        && p.y.0 <= a.y.0.max(b.y.0)
}

/// Where segments `a`–`b` and `c`–`d` meet, if they do (touching counts).
fn meet(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let (d1, d2) = (cross(a, b, c), cross(a, b, d));
    let (d3, d4) = (cross(c, d, a), cross(c, d, b));
    if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)) {
        // A proper crossing: a + t (b - a), t = d3 / (d3 - d4).
        let t = d3 as f64 / (d3 - d4) as f64;
        let x = a.x.0 as f64 + t * (b.x.0 - a.x.0) as f64;
        let y = a.y.0 as f64 + t * (b.y.0 - a.y.0) as f64;
        return Some(Point::new(Nm(x.round() as i64), Nm(y.round() as i64)));
    }
    [(d1, a, b, c), (d2, a, b, d), (d3, c, d, a), (d4, c, d, b)]
        .into_iter()
        .find(|&(o, s, e, p)| o == 0 && within(s, e, p))
        .map(|(.., p)| p)
}

/// Why corners cannot be rounded or bevelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CornerError {
    /// No vertex has this index.
    NoVertex(usize),
    /// The vertex is not a corner: its edges run straight on.
    NotACorner(usize),
    /// An edge at the vertex is an arc; only corners between straight edges are changed.
    ArcEdge(usize),
    /// The radius or distance needs more of an edge at the vertex than there is.
    TooLarge(usize),
}

impl fmt::Display for CornerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CornerError::NoVertex(i) => write!(f, "there is no vertex {i}"),
            CornerError::NotACorner(i) => write!(f, "vertex {i} is not a corner"),
            CornerError::ArcEdge(i) => {
                write!(
                    f,
                    "vertex {i} joins an arc; only straight corners can be changed"
                )
            }
            CornerError::TooLarge(i) => {
                write!(f, "the edges at vertex {i} are too short for that size")
            }
        }
    }
}

impl std::error::Error for CornerError {}

/// How a corner is cut.
#[derive(Clone, Copy)]
enum Cut {
    /// An arc of this radius, tangent to both edges.
    Fillet(Nm),
    /// A straight bevel this far along each edge.
    Chamfer(Nm),
}

/// Rounds the corners at `corners` (every corner between straight edges when `None`) with
/// arcs of `radius` tangent to both edges.
pub fn fillet(
    vertices: &[Vertex],
    corners: Option<&[usize]>,
    radius: Nm,
) -> Result<Vec<Vertex>, CornerError> {
    cut(vertices, corners, Cut::Fillet(radius))
}

/// Bevels the corners at `corners` (every corner between straight edges when `None`) with
/// a straight edge from `distance` back along one edge to `distance` along the other.
pub fn chamfer(
    vertices: &[Vertex],
    corners: Option<&[usize]>,
    distance: Nm,
) -> Result<Vec<Vertex>, CornerError> {
    cut(vertices, corners, Cut::Chamfer(distance))
}

/// One corner to cut: the unit directions in and out, the turn and how far along each edge
/// the cut reaches.
struct Corner {
    into: (f64, f64),
    out: (f64, f64),
    turn: f64,
    reach: f64,
}

fn unit(from: Point, to: Point) -> ((f64, f64), f64) {
    let (dx, dy) = ((to.x.0 - from.x.0) as f64, (to.y.0 - from.y.0) as f64);
    let len = dx.hypot(dy);
    ((dx / len, dy / len), len)
}

fn cut(
    vertices: &[Vertex],
    corners: Option<&[usize]>,
    how: Cut,
) -> Result<Vec<Vertex>, CornerError> {
    let n = vertices.len();
    let explicit = corners.is_some();
    let chosen: Vec<usize> = match corners {
        Some(list) => {
            if let Some(&bad) = list.iter().find(|&&i| i >= n) {
                return Err(CornerError::NoVertex(bad));
            }
            list.to_vec()
        }
        None => (0..n).collect(),
    };
    let mut cuts: Vec<Option<Corner>> = (0..n).map(|_| None).collect();
    for i in chosen {
        let prev = vertices[(i + n - 1) % n];
        let (here, next) = (vertices[i], vertices[(i + 1) % n]);
        if prev.sweep != Millideg::ZERO || here.sweep != Millideg::ZERO {
            if explicit {
                return Err(CornerError::ArcEdge(i));
            }
            continue;
        }
        let (into, _) = unit(prev.point(), here.point());
        let (out, _) = unit(here.point(), next.point());
        let turn = (into.0 * out.1 - into.1 * out.0).atan2(into.0 * out.0 + into.1 * out.1);
        if turn.abs() < 1e-9 {
            if explicit {
                return Err(CornerError::NotACorner(i));
            }
            continue;
        }
        let reach = match how {
            Cut::Fillet(r) => r.0 as f64 * (turn.abs() / 2.0).tan(),
            Cut::Chamfer(d) => d.0 as f64,
        };
        cuts[i] = Some(Corner {
            into,
            out,
            turn,
            reach,
        });
    }
    // Each edge must hold the cuts at both of its ends.
    for i in 0..n {
        let j = (i + 1) % n;
        let (_, len) = unit(vertices[i].point(), vertices[j].point());
        let used = |k: usize| cuts[k].as_ref().map_or(0.0, |c| c.reach);
        if used(i) + used(j) > len + 0.5 {
            let at = if cuts[j].is_some() { j } else { i };
            return Err(CornerError::TooLarge(at));
        }
    }
    let round = |p: Point, d: (f64, f64), by: f64| {
        Point::new(
            Nm(p.x.0 + (d.0 * by).round() as i64),
            Nm(p.y.0 + (d.1 * by).round() as i64),
        )
    };
    let mut out: Vec<Vertex> = Vec::with_capacity(n * 2);
    for (i, v) in vertices.iter().enumerate() {
        let Some(c) = &cuts[i] else {
            out.push(*v);
            continue;
        };
        let start = round(v.point(), c.into, -c.reach);
        let end = round(v.point(), c.out, c.reach);
        let sweep = match how {
            Cut::Fillet(_) => Millideg((c.turn.to_degrees() * 1000.0).round() as i64),
            Cut::Chamfer(_) => Millideg::ZERO,
        };
        out.push(Vertex::arc(start, sweep));
        out.push(Vertex::arc(end, v.sweep));
    }
    Ok(drop_empty_edges(out))
}

/// Drops a vertex whose straight edge to the next vertex has no length, as where two cuts
/// use up a whole edge between them.
fn drop_empty_edges(mut vertices: Vec<Vertex>) -> Vec<Vertex> {
    let mut i = 0;
    while vertices.len() > 2 && i < vertices.len() {
        let next = vertices[(i + 1) % vertices.len()];
        if vertices[i].point() == next.point() && vertices[i].sweep == Millideg::ZERO {
            vertices.remove(i);
        } else {
            i += 1;
        }
    }
    vertices
}
