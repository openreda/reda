//! The surfaces without a closed-form inverse: B-spline surfaces, surfaces of revolution and
//! surfaces of linear extrusion. Each gives its point and derivatives at `(u, v)`; a point's
//! `(u, v)` is found by search and Newton steps, near the point before along a bound when there
//! is one, which is how each edge's trimming curve in the surface's parameters is fitted.

use std::cell::OnceCell;
use std::f64::consts::TAU;

use super::MeshOptions;
use super::bspline::{SplineSurface, SurfacePoint};
use super::curve::Curve;
use super::geometry::{Frame, V3, direction, point};
use super::surface::{Lost, UvBox};
use crate::StepModel;
use crate::records::{
    Axis1Placement, BSplineSurface, RecordId, SurfaceOfLinearExtrusion, SurfaceOfRevolution, Vector,
};

/// How near a collapsed boundary or an axis, relative to the surface's size, a point counts
/// as on it.
const SINGULAR: f64 = 1e-6;

/// Wraps `x` into `[a, a + period)`, or leaves it when there is no period.
fn wrap(x: f64, a: f64, period: Option<f64>) -> f64 {
    match period {
        Some(p) => a + (x - a).rem_euclid(p),
        None => x,
    }
}

// ---------------------------------------------------------------------------------------------

/// A B-spline surface ready to mesh: its periods where it closes on itself, the points its
/// boundaries collapse to, and a coarse grid of its points for finding a point's `(u, v)`.
#[derive(Clone, Debug)]
pub(crate) struct Spline {
    s: SplineSurface,
    periods: (Option<f64>, Option<f64>),
    poles: Vec<(V3, Lost)>,
    size: f64,
    grid: OnceCell<Vec<(f64, f64, V3)>>,
}

impl Spline {
    pub fn of(model: &StepModel, b: &BSplineSurface) -> Result<Spline, String> {
        let s = SplineSurface::of(model, b)?;
        let period = |k: &super::bspline::Knots, closes: bool| {
            let (a, b) = k.domain();
            closes.then_some(b - a)
        };
        let periods = (period(&s.u, s.closes(true)), period(&s.v, s.closes(false)));
        let mut poles = Vec::new();
        for last in [false, true] {
            // A collapsed first or last row (u at an end) leaves v meaningless there.
            if let Some(p) = s.collapsed(true, last) {
                poles.push((p, Lost::V));
            }
            if let Some(p) = s.collapsed(false, last) {
                poles.push((p, Lost::U));
            }
        }
        let size = s.size().max(1e-9);
        Ok(Spline {
            s,
            periods,
            poles,
            size,
            grid: OnceCell::new(),
        })
    }

    pub fn derivs(&self, u: f64, v: f64) -> SurfacePoint {
        let (u0, _) = self.s.u.domain();
        let (v0, _) = self.s.v.domain();
        self.s
            .derivs(wrap(u, u0, self.periods.0), wrap(v, v0, self.periods.1))
    }

    pub fn periods(&self) -> (Option<f64>, Option<f64>) {
        self.periods
    }

    pub fn domain(&self) -> UvBox {
        UvBox {
            u: self.s.u.domain(),
            v: self.s.v.domain(),
        }
    }

    pub fn lost(&self, p: V3) -> Option<Lost> {
        let tol = SINGULAR * self.size.max(1.0);
        self.poles
            .iter()
            .find(|(q, _)| (p - *q).length() <= tol)
            .map(|&(_, l)| l)
    }

    /// The ends of the `v` range (`across` 1) or the `u` range (`across` 0), each a pole
    /// where that boundary collapses to a point; none in a direction that wraps.
    pub fn ends(&self, across: usize) -> (Option<End>, Option<End>) {
        let along_u = across == 0;
        let (k, period) = if along_u {
            (&self.s.u, self.periods.0)
        } else {
            (&self.s.v, self.periods.1)
        };
        if period.is_some() {
            return (None, None);
        }
        let (a, b) = k.domain();
        let end = |last: bool, at: f64| {
            Some(End {
                at,
                pole: self.s.collapsed(along_u, last).is_some(),
            })
        };
        (end(false, a), end(true, b))
    }

