//! Spline and ellipse fit (docs/spec/v0.3/testing.md#property-tests): random splines and
//! ellipses are fitted within 0.005 mm at dense samples, and a circular ellipse gives one
//! true arc. The true curves are evaluated here independently: ellipses from their
//! parametric form, splines by summing the Cox–de Boor basis functions.

use std::f64::consts::TAU;

use proptest::prelude::*;
use reda_import_dxf::{Piece, Shape, Spline, Xy};

/// 0.005 mm, in a drawing in mm.
const TOLERANCE: f64 = 0.005;
const SAMPLES: usize = 3000;

fn distance(pieces: &[Piece], p: Xy) -> f64 {
    let dist = |a: Xy, b: Xy| (a.x - b.x).hypot(a.y - b.y);
    pieces
        .iter()
        .map(|piece| match *piece {
            Piece::Line { from, to } => {
                let (dx, dy) = (to.x - from.x, to.y - from.y);
                let len2 = dx * dx + dy * dy;
                let s = if len2 > 0.0 {
                    (((p.x - from.x) * dx + (p.y - from.y) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                dist(p, Xy::new(from.x + dx * s, from.y + dy * s))
            }
            Piece::Arc {
                from,
                to,
                sweep,
                centre,
                radius,
            } => {
                let a0 = (from.y - centre.y).atan2(from.x - centre.x);
                let a = (p.y - centre.y).atan2(p.x - centre.x);
                let d = if sweep >= 0.0 { a - a0 } else { a0 - a };
                if d.rem_euclid(TAU) <= sweep.to_radians().abs() {
                    (dist(p, centre) - radius).abs()
                } else {
                    dist(p, from).min(dist(p, to))
                }
            }
        })
        .fold(f64::INFINITY, f64::min)
}

fn ends(piece: &Piece) -> (Xy, Xy) {
    match *piece {
        Piece::Line { from, to } | Piece::Arc { from, to, .. } => (from, to),
    }
}

/// Every sample is within the tolerance of the fit, and the fit runs end to end from
/// `first` to `last`.
fn check_fit(
    pieces: &[Piece],
    samples: impl Iterator<Item = Xy>,
    first: Xy,
    last: Xy,
) -> Result<(), TestCaseError> {
    prop_assert!(!pieces.is_empty());
    for p in samples {
        let d = distance(pieces, p);
        prop_assert!(
            d <= TOLERANCE,
            "{:?} is {} from the fit of {} pieces",
            p,
            d,
            pieces.len()
        );
    }
    for pair in pieces.windows(2) {
        prop_assert_eq!(ends(&pair[0]).1, ends(&pair[1]).0);
    }
    let near = |a: Xy, b: Xy| (a.x - b.x).hypot(a.y - b.y) < 1e-9;
    prop_assert!(near(ends(&pieces[0]).0, first));
    prop_assert!(near(ends(pieces.last().unwrap()).1, last));
    Ok(())
}

/// N_{i,p}(t) by the recurrence's definition, with half-open spans.
fn basis(knots: &[f64], i: usize, p: usize, t: f64) -> f64 {
    if p == 0 {
        return if knots[i] <= t && t < knots[i + 1] {
            1.0
        } else {
            0.0
        };
    }
    let mut value = 0.0;
    let left = knots[i + p] - knots[i];
    if left > 0.0 {
        value += (t - knots[i]) / left * basis(knots, i, p - 1, t);
    }
    let right = knots[i + p + 1] - knots[i + 1];
    if right > 0.0 {
        value += (knots[i + p + 1] - t) / right * basis(knots, i + 1, p - 1, t);
    }
    value
}

fn nurbs(knots: &[f64], degree: usize, points: &[Xy], weights: &[f64], t: f64) -> Xy {
    let (mut x, mut y, mut w) = (0.0, 0.0, 0.0);
    for (i, (p, wi)) in points.iter().zip(weights).enumerate() {
        let b = basis(knots, i, degree, t) * wi;
        x += b * p.x;
        y += b * p.y;
        w += b;
    }
    Xy::new(x / w, y / w)
}

fn coordinate() -> impl Strategy<Value = f64> {
    -50.0..50.0f64
}

prop_compose! {
    /// A clamped spline: degree 1 to 5, up to 12 control points, distinct interior knots
    /// spaced anyhow in (0, 1), and weights either all 1 or random.
    fn spline()(degree in 1usize..=5)(
        degree in Just(degree),
        points in prop::collection::vec((coordinate(), coordinate()), degree + 1..=12),
        rational in any::<bool>(),
        seeds in prop::collection::vec((0.01..1.0f64, 0.2..5.0f64), 13),
    ) -> (usize, Vec<f64>, Vec<Xy>, Vec<f64>) {
        let points: Vec<Xy> = points.into_iter().map(|(x, y)| Xy::new(x, y)).collect();
        let n = points.len();
        let gaps: Vec<f64> = seeds.iter().take(n - degree).map(|s| s.0).collect();
        let total: f64 = gaps.iter().sum();
        let mut knots = vec![0.0; degree + 1];
        let mut at = 0.0;
        for gap in &gaps[..gaps.len() - 1] {
            at += gap / total;
            knots.push(at);
        }
        knots.extend(vec![1.0; degree + 1]);
        let weights = if rational { seeds.iter().take(n).map(|s| s.1).collect() } else { vec![1.0; n] };
        (degree, knots, points, weights)
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn random_ellipses_are_fitted_within_the_tolerance(
        cx in coordinate(), cy in coordinate(),
        major in 0.05..60.0f64, angle in 0.0..TAU, ratio in 0.005..1.0f64,
        start in 0.0..TAU, sweep in 0.01..TAU, flipped in any::<bool>(),
    ) {
        let (sin, cos) = angle.sin_cos();
        let u = Xy::new(major * cos, major * sin);
        let side = if flipped { -1.0 } else { 1.0 };
        let v = Xy::new(-side * ratio * u.y, side * ratio * u.x);
        let at = |t: f64| {
            let (s, c) = t.sin_cos();
            Xy::new(cx + u.x * c + v.x * s, cy + u.y * c + v.y * s)
        };
        let pieces = Shape::conic(Xy::new(cx, cy), u, v, start, sweep).pieces(TOLERANCE);
        let samples = (0..=SAMPLES).map(|i| at(start + sweep * i as f64 / SAMPLES as f64));
        check_fit(&pieces, samples, at(start), at(start + sweep))?;
    }

    #[test]
    fn a_circular_ellipse_gives_one_true_arc(
        cx in coordinate(), cy in coordinate(), radius in 0.05..60.0f64,
        angle in 0.0..TAU, start in 0.0..TAU, sweep in 0.01..(TAU - 0.01), flipped in any::<bool>(),
    ) {
        let (sin, cos) = angle.sin_cos();
        let u = Xy::new(radius * cos, radius * sin);
        let v = if flipped { Xy::new(u.y, -u.x) } else { Xy::new(-u.y, u.x) };
        let pieces = Shape::conic(Xy::new(cx, cy), u, v, start, sweep).pieces(TOLERANCE);
        prop_assert_eq!(pieces.len(), 1);
        let Piece::Arc { centre, radius: r, sweep: s, .. } = pieces[0] else {
            return Err(TestCaseError::fail("not an arc"));
        };
        prop_assert!((r - radius).abs() < 1e-9 * radius.max(1.0));
        prop_assert!((centre.x - cx).abs() < 1e-9 && (centre.y - cy).abs() < 1e-9);
        prop_assert!((s.abs() - sweep.to_degrees()).abs() < 1e-6, "{} vs {}", s, sweep.to_degrees());
    }

    #[test]
    fn random_splines_are_fitted_within_the_tolerance((degree, knots, points, weights) in spline()) {
        let spline = Spline::new(degree, knots.clone(), points.clone(), weights.clone()).unwrap();
        let pieces = Shape::Spline(spline).pieces(TOLERANCE);
        // Half-open spans leave out t = 1, where a clamped curve ends on its last point.
        let samples = (0..SAMPLES).map(|i| nurbs(&knots, degree, &points, &weights, i as f64 / SAMPLES as f64));
        check_fit(&pieces, samples, points[0], *points.last().unwrap())?;
    }
}
