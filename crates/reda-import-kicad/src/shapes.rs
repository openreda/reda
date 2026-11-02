//! Shapes KiCad describes differently from REDA: arcs through three points, and polylines.

use reda_model::Shape;
use reda_model::geom::{Millideg, Nm, Point};

/// The arc from `start` through `mid` to `end`, or a line when the three are collinear.
pub fn arc_through(start: Point, mid: Point, end: Point) -> Shape {
    let f = |p: Point| (p.x.0 as f64, p.y.0 as f64);
    let ((ax, ay), (bx, by), (cx, cy)) = (f(start), f(mid), f(end));
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1.0 {
        return Shape::Line { start, end };
    }
    let (a2, b2, c2) = (ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy);
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    if !ux.is_finite() || !uy.is_finite() || ux.abs() > 1e15 || uy.abs() > 1e15 {
        return Shape::Line { start, end };
    }
    let angle = |x: f64, y: f64| (y - uy).atan2(x - ux).to_degrees();
    let turn = |a: f64| a.rem_euclid(360.0);
    let (a0, am, a1) = (angle(ax, ay), angle(bx, by), angle(cx, cy));
    let ccw = turn(a1 - a0);
    let sweep = if turn(am - a0) <= ccw {
        ccw
    } else {
        ccw - 360.0
    };
    let sweep = if start == end { 360.0 } else { sweep };
    Shape::Arc {
        center: Point::new(Nm(ux.round() as i64), Nm(uy.round() as i64)),
        start,
        sweep: Millideg((sweep * 1000.0).round() as i64),
    }
}

/// A polyline: one line for two points, a polygon when it closes on itself or is filled,
/// otherwise a line per segment. Fewer than two points draw nothing.
pub fn polyline(mut points: Vec<Point>, filled: bool) -> Vec<Shape> {
    points.dedup();
    if points.len() < 2 {
        return vec![];
    }
    let closed = points.len() > 2 && points.first() == points.last();
    if closed || (filled && points.len() > 2) {
        if closed {
            points.pop();
        }
        return vec![Shape::Polygon { points }];
    }
    points
        .windows(2)
        .map(|w| Shape::Line {
            start: w[0],
            end: w[1],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn three_points_give_centre_and_signed_sweep() {
        let ccw = arc_through(p(1000, 0), p(0, 1000), p(-1000, 0));
        assert_eq!(
            ccw,
            Shape::Arc {
                center: p(0, 0),
                start: p(1000, 0),
                sweep: Millideg(180_000)
            }
        );
        let cw = arc_through(p(1000, 0), p(0, -1000), p(-1000, 0));
        assert!(matches!(
            cw,
            Shape::Arc {
                sweep: Millideg(-180_000),
                ..
            }
        ));
        let quarter = arc_through(p(1000, 0), p(600, 800), p(0, 1000));
        assert!(matches!(
            quarter,
            Shape::Arc {
                sweep: Millideg(90_000),
                ..
            }
        ));
        assert!(matches!(
            arc_through(p(0, 0), p(1, 1), p(2, 2)),
            Shape::Line { .. }
        ));
    }
}