    fn grid(&self) -> &[(f64, f64, V3)] {
        self.grid.get_or_init(|| {
            let d = self.domain();
            let count = |k: &super::bspline::Knots| {
                ((k.breaks().len() - 1) * (k.degree + 1) * 2).clamp(8, 96)
            };
            let (nu, nv) = (count(&self.s.u), count(&self.s.v));
            let mut g = Vec::with_capacity((nu + 1) * (nv + 1));
            for i in 0..=nu {
                let u = d.u.0 + (d.u.1 - d.u.0) * i as f64 / nu as f64;
                for j in 0..=nv {
                    let v = d.v.0 + (d.v.1 - d.v.0) * j as f64 / nv as f64;
                    g.push((u, v, self.s.eval(u, v)));
                }
            }
            g
        })
    }

    /// `(u, v)` of a point on (or near) the surface, searched from `seed` when given and
    /// from the nearest point of a coarse grid when that misses.
    pub fn project(&self, p: V3, seed: Option<(f64, f64)>) -> (f64, f64) {
        let tol = 1e-7 * self.size.max(1.0);
        if let Some(seed) = seed {
            let (uv, d) = self.newton(p, seed);
            if d <= tol {
                return uv;
            }
            let (uv2, d2) = self.global(p);
            return if d2 < d { uv2 } else { uv };
        }
        self.global(p).0
    }

    fn global(&self, p: V3) -> ((f64, f64), f64) {
        let grid = self.grid();
        let mut order: Vec<(f64, usize)> = grid
            .iter()
            .enumerate()
            .map(|(i, g)| ((g.2 - p).length(), i))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut best = ((grid[order[0].1].0, grid[order[0].1].1), f64::INFINITY);
        // The few nearest grid points, in case the nearest lies across a fold.
        for &(_, i) in order.iter().take(4) {
            let (uv, d) = self.newton(p, (grid[i].0, grid[i].1));
            if d < best.1 {
                best = (uv, d);
            }
        }
        best
    }

    /// Newton steps on the distance from `p`, kept within the domain where it does not wrap.
    fn newton(&self, p: V3, (mut u, mut v): (f64, f64)) -> ((f64, f64), f64) {
        let d = self.domain();
        newton(
            |u, v| self.derivs(u, v),
            p,
            (&mut u, &mut v),
            |u, v| {
                if self.periods.0.is_none() {
                    *u = u.clamp(d.u.0, d.u.1);
                }
                if self.periods.1.is_none() {
                    *v = v.clamp(d.v.0, d.v.1);
                }
            },
        )
    }
}

/// Up to 40 damped Gauss-Newton steps from `(u, v)` towards the point of the surface nearest
/// `p`; `keep` holds the parameters in range. The result and its distance from `p`.
fn newton(
    derivs: impl Fn(f64, f64) -> SurfacePoint,
    p: V3,
    (u, v): (&mut f64, &mut f64),
    keep: impl Fn(&mut f64, &mut f64),
) -> ((f64, f64), f64) {
    keep(u, v);
    let mut s = derivs(*u, *v);
    let mut dist = (s.p - p).length();
    for _ in 0..40 {
        let r = s.p - p;
        let (a, b, c) = (s.u.dot(s.u), s.u.dot(s.v), s.v.dot(s.v));
        let (g0, g1) = (r.dot(s.u), r.dot(s.v));
        // A little damping keeps the step finite where a derivative vanishes (a pole).
        let damp = 1e-12 * (a + c) + 1e-300;
        let (a, c) = (a + damp, c + damp);
        let det = a * c - b * b;
        if det.abs() < 1e-300 {
            break;
        }
        let du = -(c * g0 - b * g1) / det;
        let dv = -(a * g1 - b * g0) / det;
        let mut step = 1.0;
        let mut improved = false;
        for _ in 0..8 {
            let (mut nu, mut nv) = (*u + du * step, *v + dv * step);
            keep(&mut nu, &mut nv);
            let ns = derivs(nu, nv);
            let nd = (ns.p - p).length();
            if nd < dist {
                (*u, *v, s, dist) = (nu, nv, ns, nd);
                improved = true;
                break;
            }
            step *= 0.5;
        }
        if !improved || (du.abs() + dv.abs()) * step < 1e-15 * (1.0 + u.abs() + v.abs()) {
            break;
        }
    }
    ((*u, *v), dist)
}

