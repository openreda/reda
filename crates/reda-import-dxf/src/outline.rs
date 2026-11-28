//! The Board Outline row through the loop builder: its lines and arcs, on the board in
//! nanometres, chained into closed loops with ends that meet within `close_gaps`. The loop
//! with the largest area is the Board Outline; loops wholly inside it are Board Cutouts.
//! Everything else stays a graphic, with the reason: a loop outside the outline or inside
//! a cutout, one that crosses itself or another loop, and every open shape, whose gaps
//! are reported with their size.
//!
//! As `pcb.board_outline.add_from_graphics` does, each group of ends within the tolerance
//! of one another moves to the lowest of them (by x, then y). Nothing is guessed: a shape
//! that doesn't close is never closed by the builder.

use std::collections::{BTreeMap, HashMap};

use reda_model::geom::{self, Millideg, Nm, Point, Segment, Vertex};
use schemars::JsonSchema;
use serde::Serialize;

/// Open ends past this many are paired with the other end of their own shape only; the
/// nearest-pair search over all of them is quadratic.
const PAIRED_ENDS: usize = 2_000;

/// A gap or crossing in the Board Outline row, before anything is imported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Problem {
    pub kind: ProblemKind,
    /// The gap's middle, or where the shape crosses or touches itself or another loop.
    pub at: Point,
    /// How far apart the gap's two ends are; left out for a crossing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<Nm>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    /// Two ends that should meet are further apart than `close_gaps`.
    Gap,
    /// A shape crosses or touches itself, three ends meet, or a loop crosses one kept.
    Crossing,
}

/// Why a piece of the Board Outline row stays a graphic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stray {
    /// Part of a shape that doesn't close.
    Open,
    /// Part of a loop that crosses or touches itself or a kept loop.
    Crossing,
    /// Part of a loop that encloses no area.
    Flat,
    /// Part of a loop outside the Board Outline.
    Outside,
    /// Part of a loop inside a Board Cutout.
    InCutout,
}

/// What the loop builder makes of the row.
#[derive(Debug, Default)]
pub struct Loops {
    /// The Board Outline's contour, counter-clockwise; `None` when nothing closes.
    pub outline: Option<Vec<Vertex>>,
    /// The Board Cutouts' contours, counter-clockwise, largest first.
    pub cutouts: Vec<Vec<Vertex>>,
    /// The segments that stay graphics, by index, with the reason.
    pub strays: BTreeMap<usize, Stray>,
    /// Gaps first, then crossings, each by position.
    pub problems: Vec<Problem>,
}

