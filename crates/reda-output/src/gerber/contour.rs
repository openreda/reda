//! Contours the Gerber format can carry: points under 1 µm apart merged, and a shape with
//! holes joined into one contour by cut-ins.
//!
//! A cut-in runs from a hole's rightmost point straight to the right, to the nearest edge
//! of what is already joined, and comes back along the same line, so the contour stays
//! one closed path whose only overlaps are the fully coincident cut-in pairs the Gerber
//! specification allows. Holes are joined rightmost first, so the line never crosses a
//! hole that is still to be joined.

use reda_model::geom::{Nm, Point};

/// Contour points closer than this are merged (Gerber specification §4.14.2).
pub(crate) const MIN_SEPARATION: Nm = Nm(1_000);

pub(crate) fn close_points(a: Point, b: Point) -> bool {
    let (dx, dy) = (i128::from((a.x - b.x).0), i128::from((a.y - b.y).0));
    let d = i128::from(MIN_SEPARATION.0);
    dx * dx + dy * dy < d * d
}

/// `ring` (closed back to its first point implicitly) with every point under 1 µm from
/// the last kept one dropped, the closing point included. Empty when fewer than three
/// points are left, which bound no area.
pub(crate) fn merged(ring: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(ring.len());
    for &p in ring {
        if out.last().is_none_or(|&last| !close_points(last, p)) {
            out.push(p);
        }
    }
    while out.len() > 1 && close_points(out[out.len() - 1], out[0]) {
        out.pop();
    }
    if out.len() < 3 { Vec::new() } else { out }
}

/// Twice the signed area of `ring`: positive when counter-clockwise.
pub(crate) fn twice_area(ring: &[Point]) -> i128 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            i128::from(a.x.0) * i128::from(b.y.0) - i128::from(b.x.0) * i128::from(a.y.0)
        })
        .sum()
}

fn oriented(mut ring: Vec<Point>, ccw: bool) -> Vec<Point> {
    if (twice_area(&ring) > 0) != ccw {
        ring.reverse();
    }
    ring
}

/// The shape bounded by `outer` less `holes` as one contour: the outer ring
/// counter-clockwise, each hole clockwise, joined by cut-ins. Rings are merged first;
/// a hole merged away is left out. Empty when the outer ring is.
pub(crate) fn joined(outer: &[Point], holes: &[Vec<Point>]) -> Vec<Point> {
    let mut contour = oriented(merged(outer), true);
    if contour.is_empty() {
        return contour;
    }
    let mut holes: Vec<Vec<Point>> = holes
        .iter()
        .map(|h| merged(h))
        .filter(|h| !h.is_empty())
        .map(|h| oriented(h, false))
        .collect();
    let rightmost = |h: &[Point]| {
        (0..h.len())
            .max_by_key(|&i| (h[i].x, std::cmp::Reverse(h[i].y)))
            .expect("a merged ring has points")
    };
    holes.sort_by_key(|h| std::cmp::Reverse(h[rightmost(h)].x));
    for hole in holes {
        let start = rightmost(&hole);
        let p = hole[start];
        let Some((edge, hit)) = nearest_crossing(&contour, p) else {
            continue;
        };
        // The hole, from its rightmost point round to it again.
        let mut loop_: Vec<Point> = (0..=hole.len())
            .map(|k| hole[(start + k) % hole.len()])
            .collect();
        let (a, b) = (contour[edge], contour[(edge + 1) % contour.len()]);
        // A crossing within 1 µm of an end of the edge uses that end, so no new point
        // lands closer than the format allows.
        let joint = if close_points(hit, a) {
            a
        } else if close_points(hit, b) {
            b
        } else {
            hit
        };
        let mut insert = Vec::with_capacity(loop_.len() + 2);
        if joint != a {
            insert.push(joint);
        }
        insert.append(&mut loop_);
        // On `b`, the point that follows closes the cut-in.
        if joint != b {
            insert.push(joint);
        }
        let at = edge + 1;
        contour.splice(at..at, insert);
    }
    contour
}

/// The edge of `contour` that a ray from `p` to the right reaches first, leaving the
/// filled side (an upward edge, with the fill on its left), and the point it meets.
fn nearest_crossing(contour: &[Point], p: Point) -> Option<(usize, Point)> {
    let n = contour.len();
    let mut best: Option<(i128, usize, Point)> = None;
    for i in 0..n {
        let (a, b) = (contour[i], contour[(i + 1) % n]);
        if a.y >= b.y || p.y < a.y || p.y > b.y {
            continue;
        }
        // x where the edge meets y = p.y, rounded to the nanometre.
        let (dy, t) = (i128::from((b.y - a.y).0), i128::from((p.y - a.y).0));
        let num = i128::from((b.x - a.x).0) * t;
        let x = i128::from(a.x.0) + div_round(num, dy);
        if x < i128::from(p.x.0) {
            continue;
        }
        if best.is_none_or(|(bx, _, _)| x < bx) {
            best = Some((x, i, Point::new(Nm(x as i64), p.y)));
        }
    }
    best.map(|(_, i, hit)| (i, hit))
}