// ---------------------------------------------------------------------------------------------

/// Where a surface's parameter range ends across the direction a face runs around it, for
/// closing such a face: at `at`, a pole (the surface meets its axis there) or an open edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct End {
    pub at: f64,
    pub pole: bool,
}

/// A curve swept around an axis: `S(u, v)` is the curve's point at `v` turned by `u` about
/// the axis (ISO 10303-42), so `u` is the angle and `v` the curve's parameter.
#[derive(Clone, Debug)]
pub(crate) struct Revolution {
    curve: Curve,
    /// The axis is the frame's z; x and y are any pair across it.
    axis: Frame,
    size: f64,
}

impl Revolution {
    pub fn of(model: &StepModel, s: &SurfaceOfRevolution) -> Result<Revolution, String> {
        let curve = Curve::of(model, s.swept_curve)?;
        let a = model
            .get::<Axis1Placement>(s.axis_position)
            .ok_or("its axis cannot be read")?;
        let origin = point(model, a.location).ok_or("its axis cannot be read")?;
        let z = match a.axis {
            Some(d) => direction(model, d).ok_or("its axis cannot be read")?,
            None => V3::Z,
        };
        let axis = Frame::from_axes(origin, z, V3::X).ok_or("its axis cannot be read")?;
        let size = curve_size(&curve, origin);
        Ok(Revolution { curve, axis, size })
    }

    fn local(&self, t: f64) -> [V3; 3] {
        let [c, d1, d2] = self.curve.derivs(t);
        let f = self.axis;
        let l = |w: V3| V3(w.dot(f.x), w.dot(f.y), w.dot(f.z));
        [f.to_local(c), l(d1), l(d2)]
    }

    pub fn derivs(&self, u: f64, v: f64) -> SurfacePoint {
        let [c, d1, d2] = self.local(v);
        let (cs, sn) = (u.cos(), u.sin());
        let rot = |w: V3| V3(w.0 * cs - w.1 * sn, w.0 * sn + w.1 * cs, w.2);
        let rot_u = |w: V3| V3(-w.0 * sn - w.1 * cs, w.0 * cs - w.1 * sn, 0.0);
        let rot_uu = |w: V3| V3(-w.0 * cs + w.1 * sn, -w.0 * sn - w.1 * cs, 0.0);
        let f = self.axis;
        let world = |w: V3| f.x * w.0 + f.y * w.1 + f.z * w.2;
        SurfacePoint {
            p: f.to_world(rot(c)),
            u: world(rot_u(c)),
            v: world(rot(d1)),
            uu: world(rot_uu(c)),
            uv: world(rot_u(d1)),
            vv: world(rot(d2)),
        }
    }

    pub fn periods(&self) -> (Option<f64>, Option<f64>) {
        (Some(TAU), self.curve.period())
    }

    pub fn lost(&self, p: V3) -> Option<Lost> {
        let l = self.axis.to_local(p);
        (l.0.hypot(l.1) <= SINGULAR * self.size.max(1.0)).then_some(Lost::U)
    }

    pub fn project(&self, p: V3) -> (f64, f64) {
        let f = self.axis;
        let meridian = |q: V3| {
            let l = f.to_local(q);
            V3(l.0.hypot(l.1), l.2, 0.0)
        };
        let v = self.curve.closest(p, &meridian);
        let (lp, lc) = (f.to_local(p), self.local(v)[0]);
        (lp.1.atan2(lp.0) - lc.1.atan2(lc.0), v)
    }