/// Builds the Board Outline and Board Cutouts from `segments`, ends meeting within
/// `close_gaps`.
pub fn build(segments: &[Segment], close_gaps: Nm) -> Loops {
    let mut out = Loops::default();
    let snapped = snap(segments, close_gaps);

    // Nodes and their segments.
    let mut node_of: HashMap<Point, usize> = HashMap::new();
    let mut points: Vec<Point> = Vec::new();
    let mut node = |p: Point| {
        *node_of.entry(p).or_insert_with(|| {
            points.push(p);
            points.len() - 1
        })
    };
    // (segment index, from node, to node, sweep)
    let mut edges = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        let (from, to) = (snapped[i].0, snapped[i].1);
        if from == to {
            // A line within the tolerance is part of the gap it closes; an arc whose
            // ends meet stays as drawn.
            if s.sweep != Millideg::ZERO {
                out.strays.insert(i, Stray::Open);
            }
            continue;
        }
        edges.push((i, node(from), node(to), s.sweep));
    }
    let mut at_node: Vec<Vec<usize>> = vec![Vec::new(); points.len()];
    for (e, &(_, a, b, _)) in edges.iter().enumerate() {
        at_node[a].push(e);
        at_node[b].push(e);
    }

    // Shapes: edges joined at nodes.
    let mut shape_of = vec![usize::MAX; edges.len()];
    let mut shapes: Vec<Vec<usize>> = Vec::new();
    for start in 0..edges.len() {
        if shape_of[start] != usize::MAX {
            continue;
        }
        let id = shapes.len();
        let mut members = vec![start];
        shape_of[start] = id;
        let mut k = 0;
        while k < members.len() {
            let (_, a, b, _) = edges[members[k]];
            for n in [a, b] {
                for &e in &at_node[n] {
                    if shape_of[e] == usize::MAX {
                        shape_of[e] = id;
                        members.push(e);
                    }
                }
            }
            k += 1;
        }
        shapes.push(members);
    }

    let mut gaps = Vec::new();
    let mut crossings = Vec::new();
    let mut open_ends: Vec<(Point, usize)> = Vec::new();
    let mut branches: Vec<Point> = Vec::new();
    let mut loops: Vec<(Vec<Vertex>, Vec<usize>, f64)> = Vec::new();
    for (id, members) in shapes.iter().enumerate() {
        let mut nodes: Vec<usize> = members
            .iter()
            .flat_map(|&e| [edges[e].1, edges[e].2])
            .collect();
        nodes.sort_unstable();
        nodes.dedup();
        let mut closed = true;
        for &n in &nodes {
            match at_node[n].len() {
                2 => {}
                1 => {
                    closed = false;
                    open_ends.push((points[n], id));
                }
                _ => {
                    closed = false;
                    branches.push(points[n]);
                }
            }
        }
        let segment_ids = || members.iter().map(|&e| edges[e].0);
        if !closed {
            for i in segment_ids() {
                out.strays.insert(i, Stray::Open);
            }
            continue;
        }
        let vertices = walk(&edges, &at_node, members, &points);
        let flat = geom::check_contour(&vertices).is_err();
        // A loop crossing itself, such as a bow tie, may enclose no area on balance.
        if let Some(at) = geom::crossing(&vertices).filter(|_| !flat) {
            crossings.push(at);
            for i in segment_ids() {
                out.strays.insert(i, Stray::Crossing);
            }
            continue;
        }
        let area = area(&vertices);
        if flat || area.abs() < 1.0 {
            for i in segment_ids() {
                out.strays.insert(i, Stray::Flat);
            }
            continue;
        }
        let vertices = if area < 0.0 {
            reversed(&vertices)
        } else {
            vertices
        };
        loops.push((vertices, segment_ids().collect(), area.abs()));
    }
    crossings.extend(branches.iter().copied());
    gaps.extend(pair_ends(open_ends, &branches));

    // The largest loop is the outline; the rest, largest first, are cutouts when wholly
    // inside it and in no cutout.
    loops.sort_by(|a, b| b.2.total_cmp(&a.2));
    let mut kept: Vec<Ring> = Vec::new();
    for (vertices, ids, _) in loops {
        let ring = Ring::of(&vertices);
        let mut why = None;
        if let Some(outline) = kept.first() {
            if let Some(at) = kept.iter().filter_map(|k| k.meets(&ring)).min_by_key(key) {
                crossings.push(at);
                why = Some(Stray::Crossing);
            } else if !outline.holds(&ring) {
                why = Some(Stray::Outside);
            } else if kept[1..].iter().any(|c| c.holds(&ring)) {
                why = Some(Stray::InCutout);
            }
        }
        match why {
            Some(why) => {
                for i in ids {
                    out.strays.insert(i, why);
                }
            }
            None => {
                kept.push(ring);
                if out.outline.is_none() {
                    out.outline = Some(vertices);
                } else {
                    out.cutouts.push(vertices);
                }
            }
        }
    }

    gaps.sort_by_key(|p: &Problem| key(&p.at));
    crossings.sort_by_key(key);
    crossings.dedup();
    out.problems = gaps;
    out.problems.extend(crossings.into_iter().map(|at| Problem {
        kind: ProblemKind::Crossing,
        at,
        size: None,
    }));
    out
}

fn key(p: &Point) -> (i64, i64) {
    (p.x.0, p.y.0)
}

