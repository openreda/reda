//! Flattening arcs and arc-edged contours into points.

use reda_model::geom::{Millideg, Nm, Point, Vertex};

/// Arcs are drawn as chords at most this many millidegrees long.
pub(crate) const ARC_STEP: i64 = 5_000;

/// The points of the arc about `center` from `start`, sweeping `sweep` (counter-clockwise
/// when positive), in chords of at most [`ARC_STEP`]. Starts at `start`.
pub(crate) fn about(center: Point, start: Point, sweep: Millideg) -> Vec<Point> {
    let steps = ((sweep.0.abs() + ARC_STEP - 1) / ARC_STEP).max(1);
    let (dx, dy) = ((start.x - center.x).0 as f64, (start.y - center.y).0 as f64);
    let r = dx.hypot(dy);
    let a0 = dy.atan2(dx);
    let total = (sweep.0 as f64 / 1000.0).to_radians();
    (0..=steps)
        .map(|i| {
            if i == 0 {
                return start;
            }
            let a = a0 + total * i as f64 / steps as f64;
            Point::new(
                center.x + Nm((r * a.cos()).round() as i64),
                center.y + Nm((r * a.sin()).round() as i64),
            )
        })
        .collect()
}

/// The points of the arc from `from` to `to` sweeping `sweep`, ending exactly on `to`.
pub(crate) fn through(from: Point, to: Point, sweep: Millideg) -> Vec<Point> {
    if sweep.0 == 0 || from == to {
        return vec![from, to];
    }
    let (dx, dy) = ((to.x - from.x).0 as f64, (to.y - from.y).0 as f64);
    let chord = dx.hypot(dy);
    let half = (sweep.0 as f64 / 1000.0).to_radians() / 2.0;
    // The centre is off the chord's midpoint along its left normal (right for a
    // clockwise sweep).
    let k = (chord / 2.0) / half.tan() / chord;
    let center = Point::new(
        Nm(((from.x.0 + to.x.0) as f64 / 2.0 - dy * k).round() as i64),
        Nm(((from.y.0 + to.y.0) as f64 / 2.0 + dx * k).round() as i64),
    );
    let mut points = about(center, from, sweep);
    if let Some(last) = points.last_mut() {
        *last = to;
    }
    points
}

/// The corners of a closed contour with arc edges, the arcs flattened. The last point does
/// not repeat the first.
pub(crate) fn contour(vertices: &[Vertex]) -> Vec<Point> {
    let n = vertices.len();
    let mut out = Vec::new();
    for (i, v) in vertices.iter().enumerate() {
        let next = &vertices[(i + 1) % n];
        let (a, b) = (Point::new(v.x, v.y), Point::new(next.x, next.y));
        let mut points = through(a, b, v.sweep);
        points.pop();
        out.extend(points);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn a_half_turn_counter_clockwise_bulges_below_its_chord_going_right() {
        // From (-1, 0) to (1, 0) counter-clockwise: about the origin, through (0, -1).
        let points = through(p(-1000, 0), p(1000, 0), Millideg(180_000));
        assert_eq!(points.len(), 37);
        assert_eq!(points[18], p(0, -1000));
        assert_eq!(*points.last().unwrap(), p(1000, 0));
    }

    #[test]
    fn a_quarter_turn_clockwise_bulges_the_other_way() {
        let points = through(p(0, 1000), p(1000, 0), Millideg(-90_000));
        // About the origin: the middle point is on the unit circle at 45 degrees.
        assert_eq!(points[9], p(707, 707));
    }
}
