//! Vector outlines of Scene primitives, shared by the SVG and PDF writers.
//!
//! Every point is rounded to whole nm, so both writers print exact decimals and their
//! output is the same bytes on every platform.

use std::collections::BTreeMap;

use reda_model::geom::{Millideg, Nm, Point};
use reda_text::Mesh;

use crate::transform::rotate;

/// One step of a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Seg {
    Move(Point),
    Line(Point),
    /// A cubic Bézier through two control points to the end point.
    Cubic(Point, Point, Point),
    Close,
}

/// How far a quarter circle's Bézier control points sit along the tangents, as a
/// fraction of the radius.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// The closed outlines of the area a triangle list covers.
///
/// Triangles are turned counter-clockwise and every edge two triangles share is dropped,
/// so a glyph comes out as its outer contour and holes rather than as triangles: no
/// hairline seams between triangles in a viewer, and much smaller files. Filled with the
/// non-zero rule, the contours cover exactly what the triangles cover.
pub(crate) fn mesh_contours(mesh: &Mesh) -> Vec<Vec<Point>> {
    let mut edges: BTreeMap<(Point, Point), u32> = BTreeMap::new();
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| mesh.points[t[i] as usize]);
        let turn = cross(a, b, c);
        if turn == 0 {
            continue;
        }
        let tri = if turn > 0 { [a, b, c] } else { [a, c, b] };
        for i in 0..3 {
            let (p, q) = (tri[i], tri[(i + 1) % 3]);
            match edges.get_mut(&(q, p)) {
                Some(n) => {
                    *n -= 1;
                    if *n == 0 {
                        edges.remove(&(q, p));
                    }
                }
                None => *edges.entry((p, q)).or_default() += 1,
            }
        }
    }
    let mut next: BTreeMap<Point, Vec<Point>> = BTreeMap::new();
    for ((p, q), n) in edges {
        for _ in 0..n {
            next.entry(p).or_default().push(q);
        }
    }
    // Every triangle is a cycle and cancelling opposite edges keeps each point's in and
    // out counts equal, so each walk comes back to where it started.
    let mut contours = Vec::new();
    while let Some((&start, _)) = next.iter().next() {
        let mut contour = vec![start];
        let mut at = start;
        while let Some(outs) = next.get_mut(&at) {
            let to = outs.pop().expect("points with no edges left are removed");
            if outs.is_empty() {
                next.remove(&at);
            }
            if to == start {
                break;
            }
            contour.push(to);
            at = to;
        }
        contours.push(contour);
    }
    contours
}

fn cross(a: Point, b: Point, c: Point) -> i128 {
    let (abx, aby) = (i128::from((b.x - a.x).0), i128::from((b.y - a.y).0));
    let (acx, acy) = (i128::from((c.x - a.x).0), i128::from((c.y - a.y).0));
    abx * acy - aby * acx
}

/// The outline of a rectangle `size` about `center`, turned `rotation`, with corners
/// rounded to `radius` (clamped to half the shorter side).
pub(crate) fn rect(center: Point, size: (Nm, Nm), rotation: Millideg, radius: Nm) -> Vec<Seg> {
    let (hw, hh) = (size.0.0 as f64 / 2.0, size.1.0 as f64 / 2.0);
    let r = (radius.0 as f64).clamp(0.0, hw.min(hh));
    let place = |x: f64, y: f64| {
        let (x, y) = rotate(x.round() as i64, y.round() as i64, rotation);
        Point::new(center.x + Nm(x), center.y + Nm(y))
    };
    if r == 0.0 {
        return vec![
            Seg::Move(place(-hw, -hh)),
            Seg::Line(place(hw, -hh)),
            Seg::Line(place(hw, hh)),
            Seg::Line(place(-hw, hh)),
            Seg::Close,
        ];
    }
    let k = r * (1.0 - KAPPA);
    vec![
        Seg::Move(place(-hw + r, -hh)),
        Seg::Line(place(hw - r, -hh)),
        Seg::Cubic(place(hw - k, -hh), place(hw, -hh + k), place(hw, -hh + r)),
        Seg::Line(place(hw, hh - r)),
        Seg::Cubic(place(hw, hh - k), place(hw - k, hh), place(hw - r, hh)),
        Seg::Line(place(-hw + r, hh)),
        Seg::Cubic(place(-hw + k, hh), place(-hw, hh - k), place(-hw, hh - r)),
        Seg::Line(place(-hw, -hh + r)),
        Seg::Cubic(
            place(-hw, -hh + k),
            place(-hw + k, -hh),
            place(-hw + r, -hh),
        ),
        Seg::Close,
    ]
}

/// A circle of radius `r` about `center`, as four Bézier quarters, counter-clockwise.
pub(crate) fn circle(center: Point, r: Nm) -> Vec<Seg> {
    let k = (r.0 as f64 * KAPPA).round() as i64;
    let r = r.0;
    let p = |x: i64, y: i64| Point::new(center.x + Nm(x), center.y + Nm(y));
    vec![
        Seg::Move(p(r, 0)),
        Seg::Cubic(p(r, k), p(k, r), p(0, r)),
        Seg::Cubic(p(-k, r), p(-r, k), p(-r, 0)),
        Seg::Cubic(p(-r, -k), p(-k, -r), p(0, -r)),
        Seg::Cubic(p(k, -r), p(r, -k), p(r, 0)),
        Seg::Close,
    ]
}

/// The contours as one path.
pub(crate) fn polygons(contours: &[Vec<Point>]) -> Vec<Seg> {
    let mut path = Vec::new();
    for c in contours {
        let Some((first, rest)) = c.split_first() else {
            continue;
        };
        path.push(Seg::Move(*first));
        path.extend(rest.iter().map(|p| Seg::Line(*p)));
        path.push(Seg::Close);
    }
    path
}

/// `nm` as exact millimetres, without trailing zeros.
pub(crate) fn mm(nm: i64) -> String {
    Nm(nm).mm_exact()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn a_square_of_two_triangles_is_one_contour() {
        let mesh = Mesh {
            points: vec![p(0, 0), p(10, 0), p(10, 10), p(0, 10)],
            // The second triangle is clockwise: it is turned before its edges count.
            indices: vec![0, 1, 2, 0, 3, 2],
        };
        assert_eq!(
            mesh_contours(&mesh),
            vec![vec![p(0, 0), p(10, 0), p(10, 10), p(0, 10)]]
        );
    }

    #[test]
    fn a_ring_keeps_its_hole() {
        // A 3 x 3 grid of unit squares without the middle one.
        let mut points = Vec::new();
        let mut indices = Vec::new();
        for (x, y) in [
            (0, 0),
            (1, 0),
            (2, 0),
            (0, 1),
            (2, 1),
            (0, 2),
            (1, 2),
            (2, 2),
        ] {
            let base = points.len() as u32;
            points.extend([p(x, y), p(x + 1, y), p(x + 1, y + 1), p(x, y + 1)]);
            indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        }
        let contours = mesh_contours(&Mesh { points, indices });
        assert_eq!(contours.len(), 2);
        let mut lengths: Vec<usize> = contours.iter().map(Vec::len).collect();
        lengths.sort();
        assert_eq!(
            lengths,
            vec![4, 12],
            "the hole, and the outer edge through every grid point"
        );
    }

    #[test]
    fn millimetres_are_exact() {
        assert_eq!(mm(152_400), "0.1524");
        assert_eq!(mm(-25_400_000), "-25.4");
        assert_eq!(mm(3_000_000), "3");
        assert_eq!(mm(1), "0.000001");
    }
}