/// Each segment's ends after snapping: every group of ends within `tolerance` of one
/// another, transitively, moves to its lowest end.
fn snap(segments: &[Segment], tolerance: Nm) -> Vec<(Point, Point)> {
    let mut ends: Vec<Point> = segments.iter().flat_map(|s| [s.from, s.to]).collect();
    ends.sort_by_key(key);
    ends.dedup();
    let mut parent: Vec<usize> = (0..ends.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    if tolerance > Nm::ZERO {
        // Buckets a tolerance wide: near ends are in the same bucket or a neighbour.
        let cell = tolerance.0;
        let bucket = |p: Point| (p.x.0.div_euclid(cell), p.y.0.div_euclid(cell));
        let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, &p) in ends.iter().enumerate() {
            let (bx, by) = bucket(p);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let Some(near) = buckets.get(&(bx + dx, by + dy)) else {
                        continue;
                    };
                    for &j in near {
                        if distance(p, ends[j]) <= tolerance.0 as f64 {
                            let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                            // The lower index is the lower end: the root stays lowest.
                            parent[a.max(b)] = a.min(b);
                        }
                    }
                }
            }
            buckets.entry((bx, by)).or_default().push(i);
        }
    }
    let mut to_root = |p: Point| {
        let i = ends.binary_search_by_key(&key(&p), key).expect("listed");
        ends[root(&mut parent, i)]
    };
    segments
        .iter()
        .map(|s| (to_root(s.from), to_root(s.to)))
        .collect()
}

fn distance(a: Point, b: Point) -> f64 {
    ((a.x.0 - b.x.0) as f64).hypot((a.y.0 - b.y.0) as f64)
}

/// The closed shape `members` (every node meeting two edges) as a contour, from the end of
/// its first edge that comes first, along that edge.
fn walk(
    edges: &[(usize, usize, usize, Millideg)],
    at_node: &[Vec<usize>],
    members: &[usize],
    points: &[Point],
) -> Vec<Vertex> {
    let first = members[0];
    let (_, start, mut here, sweep) = edges[first];
    let mut out = vec![Vertex::arc(points[start], sweep)];
    let mut came = first;
    while here != start {
        let next = at_node[here]
            .iter()
            .copied()
            .find(|&e| e != came)
            .expect("two edges at every node");
        let (_, a, b, sweep) = edges[next];
        let (to, sweep) = if a == here { (b, sweep) } else { (a, -sweep) };
        out.push(Vertex::arc(points[here], sweep));
        came = next;
        here = to;
    }
    out
}

/// The signed area a contour encloses, counter-clockwise positive, arcs exact.
fn area(vertices: &[Vertex]) -> f64 {
    let n = vertices.len();
    let mut twice = 0.0;
    let mut bulges = 0.0;
    for (i, v) in vertices.iter().enumerate() {
        let (a, b) = (v.point(), vertices[(i + 1) % n].point());
        let (ax, ay, bx, by) = (a.x.0 as f64, a.y.0 as f64, b.x.0 as f64, b.y.0 as f64);
        twice += ax * by - bx * ay;
        if v.sweep != Millideg::ZERO {
            // The circular segment between the chord and the arc: r²/2 (θ − sin θ).
            let theta = (v.sweep.0 as f64 / 1000.0).to_radians();
            let chord = distance(a, b);
            let r = chord / (2.0 * (theta.abs() / 2.0).sin());
            bulges += r * r / 2.0 * (theta - theta.sin());
        }
    }
    twice / 2.0 + bulges
}

/// The same contour traced the other way round.
fn reversed(vertices: &[Vertex]) -> Vec<Vertex> {
    let n = vertices.len();
    (0..n)
        .rev()
        .map(|i| Vertex::arc(vertices[(i + 1) % n].point(), -vertices[i].sweep))
        .collect()
}

