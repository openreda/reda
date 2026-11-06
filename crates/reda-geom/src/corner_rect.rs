//! Rectangles whose corners are each square, rounded or chamfered: the copper outline of
//! every Pad shape. A circle is a square rounded by half its side, an oval a rectangle
//! rounded by half its shorter side.

use crate::{Millideg, Nm, Point, Vertex};

/// What is done to one corner of a [`CornerRect`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CornerCut {
    Square,
    /// Rounded by a quarter arc of this radius.
    Round(Nm),
    /// Cut off at 45°, this far along each side from the corner.
    Chamfer(Nm),
}

/// A `width` × `height` rectangle about `center` with its corners cut, in the order
/// top-left, top-right, bottom-left, bottom-right (+y is up). The cuts must fit: no two on
/// one side may together use more than that side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CornerRect {
    pub width: Nm,
    pub height: Nm,
    pub center: Point,
    pub corners: [CornerCut; 4],
}

/// The corner order of [`CornerRect::corners`]: top-left, top-right, bottom-left,
/// bottom-right, as the signs of their x and y.
const SIGNS: [(i64, i64); 4] = [(-1, 1), (1, 1), (-1, -1), (1, -1)];

/// A direction along an axis, as the signs of x and y.
type Dir = (i64, i64);

/// One corner of the outline: where it arrives, where it leaves, and the centre and radius
/// of the arc between them when the corner is rounded.
type CornerEnds = (Point, Point, Option<(Point, Nm)>);

/// Counter-clockwise from the bottom-right corner: the corner's index, the direction the
/// outline arrives along and the direction it leaves along.
const WALK: [(usize, Dir, Dir); 4] = [
    (3, (1, 0), (0, 1)),
    (1, (0, 1), (-1, 0)),
    (0, (-1, 0), (0, -1)),
    (2, (0, -1), (1, 0)),
];

/// One piece of the outline.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Piece {
    Line(Point, Point),
    /// A counter-clockwise quarter arc about `centre` from `from` to `to`.
    Arc {
        centre: Point,
        radius: Nm,
        from: Point,
        to: Point,
    },
}

impl CornerRect {
    /// A rectangle with every corner cut the same way.
    pub fn uniform(width: Nm, height: Nm, center: Point, cut: CornerCut) -> CornerRect {
        CornerRect {
            width,
            height,
            center,
            corners: [cut; 4],
        }
    }

    fn half(&self) -> (i64, i64) {
        (self.width.0 / 2, self.height.0 / 2)
    }

    /// The one radius every corner is rounded by (0 when all are square), or `None` when
    /// the corners differ or any is chamfered.
    pub fn uniform_radius(&self) -> Option<Nm> {
        let radius = |c: CornerCut| match c {
            CornerCut::Square => Some(Nm::ZERO),
            CornerCut::Round(r) => Some(r),
            CornerCut::Chamfer(_) => None,
        };
        let first = radius(self.corners[0])?;
        self.corners
            .iter()
            .all(|c| radius(*c) == Some(first))
            .then_some(first)
    }

    /// Whether it is a disc: a square rounded by half its side.
    pub fn is_circle(&self) -> bool {
        self.width == self.height && self.uniform_radius() == Some(Nm(self.width.0 / 2))
    }

    /// The same outline mirrored left-right about x = 0.
    pub fn mirrored(&self) -> CornerRect {
        let [tl, tr, bl, br] = self.corners;
        CornerRect {
            center: Point::new(-self.center.x, self.center.y),
            corners: [tr, tl, br, bl],
            ..*self
        }
    }

    /// The outline grown by `margin` on every side (shrunk when negative), or `None` when
    /// nothing is left. Rounded corners grow with it, square ones stay square and a
    /// chamfer moves out parallel to itself.
    pub fn grown(&self, margin: Nm) -> Option<CornerRect> {
        let (width, height) = (self.width + margin + margin, self.height + margin + margin);
        if width <= Nm::ZERO || height <= Nm::ZERO {
            return None;
        }
        let most = width.0.min(height.0) / 2;
        let chamfer_growth = (margin.0 as f64 * (2.0 - std::f64::consts::SQRT_2)).round() as i64;
        let corners = self.corners.map(|c| match c {
            CornerCut::Square => CornerCut::Square,
            CornerCut::Round(r) => CornerCut::Round(Nm((r.0 + margin.0).clamp(0, most))),
            CornerCut::Chamfer(c) => CornerCut::Chamfer(Nm((c.0 + chamfer_growth).clamp(0, most))),
        });
        Some(CornerRect {
            width,
            height,
            center: self.center,
            corners,
        })
    }

    /// The four corners of the uncut rectangle.
    pub fn box_corners(&self) -> [Point; 4] {
        let (hw, hh) = self.half();
        SIGNS.map(|(sx, sy)| Point::new(self.center.x + Nm(sx * hw), self.center.y + Nm(sy * hh)))
    }

