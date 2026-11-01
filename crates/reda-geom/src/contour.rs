//! Closed contours, such as a Board Outline or a Board Cutout: a list of vertices joined by
//! straight or arc edges, the last vertex joining back to the first.
//!
//! An arc edge is stored as the sweep angle from its vertex to the next one, so it stays
//! exact in integers; its centre and radius are derived when geometry is computed.

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Millideg, Nm, Point};

/// One corner of a contour and the edge that leaves it towards the next vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vertex {
    pub x: Nm,
    pub y: Nm,
    /// The edge to the next vertex: left out (or 0) for a straight edge, otherwise an arc
    /// sweeping this angle, counter-clockwise when positive.
    #[serde(default, skip_serializing_if = "is_straight")]
    pub sweep: Millideg,
}

fn is_straight(sweep: &Millideg) -> bool {
    *sweep == Millideg::ZERO
}

impl Vertex {
    /// A vertex whose outgoing edge is straight.
    pub const fn at(point: Point) -> Vertex {
        Vertex::arc(point, Millideg::ZERO)
    }

    /// A vertex whose outgoing edge is an arc of `sweep`.
    pub const fn arc(point: Point, sweep: Millideg) -> Vertex {
        Vertex {
            x: point.x,
            y: point.y,
            sweep,
        }
    }

    pub const fn point(&self) -> Point {
        Point::new(self.x, self.y)
    }
}

/// A loose straight line or arc, such as a drawn graphic line, to be chained into a
/// contour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Segment {
    pub from: Point,
    pub to: Point,
    /// 0 for a straight line, otherwise the arc's sweep from `from` to `to`.
    pub sweep: Millideg,
}

impl Segment {
    pub const fn line(from: Point, to: Point) -> Segment {
        Segment::arc(from, to, Millideg::ZERO)
    }

    pub const fn arc(from: Point, to: Point, sweep: Millideg) -> Segment {
        Segment { from, to, sweep }
    }

    fn reversed(self) -> Segment {
        Segment::arc(self.to, self.from, -self.sweep)
    }
}

/// Why vertices or segments do not make one closed contour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContourError {
    /// Fewer than three vertices, or two joined only by straight edges.
    TooFewVertices,
    /// Two consecutive vertices at the same point.
    ZeroLengthEdge(Point),
    /// An arc that sweeps a full turn or more.
    BadSweep(Point),
    /// A segment end that no other segment meets.
    OpenEnd(Point),
    /// Three or more segment ends meet at this point.
    Branch(Point),
    /// The segments close into more than one loop.
    SeveralLoops,
}

impl fmt::Display for ContourError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = |p: &Point| format!("({} nm, {} nm)", p.x.0, p.y.0);
        match self {
            ContourError::TooFewVertices => f.write_str(
                "a closed contour needs at least three vertices, or two joined by an arc",
            ),
            ContourError::ZeroLengthEdge(p) => {
                write!(f, "two consecutive vertices are both at {}", at(p))
            }
            ContourError::BadSweep(p) => write!(
                f,
                "the arc from {} sweeps a full turn or more, or nothing",
                at(p)
            ),
            ContourError::OpenEnd(p) => write!(f, "the segments do not close: {} is open", at(p)),
            ContourError::Branch(p) => {
                write!(f, "three or more segment ends meet at {}", at(p))
            }
            ContourError::SeveralLoops => f.write_str("the segments form more than one loop"),
        }
    }
}

impl std::error::Error for ContourError {}

/// Checks that `vertices` make a closed contour: at least three vertices (or two with an
/// arc between them), no zero-length edge, and every arc sweeping less than a full turn.
/// Self-intersection is not checked.
pub fn check_contour(vertices: &[Vertex]) -> Result<(), ContourError> {
    let has_arc = vertices.iter().any(|v| !is_straight(&v.sweep));
    if vertices.len() < 2 || (vertices.len() < 3 && !has_arc) {
        return Err(ContourError::TooFewVertices);
    }
    for (i, v) in vertices.iter().enumerate() {
        let next = vertices[(i + 1) % vertices.len()];
        if v.point() == next.point() {
            return Err(ContourError::ZeroLengthEdge(v.point()));
        }
        if v.sweep.0.abs() >= Millideg::TURN.0 {
            return Err(ContourError::BadSweep(v.point()));
        }
    }
    Ok(())
}

/// A `width` × `height` rectangle from the origin, counter-clockwise, with each corner
/// rounded by a quarter arc of `corner_radius` (0 for sharp corners). A side the corners
/// use up entirely is left out. The caller checks `2 × corner_radius` fits both sides.
pub fn rounded_rect(width: Nm, height: Nm, corner_radius: Nm) -> Vec<Vertex> {
    let (w, h, r) = (width, height, corner_radius);
    let p = |x: Nm, y: Nm| Point::new(x, y);
    if r == Nm::ZERO {
        return [
            p(Nm::ZERO, Nm::ZERO),
            p(w, Nm::ZERO),
            p(w, h),
            p(Nm::ZERO, h),
        ]
        .into_iter()
        .map(Vertex::at)
        .collect();
    }
    let quarter = Millideg::from_deg(90);
    let corners = [
        (p(r, Nm::ZERO), p(w - r, Nm::ZERO)),
        (p(w, r), p(w, h - r)),
        (p(w - r, h), p(r, h)),
        (p(Nm::ZERO, h - r), p(Nm::ZERO, r)),
    ];
    let mut out = Vec::with_capacity(8);
    for (side_start, side_end) in corners {
        // A side the corner arcs use up has no straight part: its start is its end.
        if side_start != side_end {
            out.push(Vertex::at(side_start));
        }
        out.push(Vertex::arc(side_end, quarter));
    }
    out
}

/// Chains loose segments, given in any order and either direction, into one closed
/// contour. Ends must meet exactly. The contour starts at the first segment and follows its
/// direction.
pub fn chain(segments: &[Segment]) -> Result<Vec<Vertex>, ContourError> {
    let Some(first) = segments.first() else {
        return Err(ContourError::TooFewVertices);
    };
    let mut ends: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    for s in segments {
        for p in [s.from, s.to] {
            *ends.entry((p.x.0, p.y.0)).or_default() += 1;
        }
    }
    let point = |(x, y): (i64, i64)| Point::new(Nm(x), Nm(y));
    // Report problems in a stable order: open ends first, then branches.
    if let Some((&key, _)) = ends.iter().find(|(_, n)| *n % 2 == 1) {
        return Err(ContourError::OpenEnd(point(key)));
    }
    if let Some((&key, _)) = ends.iter().find(|(_, n)| **n > 2) {
        return Err(ContourError::Branch(point(key)));
    }

    let mut used = vec![false; segments.len()];
    used[0] = true;
    let mut out = vec![Vertex::arc(first.from, first.sweep)];
    let mut end = first.to;
    while end != first.from {
        let next = (0..segments.len())
            .filter(|&i| !used[i])
            .find_map(|i| {
                let s = segments[i];
                if s.from == end {
                    Some((i, s))
                } else if s.to == end {
                    Some((i, s.reversed()))
                } else {
                    None
                }
            })
            .ok_or(ContourError::OpenEnd(end))?;
        used[next.0] = true;
        out.push(Vertex::arc(next.1.from, next.1.sweep));
        end = next.1.to;
    }
    if used.iter().any(|u| !u) {
        return Err(ContourError::SeveralLoops);
    }
    check_contour(&out)?;
    Ok(out)
}