/// Open ends as gaps: the closest pairs first. With too many ends to pair them all, each
/// shape's ends pair among themselves. An end left over (beside three ends meeting) pairs
/// with the nearest such meeting.
fn pair_ends(mut ends: Vec<(Point, usize)>, branches: &[Point]) -> Vec<Problem> {
    let gap = |a: Point, b: Point| Problem {
        kind: ProblemKind::Gap,
        at: Point::new(
            Nm((a.x.0 + b.x.0).div_euclid(2)),
            Nm((a.y.0 + b.y.0).div_euclid(2)),
        ),
        size: Some(Nm(distance(a, b).round() as i64)),
    };
    let mut out = Vec::new();
    let mut paired = vec![false; ends.len()];
    if ends.len() <= PAIRED_ENDS {
        let mut pairs = Vec::new();
        for i in 0..ends.len() {
            for j in i + 1..ends.len() {
                pairs.push((distance(ends[i].0, ends[j].0), i, j));
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then((a.1, a.2).cmp(&(b.1, b.2))));
        for (_, i, j) in pairs {
            if !paired[i] && !paired[j] {
                paired[i] = true;
                paired[j] = true;
                out.push(gap(ends[i].0, ends[j].0));
            }
        }
    } else {
        ends.sort_by_key(|(p, shape)| (*shape, key(p)));
        for pair in ends.chunks(2) {
            if let [(a, s), (b, t)] = pair
                && s == t
            {
                out.push(gap(*a, *b));
            }
        }
        paired = vec![true; ends.len()];
    }
    for (i, (p, _)) in ends.iter().enumerate() {
        if paired[i] {
            continue;
        }
        if let Some(b) = branches
            .iter()
            .copied()
            .min_by(|a, b| distance(*p, *a).total_cmp(&distance(*p, *b)))
        {
            out.push(gap(*p, b));
        }
    }
    out
}

/// A loop as a ring of points, arcs as chords within the arc tolerance, with its bounds.
struct Ring {
    points: Vec<Point>,
    low: Point,
    high: Point,
}

impl Ring {
    fn of(vertices: &[Vertex]) -> Ring {
        let n = vertices.len();
        let mut points = Vec::new();
        for (i, v) in vertices.iter().enumerate() {
            let next = vertices[(i + 1) % n].point();
            if v.sweep == Millideg::ZERO {
                points.push(v.point());
            } else {
                let chords = geom::grown::arc_points(v.point(), next, v.sweep);
                points.extend_from_slice(&chords[..chords.len() - 1]);
            }
        }
        let low = Point::new(
            points.iter().map(|p| p.x).min().expect("a loop"),
            points.iter().map(|p| p.y).min().expect("a loop"),
        );
        let high = Point::new(
            points.iter().map(|p| p.x).max().expect("a loop"),
            points.iter().map(|p| p.y).max().expect("a loop"),
        );
        Ring { points, low, high }
    }

    fn edges(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        let n = self.points.len();
        (0..n).map(move |i| (self.points[i], self.points[(i + 1) % n]))
    }

    fn overlaps(&self, other: &Ring) -> bool {
        self.low.x <= other.high.x
            && other.low.x <= self.high.x
            && self.low.y <= other.high.y
            && other.low.y <= self.high.y
    }

    /// The lowest point where the two rings cross or touch.
    fn meets(&self, other: &Ring) -> Option<Point> {
        if !self.overlaps(other) {
            return None;
        }
        let mut best: Option<Point> = None;
        for (a, b) in self.edges() {
            for (c, d) in other.edges() {
                if a.x.max(b.x) < c.x.min(d.x)
                    || c.x.max(d.x) < a.x.min(b.x)
                    || a.y.max(b.y) < c.y.min(d.y)
                    || c.y.max(d.y) < a.y.min(b.y)
                {
                    continue;
                }
                if let Some(p) = meet(a, b, c, d)
                    && best.is_none_or(|q| key(&p) < key(&q))
                {
                    best = Some(p);
                }
            }
        }
        best
    }

    /// Whether `other`, which neither crosses nor touches this ring, lies inside it.
    fn holds(&self, other: &Ring) -> bool {
        let inside_box = self.low.x <= other.low.x
            && self.low.y <= other.low.y
            && other.high.x <= self.high.x
            && other.high.y <= self.high.y;
        inside_box && self.contains(other.points[0])
    }

    /// Whether `p` is inside the ring, by the crossings of a ray towards +x.
    fn contains(&self, p: Point) -> bool {
        let mut inside = false;
        for (a, b) in self.edges() {
            if (a.y > p.y) != (b.y > p.y) {
                let t = (p.y.0 - a.y.0) as f64 / (b.y.0 - a.y.0) as f64;
                let x = a.x.0 as f64 + t * (b.x.0 - a.x.0) as f64;
                if (p.x.0 as f64) < x {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

fn cross(o: Point, a: Point, b: Point) -> i128 {
    let (ax, ay) = (i128::from(a.x.0 - o.x.0), i128::from(a.y.0 - o.y.0));
    let (bx, by) = (i128::from(b.x.0 - o.x.0), i128::from(b.y.0 - o.y.0));
    ax * by - ay * bx
}

/// Whether `p`, known to be on the line through `a` and `b`, lies within the segment.
fn within(a: Point, b: Point, p: Point) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}

/// Where segments `a`–`b` and `c`–`d` meet, if they do (touching counts).
fn meet(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let (d1, d2) = (cross(a, b, c), cross(a, b, d));
    let (d3, d4) = (cross(c, d, a), cross(c, d, b));
    if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0)) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn a_circle_of_two_half_arcs_has_the_area_of_a_circle() {
        let half = Millideg::from_deg(180);
        let circle = [Vertex::arc(p(-10, 0), half), Vertex::arc(p(10, 0), half)];
        let expected = std::f64::consts::PI * 100.0;
        assert!((area(&circle) - expected).abs() < 1e-9, "{}", area(&circle));
        assert!((area(&reversed(&circle)) + expected).abs() < 1e-9);
    }

    #[test]
    fn a_circle_broken_on_both_sides_has_two_gaps_between_its_halves() {
        let half = Millideg::from_deg(180);
        // Two half circles of radius 10 000, each 100 nm short of the other's ends.
        let segments = [
            Segment::arc(p(10_000, 50), p(-10_000, 50), half),
            Segment::arc(p(-10_000, -50), p(10_000, -50), half),
        ];
        let loops = build(&segments, Nm(1));
        assert!(loops.outline.is_none());
        assert_eq!(
            loops.problems,
            [
                Problem {
                    kind: ProblemKind::Gap,
                    at: p(-10_000, 0),
                    size: Some(Nm(100)),
                },
                Problem {
                    kind: ProblemKind::Gap,
                    at: p(10_000, 0),
                    size: Some(Nm(100)),
                },
            ]
        );
        assert_eq!(loops.strays.len(), 2);
        // Within 100 nm the ends meet, and the circle is the outline.
        let loops = build(&segments, Nm(100));
        assert!(loops.problems.is_empty());
        assert_eq!(loops.outline.map(|o| o.len()), Some(2));
    }

    #[test]
    fn three_ends_meeting_are_a_crossing_and_the_shape_stays_open() {
        // A triangle with a tail from one corner.
        let segments = [
            Segment::line(p(0, 0), p(100, 0)),
            Segment::line(p(100, 0), p(0, 100)),
            Segment::line(p(0, 100), p(0, 0)),
            Segment::line(p(0, 0), p(-50, -50)),
        ];
        let loops = build(&segments, Nm(0));
        assert!(loops.outline.is_none());
        assert!(loops.problems.contains(&Problem {
            kind: ProblemKind::Crossing,
            at: p(0, 0),
            size: None,
        }));
        assert_eq!(loops.strays.len(), 4);
    }

    #[test]
    fn a_square_traced_clockwise_turns_round_with_its_area() {
        let square: Vec<Vertex> = [p(0, 0), p(0, 10), p(10, 10), p(10, 0)]
            .into_iter()
            .map(Vertex::at)
            .collect();
        assert_eq!(area(&square), -100.0);
        assert_eq!(area(&reversed(&square)), 100.0);
    }
}
