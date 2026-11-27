//! Curves: lines, conics and B-splines evaluated with their derivatives, a point's parameter
//! found on them, and each `EDGE_CURVE` sampled once, from its start vertex to its end vertex,
//! within the chord tolerance. Every face that uses an edge takes the same points, so
//! neighbouring faces meet without gaps.

use std::f64::consts::TAU;

use super::MeshOptions;
use super::bspline::SplineCurve;
use super::geometry::{Frame, V3, direction, point};
use super::surface::{arc_step, curve_step};
use crate::StepModel;
use crate::records::{
    BSplineCurve, Circle, EdgeCurve, Ellipse, Hyperbola, Line, Parabola, RecordId, SurfaceCurve,
    TrimmedCurve, Vector, VertexPoint,
};

/// A curve with its parameterisation (ISO 10303-42).
#[derive(Clone, Debug)]
pub(crate) enum Curve {
    /// `C(t) = o + t·d`.
    Line {
        origin: V3,
        dir: V3,
    },
    /// `C(t) = o + r·(cos t·x + sin t·y)`.
    Circle(Frame, f64),
    /// `C(t) = o + a·cos t·x + b·sin t·y`.
    Ellipse(Frame, f64, f64),
    /// `C(t) = o + a·(t²·x + 2t·y)`.
    Parabola(Frame, f64),
    /// `C(t) = o + a·cosh t·x + b·sinh t·y`.
    Hyperbola(Frame, f64, f64),
    Spline(Box<SplineCurve>),
}

impl Curve {
    /// The curve record `id` names (through trimmed and surface curves), or why it cannot be
    /// evaluated.
    pub fn of(model: &StepModel, id: RecordId) -> Result<Curve, String> {
        curve(model, id, 0)
    }

    /// The point and its first and second derivatives at `t`.
    pub fn derivs(&self, t: f64) -> [V3; 3] {
        let (c, s) = (t.cos(), t.sin());
        match self {
            Curve::Line { origin, dir } => [*origin + *dir * t, *dir, V3::default()],
            Curve::Circle(f, r) => [
                f.to_world(V3(r * c, r * s, 0.0)),
                f.x * (-r * s) + f.y * (r * c),
                f.x * (-r * c) + f.y * (-r * s),
            ],
            Curve::Ellipse(f, a, b) => [
                f.to_world(V3(a * c, b * s, 0.0)),
                f.x * (-a * s) + f.y * (b * c),
                f.x * (-a * c) + f.y * (-b * s),
            ],
            Curve::Parabola(f, a) => [
                f.to_world(V3(a * t * t, 2.0 * a * t, 0.0)),
                f.x * (2.0 * a * t) + f.y * (2.0 * a),
                f.x * (2.0 * a),
            ],
            Curve::Hyperbola(f, a, b) => {
                let (ch, sh) = (t.cosh(), t.sinh());
                [
                    f.to_world(V3(a * ch, b * sh, 0.0)),
                    f.x * (a * sh) + f.y * (b * ch),
                    f.x * (a * ch) + f.y * (b * sh),
                ]
            }
            Curve::Spline(s) => s.derivs(self.wrap(t)),
        }
    }

    pub fn eval(&self, t: f64) -> V3 {
        match self {
            Curve::Spline(s) => s.eval(self.wrap(t)),
            _ => self.derivs(t)[0],
        }
    }

    /// A closed B-spline's parameter brought into its range, so the curve runs on past its
    /// ends.
    fn wrap(&self, t: f64) -> f64 {
        match (self, self.period()) {
            (Curve::Spline(s), Some(p)) => {
                let (a, _) = s.knots.domain();
                a + (t - a).rem_euclid(p)
            }
            _ => t,
        }
    }

    /// The period of a closed curve's parameter: 2π for circles and ellipses, the parameter
    /// range for a B-spline that ends where it starts.
    pub fn period(&self) -> Option<f64> {
        match self {
            Curve::Circle(..) | Curve::Ellipse(..) => Some(TAU),
            Curve::Spline(s) if s.closed => {
                let (a, b) = s.knots.domain();
                Some(b - a)
            }
            _ => None,
        }
    }

