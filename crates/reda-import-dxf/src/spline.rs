//! SPLINE geometry (DXF Reference, "SPLINE"): a non-uniform rational B-spline given by its
//! degree, knots, control points and weights, or a curve through fit points. Fit points
//! become a cubic B-spline that passes through each of them, parametrised by chord length,
//! with the file's end tangents or, without them, the tangents of the parabola through the
//! first and last three points.

use crate::shape::Xy;

/// The highest degree read. AutoCAD draws up to 11; higher is a broken file.
const MAX_DEGREE: usize = 25;

/// A NURBS curve in the plane.
#[derive(Clone, Debug, PartialEq)]
pub struct Spline {
    degree: usize,
    knots: Vec<f64>,
    points: Vec<Xy>,
    weights: Vec<f64>,
}

impl Spline {
    /// The curve of `points` and `weights` (one each, or no weights for all 1) over
    /// `knots`, `points.len() + degree + 1` of them, never decreasing. `None` when they
    /// don't make a curve.
    pub fn new(
        degree: usize,
        knots: Vec<f64>,
        points: Vec<Xy>,
        weights: Vec<f64>,
    ) -> Option<Spline> {
        let n = points.len();
        let weights = if weights.is_empty() {
            vec![1.0; n]
        } else {
            weights
        };
        let fine = (1..=MAX_DEGREE).contains(&degree)
            && n > degree
            && knots.len() == n + degree + 1
            && weights.len() == n
            && knots.iter().all(|k| k.is_finite())
            && knots.windows(2).all(|w| w[0] <= w[1])
            && knots[degree] < knots[n]
            && weights.iter().all(|w| w.is_finite() && *w > 0.0)
            && points.iter().all(|p| p.x.is_finite() && p.y.is_finite());
        fine.then_some(Spline {
            degree,
            knots,
            points,
            weights,
        })
    }

    /// The cubic through `fit` (two points at least, the same point twice in a row counting
    /// once), leaving the first point along `start` and reaching the last along `end` when
    /// given. Two points and no tangents make a straight line.
    pub fn through(fit: &[Xy], start: Option<Xy>, end: Option<Xy>) -> Option<Spline> {
        let mut q: Vec<Xy> = Vec::with_capacity(fit.len());
        for p in fit {
            if q.last() != Some(p) {
                q.push(*p);
            }
        }
        if q.len() < 2 || q.iter().any(|p| !(p.x.is_finite() && p.y.is_finite())) {
            return None;
        }
        let n = q.len() - 1;
        if n == 1 && start.is_none() && end.is_none() {
            return Spline::new(1, vec![0.0, 0.0, 1.0, 1.0], q, vec![]);
        }
        // Chord-length parameters from 0 to 1.
        let mut u = vec![0.0];
        for i in 1..=n {
            u.push(u[i - 1] + distance(q[i - 1], q[i]));
        }
        let total = u[n];
        if !(total > 0.0 && total.is_finite()) {
            return None;
        }
        u.iter_mut().for_each(|v| *v /= total);
        u[n] = 1.0;

        let slope = |i: usize| scale(sub(q[i + 1], q[i]), 1.0 / (u[i + 1] - u[i]));
        let tangent = |given: Option<Xy>, estimate: Xy| match given.and_then(unit) {
            Some(t) => scale(t, total),
            None => estimate,
        };
        let d0 = tangent(
            start,
            if n == 1 {
                slope(0)
            } else {
                let (h1, h2) = (u[1] - u[0], u[2] - u[1]);
                sub(
                    scale(slope(0), (2.0 * h1 + h2) / (h1 + h2)),
                    scale(slope(1), h1 / (h1 + h2)),
                )
            },
        );
        let dn = tangent(
            end,
            if n == 1 {
                slope(0)
            } else {
                let (h1, h2) = (u[n - 1] - u[n - 2], u[n] - u[n - 1]);
                sub(
                    scale(slope(n - 1), (2.0 * h2 + h1) / (h1 + h2)),
                    scale(slope(n - 2), h2 / (h1 + h2)),
                )
            },
        );

        // Clamped cubic knots with the interior parameters as single knots: n + 3 control
        // points. The first two and last two follow from the ends and their tangents.
        let mut knots = vec![0.0; 4];
        knots.extend(&u[1..n]);
        knots.extend([1.0; 4]);
        let mut p = vec![Xy::default(); n + 3];
        p[0] = q[0];
        p[1] = add(q[0], scale(d0, u[1] / 3.0));
        p[n + 1] = sub(q[n], scale(dn, (1.0 - u[n - 1]) / 3.0));
        p[n + 2] = q[n];
        // Each interior fit point k gives N_k P_k + N_{k+1} P_{k+1} + N_{k+2} P_{k+2} = Q_k:
        // a tridiagonal system in P_2 … P_n.
        let rows = n - 1;
        let (mut a, mut b, mut c, mut d) = (
            vec![0.0; rows],
            vec![0.0; rows],
            vec![0.0; rows],
            vec![Xy::default(); rows],
        );
        for (i, k) in (1..n).enumerate() {
            let basis = basis(&knots, 3, k + 3, u[k]);
            let (lower, middle, upper) = (basis[0], basis[1], basis[2]);
            d[i] = q[k];
            if k == 1 {
                d[i] = sub(d[i], scale(p[1], lower));
            } else {
                a[i] = lower;
            }
            if k == n - 1 {
                d[i] = sub(d[i], scale(p[n + 1], upper));
            } else {
                c[i] = upper;
            }
            b[i] = middle;
        }
        let solved = solve_tridiagonal(&a, &b, &mut c, &mut d)?;
        p[2..2 + rows].copy_from_slice(&solved);
        Spline::new(3, knots, p, vec![])
    }

