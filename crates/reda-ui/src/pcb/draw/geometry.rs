//! Geometry the drawing Tools need: rectangle corners and arcs through three points.

use std::f64::consts::TAU;

use reda_engine::model::geom::{Millideg, Nm, Point};

/// The corners of the axis-aligned rectangle with opposite corners `a` and `b`, in order
/// from `a`.
pub(super) fn rect_corners(a: Point, b: Point) -> [Point; 4] {
    [a, Point::new(b.x, a.y), b, Point::new(a.x, b.y)]
}

/// The circle through `a`, `b` and `c`: centre and the three points' angles (radians),
/// or `None` when they lie on one line.
fn circle(a: Point, b: Point, c: Point) -> Option<((f64, f64), [f64; 3])> {
    let f = |p: Point| (p.x.0 as f64, p.y.0 as f64);
    let ((ax, ay), (bx, by), (cx, cy)) = (f(a), f(b), f(c));
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-6 * (ax.abs() + bx.abs() + cx.abs() + ay.abs() + by.abs() + cy.abs() + 1.0) {
        return None;
    }
    let (a2, b2, c2) = (ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy);
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    let angle = |(x, y): (f64, f64)| (y - uy).atan2(x - ux);
    Some((
        (ux, uy),
        [angle((ax, ay)), angle((bx, by)), angle((cx, cy))],
    ))
}

/// The counter-clockwise turn from angle `from` to `to`, in [0, 2π).
fn ccw(from: f64, to: f64) -> f64 {
    (to - from).rem_euclid(TAU)
}

/// The sweep (counter-clockwise when positive) of the arc from `from` to `to` that
/// passes through `through`, or `None` when the three lie on one line.
pub fn arc_through(from: Point, to: Point, through: Point) -> Option<Millideg> {
    let (_, [a, b, c]) = circle(from, to, through)?;
    let sweep = ccw(a, b);
    let sweep = if ccw(a, c) < sweep {
        sweep
    } else {
        sweep - TAU
    };
    Some(Millideg((sweep.to_degrees() * 1000.0).round() as i64))
}

/// Points along the arc from `from` to `to` through `through`, at most 5° apart.
pub(super) fn arc_points(from: Point, to: Point, through: Point) -> Option<Vec<Point>> {
    let ((ux, uy), [a, _, _]) = circle(from, to, through)?;
    let sweep = (arc_through(from, to, through)?.0 as f64 / 1000.0).to_radians();
    let r = ((from.x.0 as f64 - ux).powi(2) + (from.y.0 as f64 - uy).powi(2)).sqrt();
    let steps = ((sweep.abs().to_degrees() / 5.0).ceil() as usize).max(1);
    let mut points: Vec<Point> = (0..steps)
        .map(|i| {
            let t = a + sweep * i as f64 / steps as f64;
            Point::new(
                Nm((ux + r * t.cos()).round() as i64),
                Nm((uy + r * t.sin()).round() as i64),
            )
        })
        .collect();
    points.push(to);
    Some(points)
}

/// Points around the circle about `centre` through `edge`, 5° apart.
pub(super) fn circle_points(centre: Point, edge: Point) -> Vec<Point> {
    let (dx, dy) = (
        (edge.x.0 - centre.x.0) as f64,
        (edge.y.0 - centre.y.0) as f64,
    );
    let (r, start) = (dx.hypot(dy), dy.atan2(dx));
    (0..72)
        .map(|i| {
            let t = start + TAU * i as f64 / 72.0;
            Point::new(
                Nm((centre.x.0 as f64 + r * t.cos()).round() as i64),
                Nm((centre.y.0 as f64 + r * t.sin()).round() as i64),
            )
        })
        .collect()
}

/// The arc about `centre` from `start` counter-clockwise to the ray towards `towards`:
/// its end and sweep, or `None` when a point sits on the centre.
pub(super) fn arc_about(centre: Point, start: Point, towards: Point) -> Option<(Point, Millideg)> {
    let rel = |p: Point| ((p.x.0 - centre.x.0) as f64, (p.y.0 - centre.y.0) as f64);
    let ((sx, sy), (tx, ty)) = (rel(start), rel(towards));
    if (sx == 0.0 && sy == 0.0) || (tx == 0.0 && ty == 0.0) {
        return None;
    }
    let (r, a, b) = (sx.hypot(sy), sy.atan2(sx), ty.atan2(tx));
    let sweep = Millideg((ccw(a, b).to_degrees() * 1000.0).round() as i64);
    if sweep.0 == 0 {
        return None;
    }
    let end = Point::new(
        Nm((centre.x.0 as f64 + r * b.cos()).round() as i64),
        Nm((centre.y.0 as f64 + r * b.sin()).round() as i64),
    );
    Some((end, sweep))
}

/// Points along the arc about `centre` from `start`, sweeping `sweep`, at most 5° apart.
pub(super) fn arc_points_about(centre: Point, start: Point, sweep: Millideg) -> Vec<Point> {
    let (dx, dy) = (
        (start.x.0 - centre.x.0) as f64,
        (start.y.0 - centre.y.0) as f64,
    );
    let (r, a) = (dx.hypot(dy), dy.atan2(dx));
    let sweep = (sweep.0 as f64 / 1000.0).to_radians();
    let steps = ((sweep.abs().to_degrees() / 5.0).ceil() as usize).max(1);
    (0..=steps)
        .map(|i| {
            let t = a + sweep * i as f64 / steps as f64;
            Point::new(
                Nm((centre.x.0 as f64 + r * t.cos()).round() as i64),
                Nm((centre.y.0 as f64 + r * t.sin()).round() as i64),
            )
        })
        .collect()
}
