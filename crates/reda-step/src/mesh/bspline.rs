//! B-spline curves and surfaces, rational or not (ISO 10303-42): the knot vector expanded from
//! its multiplicities, and points with their first and second derivatives by the basis
//! functions' derivatives (de Boor, after Piegl and Tiller's *The NURBS Book*, chapter 2), a
//! rational one by the quotient rule on its homogeneous points.

use super::geometry::{V3, point};
use crate::StepModel;
use crate::records::{BSplineCurve, BSplineSurface, RecordId};

/// Most control points in either direction, and in all, that a spline may have.
const MAX_POINTS: usize = 1 << 20;
/// Highest degree evaluated.
const MAX_DEGREE: usize = 25;

/// A knot vector with its degree: `knots.len() == count + degree + 1`.
#[derive(Clone, Debug)]
pub(crate) struct Knots {
    pub degree: usize,
    pub knots: Vec<f64>,
}

impl Knots {
    /// The knots from their distinct values and multiplicities, for `count` control points.
    fn expand(degree: i64, values: &[f64], mults: &[i64], count: usize) -> Result<Knots, String> {
        if !(1..=MAX_DEGREE as i64).contains(&degree) {
            return Err(format!("its degree {degree} is not one that is evaluated"));
        }
        let degree = degree as usize;
        if values.len() != mults.len() || values.is_empty() {
            return Err("its knots and their multiplicities differ in number".into());
        }
        let mut knots = Vec::new();
        for (&k, &m) in values.iter().zip(mults) {
            if !(1..=degree as i64 + 1).contains(&m) || !k.is_finite() {
                return Err("a knot with a bad multiplicity or value".into());
            }
            if knots.last().is_some_and(|&l: &f64| k < l) {
                return Err("its knots are not in order".into());
            }
            knots.extend(std::iter::repeat_n(k, m as usize));
            if knots.len() > MAX_POINTS {
                return Err("too many knots".into());
            }
        }
        if knots.len() != count + degree + 1 {
            return Err(format!(
                "{} knots do not fit {count} control points of degree {degree}",
                knots.len()
            ));
        }
        let k = Knots { degree, knots };
        let (a, b) = k.domain();
        if b.partial_cmp(&a) != Some(std::cmp::Ordering::Greater) {
            return Err("its parameter range is empty".into());
        }
        Ok(k)
    }

    /// The parameter range: from the knot at `degree` to the one at `count`.
    pub fn domain(&self) -> (f64, f64) {
        let n = self.knots.len() - self.degree - 1;
        (self.knots[self.degree], self.knots[n])
    }

    /// The distinct knot values within the domain, its ends included.
    pub fn breaks(&self) -> Vec<f64> {
        let (a, b) = self.domain();
        let mut out: Vec<f64> = Vec::new();
        for &k in &self.knots {
            if k >= a && k <= b && out.last() != Some(&k) {
                out.push(k);
            }
        }
        out
    }