    fn pieces(&self) -> Vec<Piece> {
        let (hw, hh) = self.half();
        let c = self.center;
        let at = |x: i64, y: i64| Point::new(c.x + Nm(x), c.y + Nm(y));
        // Each corner's arriving and leaving points, and the arc between them if any.
        let mut ends: Vec<CornerEnds> = Vec::with_capacity(4);
        for (i, (ix, iy), (ox, oy)) in WALK {
            let (sx, sy) = SIGNS[i];
            let (x, y) = (sx * hw, sy * hh);
            match self.corners[i] {
                CornerCut::Square | CornerCut::Round(Nm(0)) | CornerCut::Chamfer(Nm(0)) => {
                    ends.push((at(x, y), at(x, y), None));
                }
                CornerCut::Round(Nm(r)) => {
                    let from = at(x - ix * r, y - iy * r);
                    let to = at(x + ox * r, y + oy * r);
                    let centre = at(x - ix * r + ox * r, y - iy * r + oy * r);
                    ends.push((from, to, Some((centre, Nm(r)))));
                }
                CornerCut::Chamfer(Nm(d)) => {
                    ends.push((at(x - ix * d, y - iy * d), at(x + ox * d, y + oy * d), None));
                }
            }
        }
        let mut out = Vec::with_capacity(8);
        for k in 0..4 {
            let (from, to, arc) = ends[k];
            match arc {
                Some((centre, radius)) => out.push(Piece::Arc {
                    centre,
                    radius,
                    from,
                    to,
                }),
                None if from != to => out.push(Piece::Line(from, to)),
                None => {}
            }
            let next = ends[(k + 1) % 4].0;
            if to != next {
                out.push(Piece::Line(to, next));
            }
        }
        out
    }

    /// The centre and radius of each rounded corner, counter-clockwise from the
    /// bottom-right. The outline is the polygon through the [`contour`](Self::contour)'s
    /// vertices joined by straight edges, together with these discs.
    pub fn round_corners(&self) -> Vec<(Point, Nm)> {
        self.pieces()
            .into_iter()
            .filter_map(|piece| match piece {
                Piece::Arc { centre, radius, .. } => Some((centre, radius)),
                Piece::Line(..) => None,
            })
            .collect()
    }

    /// The outline as a closed contour, counter-clockwise.
    pub fn contour(&self) -> Vec<Vertex> {
        self.pieces()
            .into_iter()
            .map(|piece| match piece {
                Piece::Line(from, _) => Vertex::at(from),
                Piece::Arc { from, .. } => Vertex::arc(from, Millideg::from_deg(90)),
            })
            .collect()
    }

    /// The outline as a convex polygon, counter-clockwise, its arcs replaced by chords
    /// that stay within `tolerance` of them. The corners of the chords lie on the arcs.
    pub fn polygon(&self, tolerance: Nm) -> Vec<Point> {
        let mut out = Vec::new();
        for piece in self.pieces() {
            match piece {
                Piece::Line(from, _) => out.push(from),
                Piece::Arc {
                    centre,
                    radius,
                    from,
                    ..
                } => {
                    let r = radius.0 as f64;
                    let step = if r > tolerance.0 as f64 {
                        2.0 * (1.0 - tolerance.0.max(1) as f64 / r).acos()
                    } else {
                        std::f64::consts::FRAC_PI_2
                    };
                    let n = ((std::f64::consts::FRAC_PI_2 / step).ceil() as usize).clamp(1, 256);
                    let start = ((from.y - centre.y).0 as f64).atan2((from.x - centre.x).0 as f64);
                    out.push(from);
                    for i in 1..n {
                        let a = start + std::f64::consts::FRAC_PI_2 * i as f64 / n as f64;
                        out.push(Point::new(
                            Nm(centre.x.0 + (r * a.cos()).round() as i64),
                            Nm(centre.y.0 + (r * a.sin()).round() as i64),
                        ));
                    }
                }
            }
        }
        out.dedup();
        while out.len() > 1 && out.first() == out.last() {
            out.pop();
        }
        out
    }

    /// Whether `p` is on the outline or inside it.
    pub fn contains(&self, p: Point) -> bool {
        let (hw, hh) = self.half();
        let (qx, qy) = ((p.x - self.center.x).0, (p.y - self.center.y).0);
        if qx.abs() > hw || qy.abs() > hh {
            return false;
        }
        SIGNS.iter().zip(self.corners).all(|(&(sx, sy), cut)| {
            let (u, v) = (sx * qx, sy * qy);
            match cut {
                CornerCut::Square => true,
                CornerCut::Round(Nm(r)) => {
                    let (du, dv) = (u - (hw - r), v - (hh - r));
                    du <= 0
                        || dv <= 0
                        || (du as i128).pow(2) + (dv as i128).pow(2) <= (r as i128).pow(2)
                }
                CornerCut::Chamfer(Nm(c)) => u + v <= hw + hh - c,
            }
        })
    }

    /// The distance from `p` to the outline: positive inside, negative outside.
    pub fn edge_distance(&self, p: Point) -> f64 {
        let d = self
            .pieces()
            .iter()
            .map(|piece| piece_distance(*piece, p))
            .fold(f64::INFINITY, f64::min);
        if self.contains(p) { d } else { -d }
    }
}