fn div_round(num: i128, den: i128) -> i128 {
    let q = num.div_euclid(den);
    if 2 * num.rem_euclid(den) >= den {
        q + 1
    } else {
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    fn square(x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<Point> {
        vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]
    }

    /// Whether `q` is filled under the non-zero rule.
    fn winding(ring: &[Point], q: (f64, f64)) -> i32 {
        let mut w = 0;
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let (ax, ay, bx, by) = (a.x.0 as f64, a.y.0 as f64, b.x.0 as f64, b.y.0 as f64);
            let side = (bx - ax) * (q.1 - ay) - (by - ay) * (q.0 - ax);
            if ay <= q.1 && by > q.1 && side > 0.0 {
                w += 1;
            } else if ay > q.1 && by <= q.1 && side < 0.0 {
                w -= 1;
            }
        }
        w
    }

    #[test]
    fn points_under_a_micrometre_apart_merge_and_the_rest_stay() {
        let ring = vec![
            p(0, 0),
            p(999, 0),
            p(10_000, 0),
            p(10_000, 10_000),
            p(0, 10_000),
            p(0, 500),
        ];
        assert_eq!(merged(&ring), square(0, 0, 10_000, 10_000));
        assert!(merged(&[p(0, 0), p(10_000, 0), p(10_000, 900)]).is_empty());
    }

    #[test]
    fn holes_join_the_outline_by_coincident_cut_ins() {
        // A 10 × 10 µm square with two square holes side by side.
        let outer = square(0, 0, 100_000, 100_000);
        let holes = vec![
            square(10_000, 10_000, 30_000, 30_000),
            // Given clockwise and counter-clockwise: the joiner orients them itself.
            square(50_000, 40_000, 70_000, 60_000)
                .into_iter()
                .rev()
                .collect(),
        ];
        let c = joined(&outer, &holes);
        // Per hole: its ring closed on its first point, and the joint on the outline twice.
        assert_eq!(c.len(), 4 + 2 * (4 + 1 + 2));
        assert_eq!(
            twice_area(&c),
            2 * (100_000i128 * 100_000 - 2 * 20_000 * 20_000)
        );
        for (q, filled) in [
            ((5_000.0, 5_000.0), true),
            ((20_000.0, 20_000.0), false),
            ((60_000.0, 50_000.0), false),
            ((40_000.0, 20_000.0), true),
            ((90_000.0, 50_000.0), true),
            ((150_000.0, 50_000.0), false),
        ] {
            assert_eq!(winding(&c, q) != 0, filled, "{q:?} in {c:?}");
        }
        // Every segment not on a ring's own edge is a cut-in, run both ways.
        let segments: Vec<(Point, Point)> =
            (0..c.len()).map(|i| (c[i], c[(i + 1) % c.len()])).collect();
        let on_ring = |q: Point| {
            [&outer, &holes[0], &holes[1]].iter().any(|ring| {
                (0..ring.len()).any(|i| q.on_segment(ring[i], ring[(i + 1) % ring.len()]))
            })
        };
        let cut_ins: Vec<_> = segments
            .iter()
            .filter(|(a, b)| {
                let mid = Point::new(Nm((a.x.0 + b.x.0) / 2), Nm((a.y.0 + b.y.0) / 2));
                !on_ring(mid)
            })
            .collect();
        assert_eq!(cut_ins.len(), 4, "{c:?}");
        assert!(cut_ins.iter().all(|(a, b)| segments.contains(&(*b, *a))));
    }

    #[test]
    fn a_cut_in_meeting_a_corner_uses_the_corner() {
        // The hole's rightmost point is level with a vertex of the outline.
        let outer = vec![
            p(0, 0),
            p(100_000, 0),
            p(100_000, 20_000),
            p(100_000, 100_000),
            p(0, 100_000),
        ];
        let hole = vec![p(40_000, 20_000), p(60_000, 20_000), p(50_000, 30_000)];
        let c = joined(&outer, &[hole]);
        assert_eq!(twice_area(&c), 2 * 100_000i128 * 100_000 - 20_000 * 10_000);
        assert!(c.windows(2).all(|w| w[0] != w[1]), "{c:?}");
        assert_eq!(
            c.iter().filter(|q| **q == p(100_000, 20_000)).count(),
            2,
            "{c:?}"
        );
    }
}