    /// Where the curve is defined: from the knot at `degree` to the knot at the number of
    /// control points.
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.points.len()])
    }

    /// The distinct knots across the domain, where the curve may have corners.
    pub fn breaks(&self) -> Vec<f64> {
        let (t0, t1) = self.domain();
        let mut breaks: Vec<f64> = Vec::new();
        for k in &self.knots[self.degree..=self.points.len()] {
            if breaks.last() != Some(k) {
                breaks.push(*k);
            }
        }
        debug_assert!(breaks.first() == Some(&t0) && breaks.last() == Some(&t1));
        breaks
    }

    /// The point at parameter `t` within the domain, by de Boor's algorithm on the weighted
    /// control points.
    pub fn point(&self, t: f64) -> Xy {
        let p = self.degree;
        let n = self.points.len();
        // The span [k, k+1) holding t, the last non-empty one at the domain's end.
        let k = match self.knots[p..=n].partition_point(|knot| *knot <= t) {
            0 => p,
            i => (p + i - 1).min(n - 1),
        };
        let k = (p..=k)
            .rev()
            .find(|&k| self.knots[k] < self.knots[k + 1])
            .unwrap_or(p);
        let mut d: Vec<[f64; 3]> = (0..=p)
            .map(|j| {
                let (pt, w) = (self.points[j + k - p], self.weights[j + k - p]);
                [pt.x * w, pt.y * w, w]
            })
            .collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let left = self.knots[j + k - p];
                let right = self.knots[j + 1 + k - r];
                let alpha = if right > left {
                    (t - left) / (right - left)
                } else {
                    0.0
                };
                d[j] = [0, 1, 2].map(|i| (1.0 - alpha) * d[j - 1][i] + alpha * d[j][i]);
            }
        }
        let [x, y, w] = d[p];
        Xy::new(x / w, y / w)
    }
}

