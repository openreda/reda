//! Edges as point runs: each `EDGE_CURVE` sampled once, from its start vertex to its end
//! vertex, within the chord tolerance. Every face that uses an edge takes the same points, so
//! neighbouring faces meet without gaps.

use std::f64::consts::TAU;

use super::MeshOptions;
use super::geometry::{Frame, V3, point};
use super::surface::{arc_step, curve_step};
use crate::StepModel;
use crate::records::{
    Circle, EdgeCurve, Ellipse, Hyperbola, Line, Parabola, RecordId, SurfaceCurve, TrimmedCurve,
    VertexPoint,
};

/// The curves edges are sampled along.
enum Curve {
    Line,
    /// `C(t) = o + r·(cos t·x + sin t·y)`.
    Circle(Frame, f64),
    /// `C(t) = o + a·cos t·x + b·sin t·y`.
    Ellipse(Frame, f64, f64),
    /// `C(t) = o + a·(t²·x + 2t·y)`.
    Parabola(Frame, f64),
    /// `C(t) = o + a·cosh t·x + b·sinh t·y`.
    Hyperbola(Frame, f64, f64),
}

fn curve(model: &StepModel, id: RecordId, depth: u32) -> Result<Curve, String> {
    let bad = |what: &str| format!("curve #{}: {what}", id.index() + 1);
    if depth > 8 {
        return Err(bad("curves nest too deep"));
    }
    let frame = |p| Frame::of(model, p).ok_or_else(|| bad("bad placement"));
    if model.get::<Line>(id).is_some() {
        return Ok(Curve::Line);
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
    let mut points = match curve(model, edge.edge_geometry, 0)? {
        Curve::Line => vec![start, end],
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
    };
    let last = points.len() - 1;
    points[0] = start;
    points[last] = end;
    Ok(points)
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