fn point_segment(p: Point, a: Point, b: Point) -> f64 {
    let (px, py) = (p.x.0 as f64, p.y.0 as f64);
    let (ax, ay) = (a.x.0 as f64, a.y.0 as f64);
    let (dx, dy) = (b.x.0 as f64 - ax, b.y.0 as f64 - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    };
    (px - ax - t * dx).hypot(py - ay - t * dy)
}

fn piece_distance(piece: Piece, p: Point) -> f64 {
    match piece {
        Piece::Line(a, b) => point_segment(p, a, b),
        Piece::Arc {
            centre,
            radius,
            from,
            to,
        } => {
            let (dx, dy) = ((p.x - centre.x).0 as f64, (p.y - centre.y).0 as f64);
            let (fx, fy) = ((from.x - centre.x).0 as f64, (from.y - centre.y).0 as f64);
            let (tx, ty) = ((to.x - centre.x).0 as f64, (to.y - centre.y).0 as f64);
            // Within the quarter's wedge: counter-clockwise of `from` and clockwise of `to`.
            let within = fx * dy - fy * dx >= 0.0 && dx * ty - dy * tx >= 0.0;
            if within {
                (dx.hypot(dy) - radius.0 as f64).abs()
            } else {
                let to_end = |e: Point| ((p.x - e.x).0 as f64).hypot((p.y - e.y).0 as f64);
                to_end(from).min(to_end(to))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn a_square_rectangle_is_four_corners_counter_clockwise() {
        let r = CornerRect::uniform(Nm(4), Nm(2), p(10, 0), CornerCut::Square);
        let points: Vec<Point> = r.contour().iter().map(|v| v.point()).collect();
        assert_eq!(points, vec![p(12, -1), p(12, 1), p(8, 1), p(8, -1)]);
        assert_eq!(r.uniform_radius(), Some(Nm::ZERO));
    }

    #[test]
    fn one_chamfered_corner_cuts_only_that_corner() {
        let mut r = CornerRect::uniform(Nm(10), Nm(10), p(0, 0), CornerCut::Square);
        r.corners[1] = CornerCut::Chamfer(Nm(2));
        let points: Vec<Point> = r.contour().iter().map(|v| v.point()).collect();
        assert_eq!(
            points,
            vec![p(5, -5), p(5, 3), p(3, 5), p(-5, 5), p(-5, -5)]
        );
        assert_eq!(r.uniform_radius(), None);
        assert!(!r.contains(p(5, 5)));
        assert!(r.contains(p(4, 4)));
        assert!((r.edge_distance(p(0, 0)) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_fully_rounded_square_is_a_circle_of_four_arcs() {
        let r = CornerRect::uniform(Nm(10), Nm(10), p(0, 0), CornerCut::Round(Nm(5)));
        assert!(r.is_circle());
        let contour = r.contour();
        assert_eq!(contour.len(), 4);
        assert!(contour.iter().all(|v| v.sweep == Millideg::from_deg(90)));
        assert!((r.edge_distance(p(1, 0)) - 4.0).abs() < 1e-9);
        assert!((r.edge_distance(p(8, 0)) + 3.0).abs() < 1e-9);
    }

    #[test]
    fn mirroring_swaps_left_and_right_corners() {
        let mut r = CornerRect::uniform(Nm(10), Nm(10), p(3, 1), CornerCut::Square);
        r.corners[0] = CornerCut::Round(Nm(2));
        let m = r.mirrored();
        assert_eq!(m.center, p(-3, 1));
        assert_eq!(m.corners[1], CornerCut::Round(Nm(2)));
        assert_eq!(m.corners[0], CornerCut::Square);
    }

    #[test]
    fn growing_keeps_square_corners_and_moves_chamfers_out() {
        let mut r = CornerRect::uniform(Nm(1000), Nm(1000), p(0, 0), CornerCut::Square);
        r.corners[3] = CornerCut::Chamfer(Nm(100));
        r.corners[0] = CornerCut::Round(Nm(100));
        let g = r.grown(Nm(100)).unwrap();
        assert_eq!(g.width, Nm(1200));
        assert_eq!(g.corners[1], CornerCut::Square);
        assert_eq!(g.corners[0], CornerCut::Round(Nm(200)));
        assert_eq!(g.corners[3], CornerCut::Chamfer(Nm(159)));
        assert_eq!(r.grown(Nm(-500)), None);
    }

    #[test]
    fn a_polygon_follows_its_arcs_within_the_tolerance() {
        let r = CornerRect::uniform(
            Nm(2_000_000),
            Nm(1_000_000),
            p(0, 0),
            CornerCut::Round(Nm(250_000)),
        );
        let poly = r.polygon(Nm(100));
        assert!(poly.len() > 8);
        for q in &poly {
            assert!(r.edge_distance(*q).abs() < 2.0, "{q:?} is off the outline");
        }
    }
}