    /// The parameter range of a bounded curve (a B-spline's), or one turn of a closed conic.
    pub fn range(&self) -> Option<(f64, f64)> {
        match self {
            Curve::Circle(..) | Curve::Ellipse(..) => Some((0.0, TAU)),
            Curve::Spline(s) => Some(s.knots.domain()),
            _ => None,
        }
    }

    /// The parameter of the point of the curve nearest `p` as seen through `metric` (which
    /// maps a point to the coordinates distances are taken in), searched over the curve's
    /// range, or for an unbounded curve near the parameter `seed` gives.
    pub fn closest(&self, p: V3, metric: &dyn Fn(V3) -> V3) -> f64 {
        let target = metric(p);
        let dist = |t: f64| (metric(self.eval(t)) - target).length();
        let wraps = self.period().is_some();
        let (lo, hi, samples) = match self.range() {
            Some((a, b)) => {
                let n = match self {
                    Curve::Spline(s) => (s.knots.breaks().len() * 8).clamp(32, 4000),
                    _ => 64,
                };
                (a, b, n)
            }
            None => {
                let t0 = self.seed(p);
                let d = self.derivs(t0)[1].length().max(1e-12);
                let reach = 4.0 * ((p - self.eval(t0)).length() / d) + 1e-6;
                (t0 - reach, t0 + reach, 64)
            }
        };
        let t = minimise(dist, lo, hi, samples, wraps);
        match self.range() {
            Some((a, b)) if wraps => a + (t - a).rem_euclid(b - a),
            _ => t,
        }
    }

    /// A first guess at the parameter of the point nearest `p`, for unbounded curves.
    fn seed(&self, p: V3) -> f64 {
        match self {
            Curve::Line { origin, dir } => (p - *origin).dot(*dir) / dir.dot(*dir).max(1e-300),
            Curve::Parabola(f, a) => f.to_local(p).1 / (2.0 * a),
            Curve::Hyperbola(f, _, b) => (f.to_local(p).1 / b).asinh(),
            _ => 0.0,
        }
    }
}

/// The `t` in `[lo, hi]` where `f` is least: the best of `samples` even steps, then a golden
/// section search between its neighbours (past the ends when `f` wraps around them).
pub(crate) fn minimise(
    f: impl Fn(f64) -> f64,
    lo: f64,
    hi: f64,
    samples: usize,
    wraps: bool,
) -> f64 {
    let step = (hi - lo) / samples as f64;
    let (mut best, mut best_f) = (lo, f(lo));
    for i in 1..=samples {
        let t = lo + step * i as f64;
        let ft = f(t);
        if ft < best_f {
            (best, best_f) = (t, ft);
        }
    }
    let (mut a, mut b) = if wraps {
        (best - step, best + step)
    } else {
        ((best - step).max(lo), (best + step).min(hi))
    };
    const G: f64 = 0.618_033_988_749_895;
    let mut c = b - G * (b - a);
    let mut d = a + G * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..80 {
        if (b - a).abs() <= 1e-15 * (1.0 + a.abs().max(b.abs())) {
            break;
        }
        if fc < fd {
            (b, d, fd) = (d, c, fc);
            c = b - G * (b - a);
            fc = f(c);
        } else {
            (a, c, fc) = (c, d, fd);
            d = a + G * (b - a);
            fd = f(d);
        }
    }
    let t = 0.5 * (a + b);
    if f(t) <= best_f { t } else { best }
}