    pub fn ends(&self) -> (Option<End>, Option<End>) {
        if self.curve.period().is_some() {
            return (None, None);
        }
        let Some((a, b)) = self.curve.range() else {
            return (None, None);
        };
        let end = |at: f64| {
            let l = self.local(at)[0];
            Some(End {
                at,
                pole: l.0.hypot(l.1) <= SINGULAR * self.size.max(1.0),
            })
        };
        (end(a), end(b))
    }
}

/// A rough size of a curve: its range's extent from `origin`, for tolerances.
fn curve_size(curve: &Curve, origin: V3) -> f64 {
    match curve.range() {
        Some((a, b)) => (0..=16)
            .map(|i| (curve.eval(a + (b - a) * i as f64 / 16.0) - origin).length())
            .fold(0.0, f64::max),
        None => (curve.eval(0.0) - origin).length() + curve.derivs(0.0)[1].length(),
    }
}

/// A curve swept along a vector: `S(u, v) = C(u) + v·d` (ISO 10303-42).
#[derive(Clone, Debug)]
pub(crate) struct Extrusion {
    curve: Curve,
    d: V3,
}

impl Extrusion {
    pub fn of(model: &StepModel, s: &SurfaceOfLinearExtrusion) -> Result<Extrusion, String> {
        let curve = Curve::of(model, s.swept_curve)?;
        let bad = "its extrusion vector cannot be read";
        let vector: &Vector = model.get::<Vector>(s.extrusion_axis).ok_or(bad)?;
        let dir = direction(model, vector.orientation).ok_or(bad)?;
        if !(vector.magnitude > 0.0 && vector.magnitude.is_finite()) {
            return Err(bad.into());
        }
        Ok(Extrusion {
            curve,
            d: dir * vector.magnitude,
        })
    }

    pub fn derivs(&self, u: f64, v: f64) -> SurfacePoint {
        let [c, d1, d2] = self.curve.derivs(u);
        SurfacePoint {
            p: c + self.d * v,
            u: d1,
            v: self.d,
            uu: d2,
            uv: V3::default(),
            vv: V3::default(),
        }
    }

    pub fn periods(&self) -> (Option<f64>, Option<f64>) {
        (self.curve.period(), None)
    }

    pub fn project(&self, p: V3) -> (f64, f64) {
        let d = self.d;
        let dd = d.dot(d);
        let across = |q: V3| q - d * (q.dot(d) / dd);
        let u = self.curve.closest(p, &across);
        (u, (p - self.curve.eval(u)).dot(d) / dd)
    }
}

// ---------------------------------------------------------------------------------------------