/// The `degree + 1` basis functions that are not zero on span `span` at `t`, from
/// `N_{span-degree}` to `N_span`, by the Cox–de Boor recurrence.
fn basis(knots: &[f64], degree: usize, span: usize, t: f64) -> Vec<f64> {
    let mut n = vec![0.0; degree + 1];
    let mut left = vec![0.0; degree + 1];
    let mut right = vec![0.0; degree + 1];
    n[0] = 1.0;
    for j in 1..=degree {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            let denominator = right[r + 1] + left[j - r];
            let temp = if denominator != 0.0 {
                n[r] / denominator
            } else {
                0.0
            };
            n[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        n[j] = saved;
    }
    n
}

/// Solves `a[i] x[i-1] + b[i] x[i] + c[i] x[i+1] = d[i]` by elimination down and back.
fn solve_tridiagonal(a: &[f64], b: &[f64], c: &mut [f64], d: &mut [Xy]) -> Option<Vec<Xy>> {
    let rows = b.len();
    if rows == 0 {
        return Some(Vec::new());
    }
    let mut beta = b[0];
    let mut x = vec![Xy::default(); rows];
    let pivot = |v: f64| (v.abs() > 1e-300 && v.is_finite()).then_some(v);
    beta = pivot(beta)?;
    d[0] = scale(d[0], 1.0 / beta);
    c[0] /= beta;
    for i in 1..rows {
        beta = pivot(b[i] - a[i] * c[i - 1])?;
        c[i] /= beta;
        d[i] = scale(sub(d[i], scale(d[i - 1], a[i])), 1.0 / beta);
    }
    x[rows - 1] = d[rows - 1];
    for i in (0..rows - 1).rev() {
        x[i] = sub(d[i], scale(x[i + 1], c[i]));
    }
    Some(x)
}

fn add(a: Xy, b: Xy) -> Xy {
    Xy::new(a.x + b.x, a.y + b.y)
}

fn sub(a: Xy, b: Xy) -> Xy {
    Xy::new(a.x - b.x, a.y - b.y)
}

fn scale(a: Xy, k: f64) -> Xy {
    Xy::new(a.x * k, a.y * k)
}

fn distance(a: Xy, b: Xy) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn unit(a: Xy) -> Option<Xy> {
    let l = a.x.hypot(a.y);
    (l > 0.0 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn near(a: Xy, b: Xy) {
        assert!(distance(a, b) < 1e-9, "{a:?} is not {b:?}");
    }

    #[test]
    fn a_quadratic_bezier_is_its_bernstein_form() {
        let s = Spline::new(
            2,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![Xy::new(0.0, 0.0), Xy::new(1.0, 2.0), Xy::new(2.0, 0.0)],
            vec![],
        )
        .unwrap();
        // (1-t)² P0 + 2t(1-t) P1 + t² P2 at t = 1/4: (0.5, 0.75).
        near(s.point(0.25), Xy::new(0.5, 0.75));
        near(s.point(1.0), Xy::new(2.0, 0.0));
    }

    #[test]
    fn a_rational_quadratic_with_the_right_weight_is_a_quarter_circle() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let s = Spline::new(
            2,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![Xy::new(1.0, 0.0), Xy::new(1.0, 1.0), Xy::new(0.0, 1.0)],
            vec![1.0, w, 1.0],
        )
        .unwrap();
        for i in 0..=10 {
            let p = s.point(i as f64 / 10.0);
            assert!((p.x.hypot(p.y) - 1.0).abs() < 1e-12, "{p:?}");
        }
    }

    #[test]
    fn bad_knots_weights_or_counts_make_no_curve() {
        let points = vec![Xy::new(0.0, 0.0), Xy::new(1.0, 0.0)];
        assert!(Spline::new(1, vec![0.0, 0.0, 1.0, 1.0], points.clone(), vec![]).is_some());
        assert!(Spline::new(1, vec![0.0, 1.0, 1.0], points.clone(), vec![]).is_none());
        assert!(Spline::new(1, vec![0.0, 1.0, 0.5, 1.0], points.clone(), vec![]).is_none());
        assert!(Spline::new(1, vec![1.0, 1.0, 1.0, 1.0], points.clone(), vec![]).is_none());
        assert!(Spline::new(1, vec![0.0, 0.0, 1.0, 1.0], points.clone(), vec![1.0, 0.0]).is_none());
        assert!(Spline::new(2, vec![0.0; 5], points, vec![]).is_none());
    }

    #[test]
    fn a_curve_through_fit_points_passes_through_each_one() {
        let fit = [
            Xy::new(0.0, 0.0),
            Xy::new(1.0, 2.0),
            Xy::new(3.0, 1.0),
            Xy::new(3.0, 1.0),
            Xy::new(4.0, 3.0),
            Xy::new(6.0, 0.0),
        ];
        let s = Spline::through(&fit, None, None).unwrap();
        let mut u = vec![0.0];
        let unique = [fit[0], fit[1], fit[2], fit[4], fit[5]];
        for w in unique.windows(2) {
            u.push(u.last().unwrap() + distance(w[0], w[1]));
        }
        let total = *u.last().unwrap();
        for (p, t) in unique.iter().zip(&u) {
            near(s.point(t / total), *p);
        }
    }

    #[test]
    fn end_tangents_set_the_direction_the_curve_leaves_and_arrives() {
        let fit = [Xy::new(0.0, 0.0), Xy::new(5.0, 0.0), Xy::new(10.0, 0.0)];
        let s = Spline::through(&fit, Some(Xy::new(0.0, 1.0)), Some(Xy::new(0.0, -3.0))).unwrap();
        let just_after = s.point(1e-6);
        assert!(
            just_after.y > 0.0 && just_after.x.abs() < just_after.y,
            "{just_after:?}"
        );
        let just_before = s.point(1.0 - 1e-6);
        assert!(just_before.y > 0.0, "{just_before:?}");
        near(s.point(0.5), Xy::new(5.0, 0.0));
    }

    #[test]
    fn two_fit_points_without_tangents_are_a_line() {
        let s = Spline::through(&[Xy::new(0.0, 0.0), Xy::new(2.0, 2.0)], None, None).unwrap();
        near(s.point(0.5), Xy::new(1.0, 1.0));
    }
}