fn curve(model: &StepModel, id: RecordId, depth: u32) -> Result<Curve, String> {
    let bad = |what: &str| format!("curve #{}: {what}", id.index() + 1);
    if depth > 8 {
        return Err(bad("curves nest too deep"));
    }
    let frame = |p| Frame::of(model, p).ok_or_else(|| bad("bad placement"));
    if let Some(l) = model.get::<Line>(id) {
        let origin = point(model, l.pnt).ok_or_else(|| bad("bad point"))?;
        let v = model
            .get::<Vector>(l.dir)
            .ok_or_else(|| bad("bad vector"))?;
        let d = direction(model, v.orientation).ok_or_else(|| bad("bad direction"))?;
        if !(v.magnitude > 0.0 && v.magnitude.is_finite()) {
            return Err(bad("a line whose vector has no length"));
        }
        return Ok(Curve::Line {
            origin,
            dir: d * v.magnitude,
        });
    }
    if let Some(c) = model.get::<Circle>(id) {
        if !(c.radius > 0.0 && c.radius.is_finite()) {
            return Err(bad("a circle whose radius is not positive"));
        }
        return Ok(Curve::Circle(frame(c.position)?, c.radius));
    }
    if let Some(e) = model.get::<Ellipse>(id) {
        if !(e.semi_axis_1 > 0.0 && e.semi_axis_2 > 0.0) {
            return Err(bad("an ellipse whose semi-axes are not positive"));
        }
        return Ok(Curve::Ellipse(
            frame(e.position)?,
            e.semi_axis_1,
            e.semi_axis_2,
        ));
    }
    if let Some(p) = model.get::<Parabola>(id) {
        if !(p.focal_dist > 0.0 && p.focal_dist.is_finite()) {
            return Err(bad("a parabola whose focal distance is not positive"));
        }
        return Ok(Curve::Parabola(frame(p.position)?, p.focal_dist));
    }
    if let Some(h) = model.get::<Hyperbola>(id) {
        if !(h.semi_axis > 0.0 && h.semi_imag_axis > 0.0) {
            return Err(bad("a hyperbola whose semi-axes are not positive"));
        }
        return Ok(Curve::Hyperbola(
            frame(h.position)?,
            h.semi_axis,
            h.semi_imag_axis,
        ));
    }
    if let Some(b) = model.get::<BSplineCurve>(id) {
        return Ok(Curve::Spline(Box::new(
            SplineCurve::of(model, b).map_err(|e| bad(&e))?,
        )));
    }
    if let Some(t) = model.get::<TrimmedCurve>(id) {
        return curve(model, t.basis_curve, depth + 1);
    }
    if let Some(s) = model.get::<SurfaceCurve>(id) {
        return curve(model, s.curve_3d, depth + 1);
    }
    Err(bad(&format!(
        "a {}, which is not meshed yet",
        model.record(id).entity_name()
    )))
}