    /// The span `i` with `knots[i] <= t < knots[i + 1]`, clamped to the domain (the last
    /// span for its end).
    fn span(&self, t: f64) -> usize {
        let p = self.degree;
        let n = self.knots.len() - p - 1;
        let (a, b) = self.domain();
        if t >= b {
            let mut i = n - 1;
            while i > p && self.knots[i] >= b {
                i -= 1;
            }
            return i;
        }
        if t <= a {
            let mut i = p;
            while i + 1 < n && self.knots[i + 1] <= a {
                i += 1;
            }
            return i;
        }
        let (mut lo, mut hi) = (p, n);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if t < self.knots[mid] {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        lo
    }

    /// The nonzero basis functions at `t` and their derivatives up to the second:
    /// `ders[k][j]` is the `k`th derivative of `N(span - degree + j)`.
    fn basis(&self, t: f64) -> (usize, [Vec<f64>; 3]) {
        let p = self.degree;
        let u = &self.knots;
        let span = self.span(t);
        let mut ndu = vec![vec![0.0; p + 1]; p + 1];
        let mut left = vec![0.0; p + 1];
        let mut right = vec![0.0; p + 1];
        ndu[0][0] = 1.0;
        for j in 1..=p {
            left[j] = t - u[span + 1 - j];
            right[j] = u[span + j] - t;
            let mut saved = 0.0;
            for r in 0..j {
                ndu[j][r] = right[r + 1] + left[j - r];
                let temp = if ndu[j][r] != 0.0 {
                    ndu[r][j - 1] / ndu[j][r]
                } else {
                    0.0
                };
                ndu[r][j] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            ndu[j][j] = saved;
        }
        let nd = 2.min(p);
        let mut ders = [vec![0.0; p + 1], vec![0.0; p + 1], vec![0.0; p + 1]];
        for j in 0..=p {
            ders[0][j] = ndu[j][p];
        }
        let mut a = [vec![0.0; p + 1], vec![0.0; p + 1]];
        for r in 0..=p {
            let (mut s1, mut s2) = (0usize, 1usize);
            a[0][0] = 1.0;
            for k in 1..=nd {
                let mut d = 0.0;
                let rk = r as i64 - k as i64;
                let pk = p - k;
                if r >= k {
                    let rk = rk as usize;
                    a[s2][0] = if ndu[pk + 1][rk] != 0.0 {
                        a[s1][0] / ndu[pk + 1][rk]
                    } else {
                        0.0
                    };
                    d = a[s2][0] * ndu[rk][pk];
                }
                let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
                let j2 = if r as i64 - 1 <= pk as i64 {
                    k - 1
                } else {
                    p - r
                };
                for j in j1..=j2 {
                    let idx = (rk + j as i64) as usize;
                    a[s2][j] = if ndu[pk + 1][idx] != 0.0 {
                        (a[s1][j] - a[s1][j - 1]) / ndu[pk + 1][idx]
                    } else {
                        0.0
                    };
                    d += a[s2][j] * ndu[idx][pk];
                }
                if r <= pk {
                    a[s2][k] = if ndu[pk + 1][r] != 0.0 {
                        -a[s1][k - 1] / ndu[pk + 1][r]
                    } else {
                        0.0
                    };
                    d += a[s2][k] * ndu[r][pk];
                }
                ders[k][r] = d;
                std::mem::swap(&mut s1, &mut s2);
            }
        }
        let mut f = p as f64;
        for (k, row) in ders.iter_mut().enumerate().skip(1).take(nd) {
            for x in row.iter_mut() {
                *x *= f;
            }
            f *= (p - k) as f64;
        }
        (span, ders)
    }
}

/// A point with weight `w`, stored as `(w·x, w·y, w·z, w)`.
#[derive(Clone, Copy, Debug, Default)]
struct H(V3, f64);

impl H {
    fn add_scaled(&mut self, o: H, s: f64) {
        self.0 = self.0 + o.0 * s;
        self.1 += o.1 * s;
    }
}

fn weight(w: f64) -> Result<f64, String> {
    if w > 0.0 && w.is_finite() {
        Ok(w)
    } else {
        Err("a weight that is not positive".into())
    }
}

/// A B-spline curve in 3D.
#[derive(Clone, Debug)]
pub(crate) struct SplineCurve {
    pub knots: Knots,
    points: Vec<H>,
    /// Whether it ends where it starts.
    pub closed: bool,
}

impl SplineCurve {
    pub fn of(model: &StepModel, c: &BSplineCurve) -> Result<SplineCurve, String> {
        let n = c.control_points_list.len();
        if !(2..=MAX_POINTS).contains(&n) {
            return Err("a B-spline curve with too few or too many control points".into());
        }
        let knots = Knots::expand(c.degree, &c.knots, &c.knot_multiplicities, n)?;
        if let Some(w) = &c.weights
            && w.len() != n
        {
            return Err("its weights and control points differ in number".into());
        }
        let mut points = Vec::with_capacity(n);
        for (i, &id) in c.control_points_list.iter().enumerate() {
            let p = control_point(model, id)?;
            let w = match &c.weights {
                Some(w) => weight(w[i])?,
                None => 1.0,
            };
            points.push(H(p * w, w));
        }
        let mut c = SplineCurve {
            knots,
            points,
            closed: false,
        };
        c.closed = c.is_closed();
        Ok(c)
    }

    /// The point and its first and second derivatives at `t` (clamped to the domain).
    pub fn derivs(&self, t: f64) -> [V3; 3] {
        let (a, b) = self.knots.domain();
        let t = t.clamp(a, b);
        let (span, n) = self.knots.basis(t);
        let p = self.knots.degree;
        let mut h = [H::default(); 3];
        for (j, &cp) in self.points[span - p..=span].iter().enumerate() {
            for (hk, nk) in h.iter_mut().zip(&n) {
                hk.add_scaled(cp, nk[j]);
            }
        }
        let w = h[0].1;
        let c = h[0].0 * (1.0 / w);
        let d1 = (h[1].0 - c * h[1].1) * (1.0 / w);
        let d2 = (h[2].0 - d1 * (2.0 * h[1].1) - c * h[2].1) * (1.0 / w);
        [c, d1, d2]
    }

    pub fn eval(&self, t: f64) -> V3 {
        self.derivs(t)[0]
    }

    /// Whether the curve ends where it starts.
    fn is_closed(&self) -> bool {
        let (a, b) = self.knots.domain();
        let (p, q) = (self.eval(a), self.eval(b));
        (p - q).length() <= CLOSE * self.size().max(1.0)
    }

    /// The size of the control polygon's box.
    pub fn size(&self) -> f64 {
        box_size(self.points.iter().map(|h| h.0 * (1.0 / h.1)))
    }
}

fn box_size(points: impl Iterator<Item = V3>) -> f64 {
    let mut lo = V3(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = -lo;
    for p in points {
        lo = V3(lo.0.min(p.0), lo.1.min(p.1), lo.2.min(p.2));
        hi = V3(hi.0.max(p.0), hi.1.max(p.1), hi.2.max(p.2));
    }
    (hi - lo).length()
}

fn control_point(model: &StepModel, id: RecordId) -> Result<V3, String> {
    point(model, id).ok_or_else(|| format!("control point #{} cannot be read", id.index() + 1))
}

/// A point of a surface with its derivatives.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SurfacePoint {
    pub p: V3,
    pub u: V3,
    pub v: V3,
    pub uu: V3,
    pub uv: V3,
    pub vv: V3,
}

/// A B-spline surface: rows of control points along `u`, each row along `v`.
#[derive(Clone, Debug)]
pub(crate) struct SplineSurface {
    pub u: Knots,
    pub v: Knots,
    rows: usize,
    cols: usize,
    points: Vec<H>,
}

impl SplineSurface {
    pub fn of(model: &StepModel, s: &BSplineSurface) -> Result<SplineSurface, String> {
        let rows = s.control_points_list.len();
        let cols = s.control_points_list.first().map_or(0, Vec::len);
        if rows < 2 || cols < 2 || rows * cols > MAX_POINTS {
            return Err("a B-spline surface with too few or too many control points".into());
        }
        if s.control_points_list.iter().any(|r| r.len() != cols) {
            return Err("its rows of control points differ in length".into());
        }
        let u = Knots::expand(s.u_degree, &s.u_knots, &s.u_multiplicities, rows)?;
        let v = Knots::expand(s.v_degree, &s.v_knots, &s.v_multiplicities, cols)?;
        if let Some(w) = &s.weights
            && (w.len() != rows || w.iter().any(|r| r.len() != cols))
        {
            return Err("its weights and control points differ in number".into());
        }
        let mut points = Vec::with_capacity(rows * cols);
        for (i, row) in s.control_points_list.iter().enumerate() {
            for (j, &id) in row.iter().enumerate() {
                let p = control_point(model, id)?;
                let w = match &s.weights {
                    Some(w) => weight(w[i][j])?,
                    None => 1.0,
                };
                points.push(H(p * w, w));
            }
        }
        Ok(SplineSurface {
            u,
            v,
            rows,
            cols,
            points,
        })
    }

    /// The point and its derivatives up to the second at `(u, v)`, clamped to the domain.
    pub fn derivs(&self, u: f64, v: f64) -> SurfacePoint {
        let (u0, u1) = self.u.domain();
        let (v0, v1) = self.v.domain();
        let (su, nu) = self.u.basis(u.clamp(u0, u1));
        let (sv, nv) = self.v.basis(v.clamp(v0, v1));
        let (p, q) = (self.u.degree, self.v.degree);
        // h[k][l]: the k-th u and l-th v derivative of the homogeneous point.
        let mut h = [[H::default(); 3]; 3];
        let rows = self.points.chunks(self.cols).skip(su - p).take(p + 1);
        for (i, row) in rows.enumerate() {
            for (j, &cp) in row[sv - q..=sv].iter().enumerate() {
                for (k, hk) in h.iter_mut().enumerate() {
                    if nu[k][i] == 0.0 {
                        continue;
                    }
                    for (l, hkl) in hk.iter_mut().enumerate().take(3 - k) {
                        hkl.add_scaled(cp, nu[k][i] * nv[l][j]);
                    }
                }
            }
        }
        let w = h[0][0].1;
        let inv = 1.0 / w;
        let s = h[0][0].0 * inv;
        let (wu, wv) = (h[1][0].1, h[0][1].1);
        let su_ = (h[1][0].0 - s * wu) * inv;
        let sv_ = (h[0][1].0 - s * wv) * inv;
        let suu = (h[2][0].0 - su_ * (2.0 * wu) - s * h[2][0].1) * inv;
        let svv = (h[0][2].0 - sv_ * (2.0 * wv) - s * h[0][2].1) * inv;
        let suv = (h[1][1].0 - su_ * wv - sv_ * wu - s * h[1][1].1) * inv;
        SurfacePoint {
            p: s,
            u: su_,
            v: sv_,
            uu: suu,
            uv: suv,
            vv: svv,
        }
    }

    pub fn eval(&self, u: f64, v: f64) -> V3 {
        self.derivs(u, v).p
    }

    /// The size of the control net's box.
    pub fn size(&self) -> f64 {
        box_size(self.points.iter().map(|h| h.0 * (1.0 / h.1)))
    }

    /// Points along the boundary at the start or end (`last`) of `u` (`along_u`, where
    /// `u` is fixed and `v` runs) or of `v`.
    fn boundary(&self, along_u: bool, last: bool) -> Vec<V3> {
        let (fixed, running) = if along_u {
            (&self.u, &self.v)
        } else {
            (&self.v, &self.u)
        };
        let (f0, f1) = fixed.domain();
        let f = if last { f1 } else { f0 };
        let (r0, r1) = running.domain();
        let n = 4 * (self.rows.max(self.cols)).clamp(4, 64);
        (0..=n)
            .map(|i| {
                let r = r0 + (r1 - r0) * i as f64 / n as f64;
                if along_u {
                    self.eval(f, r)
                } else {
                    self.eval(r, f)
                }
            })
            .collect()
    }

    fn tolerance(&self) -> f64 {
        CLOSE * self.size().max(1.0)
    }

    /// Whether the surface closes up along `u` (`along_u`) or `v`: its boundaries at the
    /// start and the end of that parameter's range are one curve.
    pub fn closes(&self, along_u: bool) -> bool {
        let tol = self.tolerance();
        let a = self.boundary(along_u, false);
        let b = self.boundary(along_u, true);
        a.iter().zip(&b).all(|(p, q)| (*p - *q).length() <= tol)
    }

    /// The point a boundary collapses to, if it does: the one at the start or end (`last`)
    /// of `u` (`along_u`, where `v` then means nothing) or of `v` (where `u` means nothing).
    pub fn collapsed(&self, along_u: bool, last: bool) -> Option<V3> {
        let tol = self.tolerance();
        let pts = self.boundary(along_u, last);
        pts.iter()
            .all(|&p| (p - pts[0]).length() <= tol)
            .then_some(pts[0])
    }
}

/// How near, relative to a spline's size, two of its points count as one: where it closes on
/// itself or collapses to a pole.
const CLOSE: f64 = 1e-7;

#[cfg(test)]
mod tests {
    use super::*;

    fn knots(degree: usize, knots: &[f64]) -> Knots {
        Knots {
            degree,
            knots: knots.to_vec(),
        }
    }

    #[test]
    fn basis_functions_sum_to_one_and_their_derivatives_to_zero() {
        // The NURBS Book, example 2.3: degree 2 on {0, 0, 0, 1, 2, 3, 4, 4, 5, 5, 5}.
        let k = knots(2, &[0., 0., 0., 1., 2., 3., 4., 4., 5., 5., 5.]);
        for t in [0.0, 0.3, 1.0, 2.5, 3.99, 4.0, 4.5, 5.0] {
            let (_, d) = k.basis(t);
            assert!((d[0].iter().sum::<f64>() - 1.0).abs() < 1e-12, "{t}");
            assert!(d[1].iter().sum::<f64>().abs() < 1e-12, "{t}");
            assert!(d[2].iter().sum::<f64>().abs() < 1e-12, "{t}");
        }
        // At t = 5/2: N(2,2) = 1/8, N(3,2) = 6/8, N(4,2) = 1/8 (The NURBS Book, 2.3).
        let (span, d) = k.basis(2.5);
        assert_eq!(span, 4);
        for (got, want) in d[0].iter().zip([0.125, 0.75, 0.125]) {
            assert!((got - want).abs() < 1e-12);
        }
        // Their derivatives: -1/2, 0, 1/2.
        for (got, want) in d[1].iter().zip([-0.5, 0.0, 0.5]) {
            assert!((got - want).abs() < 1e-12);
        }
    }
}