/// The parameter steps and the metric scale of a freeform surface over a face's parameter
/// box, from its derivatives sampled across the box: a step keeps the chord within the
/// tolerance where the surface bends most along it, and the angle within the angle
/// tolerance; a twisted patch (bending only across both directions) is cut so that each cell
/// stays within the chord. `None` along a direction in which the surface is straight.
pub(crate) fn steps_and_scale(
    derivs: impl Fn(f64, f64) -> SurfacePoint,
    b: UvBox,
    options: &MeshOptions,
    samples: (usize, usize),
) -> ((Option<f64>, Option<f64>), (f64, f64)) {
    let (nu, nv) = (samples.0.clamp(2, 64), samples.1.clamp(2, 64));
    let (mut du, mut dv) = (f64::INFINITY, f64::INFINITY);
    let mut twist: f64 = 0.0;
    let (mut lu_sum, mut lv_sum, mut n) = (0.0, 0.0, 0usize);
    let c = options.chord.max(1e-9);
    for i in 0..=nu {
        let u = b.u.0 + (b.u.1 - b.u.0) * i as f64 / nu as f64;
        for j in 0..=nv {
            let v = b.v.0 + (b.v.1 - b.v.0) * j as f64 / nv as f64;
            let s = derivs(u, v);
            let (lu, lv) = (s.u.length(), s.v.length());
            let Some(normal) = s.u.cross(s.v).unit() else {
                continue;
            };
            lu_sum += lu;
            lv_sum += lv;
            n += 1;
            let step = |bend: f64, speed: f64| {
                if bend <= 1e-12 * speed * speed {
                    f64::INFINITY
                } else {
                    (8.0 * c / bend).sqrt().min(options.angle * speed / bend)
                }
            };
            du = du.min(step(s.uu.dot(normal).abs(), lu));
            dv = dv.min(step(s.vv.dot(normal).abs(), lv));
            twist = twist.max(s.uv.dot(normal).abs());
        }
    }
    let scale = if n == 0 {
        (1.0, 1.0)
    } else {
        ((lu_sum / n as f64).max(1e-9), (lv_sum / n as f64).max(1e-9))
    };
    if twist > 1e-12 * scale.0 * scale.1 {
        // A cell du × dv strays about twist·du·dv/4 from its two triangles.
        let limit = 4.0 * c / twist;
        match (du.is_finite(), dv.is_finite()) {
            (false, false) => {
                du = (limit * scale.1 / scale.0).sqrt();
                dv = (limit * scale.0 / scale.1).sqrt();
            }
            (true, false) => dv = limit / du,
            (false, true) => du = limit / dv,
            (true, true) if du * dv > limit => {
                let k = (limit / (du * dv)).sqrt();
                du *= k;
                dv *= k;
            }
            _ => {}
        }
    }
    let finite = |x: f64| x.is_finite().then_some(x.max(1e-9));
    ((finite(du), finite(dv)), scale)
}

/// The number of samples to take along a curve's parameter over `range`: three per knot
/// span of a B-spline, a few otherwise.
pub(crate) fn curve_samples(curve: &Curve, range: (f64, f64)) -> usize {
    match curve {
        Curve::Spline(s) => {
            let (lo, hi) = (range.0.min(range.1), range.0.max(range.1));
            let inside = s
                .knots
                .breaks()
                .iter()
                .filter(|&&k| k > lo && k < hi)
                .count();
            3 * (inside + 1) + 6
        }
        _ => 12,
    }
}

/// The number of samples along a B-spline surface's `u` and `v` over a box.
pub(crate) fn spline_samples(s: &Spline, b: UvBox) -> (usize, usize) {
    let count = |k: &super::bspline::Knots, (lo, hi): (f64, f64)| {
        let inside = k.breaks().iter().filter(|&&x| x > lo && x < hi).count();
        3 * (inside + 1) + 6
    };
    (count(&s.s.u, b.u), count(&s.s.v, b.v))
}

impl Revolution {
    pub fn curve(&self) -> &Curve {
        &self.curve
    }
}

impl Extrusion {
    pub fn curve(&self) -> &Curve {
        &self.curve
    }
}

/// The surface record `id` names when it is a freeform one.
pub(crate) fn of(
    model: &StepModel,
    id: RecordId,
) -> Option<Result<super::surface::Surface, String>> {
    use super::surface::Surface;
    let wrap = |e: String| format!("surface #{}: {e}", id.index() + 1);
    if let Some(b) = model.get::<BSplineSurface>(id) {
        return Some(
            Spline::of(model, b)
                .map(|s| Surface::Spline(Box::new(s)))
                .map_err(wrap),
        );
    }
    if let Some(r) = model.get::<SurfaceOfRevolution>(id) {
        return Some(
            Revolution::of(model, r)
                .map(|s| Surface::Revolution(Box::new(s)))
                .map_err(wrap),
        );
    }
    if let Some(e) = model.get::<SurfaceOfLinearExtrusion>(id) {
        return Some(
            Extrusion::of(model, e)
                .map(|s| Surface::Extrusion(Box::new(s)))
                .map_err(wrap),
        );
    }
    None
}