/// The points of an edge from its start vertex to its end vertex. The first and last points
/// are the vertices' own, so edges that share a vertex meet exactly.
pub(crate) fn sample_edge(
    model: &StepModel,
    id: RecordId,
    options: &MeshOptions,
) -> Result<Vec<V3>, String> {
    let edge = model
        .get::<EdgeCurve>(id)
        .ok_or_else(|| format!("edge #{} is not an edge curve", id.index() + 1))?;
    let vertex = |v: RecordId| {
        model
            .get::<VertexPoint>(v)
            .and_then(|v| point(model, v.vertex_geometry))
            .ok_or_else(|| format!("vertex #{} has no point", v.index() + 1))
    };
    let start = vertex(edge.edge_start)?;
    let end = vertex(edge.edge_end)?;
    let closed = edge.edge_start == edge.edge_end;
    let forward = edge.same_sense;
    let mut points = match Curve::of(model, edge.edge_geometry)? {
        Curve::Line { .. } => vec![start, end],
        Curve::Circle(f, r) => {
            let at = |t: f64| f.to_world(V3(r * t.cos(), r * t.sin(), 0.0));
            let angle = |p: V3| {
                let l = f.to_local(p);
                l.1.atan2(l.0)
            };
            arc(
                angle(start),
                angle(end),
                closed,
                forward,
                arc_step(r, options),
                at,
            )
        }
        Curve::Ellipse(f, a, b) => {
            let at = |t: f64| f.to_world(V3(a * t.cos(), b * t.sin(), 0.0));
            let angle = |p: V3| {
                let l = f.to_local(p);
                (l.1 / b).atan2(l.0 / a)
            };
            // A step dt strays about |C' × C''| / |C'| · dt²/8 = ab / |C'| · dt²/8 from its
            // chord, most where |C'| is least, at the ends of the major axis: there the bend
            // radius is min²/max but the point moves only min·dt, so the step strays
            // max·dt²/8, as on a circle of the longer semi-axis. The tangent turns
            // ab / |C'|² per unit of t, at most max/min. Either semi-axis may be the longer.
            let (lo, hi) = (a.min(b), a.max(b));
            let step = curve_step(hi, hi / lo, options);
            arc(angle(start), angle(end), closed, forward, step, at)
        }
        Curve::Parabola(f, a) => {
            let at = |t: f64| f.to_world(V3(a * t * t, 2.0 * a * t, 0.0));
            let param = |p: V3| f.to_local(p).1 / (2.0 * a);
            // |C' × C''| / |C'| = 2a / √(1 + t²) and the tangent turns 1 / (1 + t²) per unit
            // of t: both most at the vertex, as on a circle of radius 2a.
            span(
                param(start),
                param(end),
                curve_step(2.0 * a, 1.0, options),
                at,
            )
        }
        Curve::Hyperbola(f, a, b) => {
            let at = |t: f64| f.to_world(V3(a * t.cosh(), b * t.sinh(), 0.0));
            let param = |p: V3| (f.to_local(p).1 / b).asinh();
            // |C' × C''| = ab and |C'| is least, b, at the vertex: the step strays as on a
            // circle of radius a, and the tangent turns at most a/b per unit of t.
            span(param(start), param(end), curve_step(a, a / b, options), at)
        }
        Curve::Spline(s) => spline_edge(&s, start, end, closed, forward, options)?,
    };
    let last = points.len() - 1;
    points[0] = start;
    points[last] = end;
    Ok(without_repeats(points))
}

/// How near, in mm, two points of an edge count as one.
const SAME_POINT: f64 = 1e-6;

/// The points with any that repeat the one before (an edge along a knot span of almost no
/// length) left out, keeping both ends.
fn without_repeats(points: Vec<V3>) -> Vec<V3> {
    let n = points.len();
    let mut out: Vec<V3> = Vec::with_capacity(n);
    for (i, p) in points.into_iter().enumerate() {
        if i + 1 == n {
            // The end vertex replaces an interior point it repeats.
            while out.len() > 1 && (p - out[out.len() - 1]).length() <= SAME_POINT {
                out.pop();
            }
            out.push(p);
        } else if out.last().is_none_or(|q| (p - *q).length() > SAME_POINT) {
            out.push(p);
        }
    }
    out
}

/// How far, relative to the curve's size, a vertex may lie off its edge's B-spline.
const OFF_CURVE: f64 = 1e-3;

/// A B-spline edge between its vertices' parameters, cut where the chord or the turn calls
/// for it.
fn spline_edge(
    s: &SplineCurve,
    start: V3,
    end: V3,
    closed: bool,
    forward: bool,
    options: &MeshOptions,
) -> Result<Vec<V3>, String> {
    let curve = Curve::Spline(Box::new(s.clone()));
    let (a, b) = s.knots.domain();
    let period = curve.period();
    let tol = OFF_CURVE * s.size().max(1e-3) + 10.0 * options.chord;
    let param = |p: V3| {
        let t = curve.closest(p, &|q| q);
        if (s.eval(t) - p).length() > tol {
            Err("a vertex that does not lie on its B-spline edge".to_string())
        } else {
            Ok(t)
        }
    };
    let (mut t0, mut t1) = (param(start)?, param(end)?);
    // On a closed curve a vertex at its ends may take either end's parameter.
    if period.is_some() {
        let snap = |t: f64, to_end: bool| {
            let near_end = (t - a).abs().min((t - b).abs()) <= 1e-9 * (b - a);
            match (near_end, to_end) {
                (true, true) => b,
                (true, false) => a,
                _ => t,
            }
        };
        if forward {
            t0 = snap(t0, false);
            t1 = snap(t1, true);
        } else {
            t0 = snap(t0, true);
            t1 = snap(t1, false);
        }
    }
    // The pieces of the curve's range the edge runs along, in its direction: through the
    // ends of a closed curve's range when it must.
    let (first, last) = if forward { (a, b) } else { (b, a) };
    let pieces: Vec<(f64, f64)> = match period {
        None if closed => vec![(first, last)],
        None => vec![(t0, t1)],
        Some(_) if closed && t0 == first => vec![(first, last)],
        Some(_) if closed => vec![(t0, last), (first, t0)],
        Some(_) if (t1 > t0) == forward && t1 != t0 => vec![(t0, t1)],
        Some(_) => vec![(t0, last), (first, t1)],
    };
    let mut points = vec![s.eval(pieces[0].0)];
    for (p0, p1) in pieces {
        sample_spline(s, p0, p1, options, &mut points);
    }
    if points.len() < 2 {
        points.push(s.eval(t1));
    }
    Ok(points)
}

/// Appends the points of `s` after `t0` up to `t1`: each knot span cut in at least `degree`
/// steps, and each step halved until its chord and turn are within the tolerances.
fn sample_spline(s: &SplineCurve, t0: f64, t1: f64, options: &MeshOptions, out: &mut Vec<V3>) {
    let (lo, hi) = (t0.min(t1), t0.max(t1));
    let mut cuts = vec![t0];
    let mut inner: Vec<f64> = s
        .knots
        .breaks()
        .into_iter()
        .filter(|&k| k > lo + 1e-9 * (hi - lo) && k < hi - 1e-9 * (hi - lo))
        .collect();
    if t1 < t0 {
        inner.reverse();
    }
    cuts.extend(inner);
    cuts.push(t1);
    let n = s.knots.degree.max(2);
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let mut prev = a;
        for i in 1..=n {
            let t = a + (b - a) * i as f64 / n as f64;
            refine(s, prev, t, options, out, 0);
            prev = t;
        }
    }
}

/// Appends the points after `a` up to `b`, halving until each piece is within tolerance.
fn refine(s: &SplineCurve, a: f64, b: f64, options: &MeshOptions, out: &mut Vec<V3>, depth: u32) {
    let [pa, da, _] = s.derivs(a);
    let [pb, db, _] = s.derivs(b);
    let m = 0.5 * (a + b);
    let pm = s.eval(m);
    let chord = pb - pa;
    let off = match chord.unit() {
        Some(d) => {
            let w = pm - pa;
            (w - d * w.dot(d)).length()
        }
        None => (pm - pa).length(),
    };
    let turn = match (da.unit(), db.unit()) {
        (Some(x), Some(y)) => x.dot(y).clamp(-1.0, 1.0).acos(),
        _ => 0.0,
    };
    if depth < 16 && (off > options.chord || turn > options.angle) {
        refine(s, a, m, options, out, depth + 1);
        refine(s, m, b, options, out, depth + 1);
    } else {
        out.push(pb);
    }
}

/// Samples `at(t)` from `t0` to `t1` on an open curve, in steps of at most `step`.
fn span(t0: f64, t1: f64, step: f64, at: impl Fn(f64) -> V3) -> Vec<V3> {
    let n = ((t1 - t0).abs() / step).ceil().clamp(1.0, 1e6) as usize;
    (0..=n)
        .map(|i| at(t0 + (t1 - t0) * i as f64 / n as f64))
        .collect()
}

/// Samples `at(t)` from `t0` to `t1`, counter-clockwise when `forward`, a whole turn when the
/// edge is closed.
fn arc(
    t0: f64,
    t1: f64,
    closed: bool,
    forward: bool,
    step: f64,
    at: impl Fn(f64) -> V3,
) -> Vec<V3> {
    let mut sweep = if forward { t1 - t0 } else { t0 - t1 }.rem_euclid(TAU);
    if closed || sweep < 1e-12 {
        sweep = TAU;
    }
    let sweep = if forward { sweep } else { -sweep };
    let n = (sweep.abs() / step).ceil().max(1.0) as usize;
    (0..=n)
        .map(|i| at(t0 + sweep * i as f64 / n as f64))
        .collect()
}
