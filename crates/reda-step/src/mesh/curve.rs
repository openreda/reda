//! Edges as point runs: each `EDGE_CURVE` sampled once, from its start vertex to its end
//! vertex, within the chord tolerance. Every face that uses an edge takes the same points, so
//! neighbouring faces meet without gaps.

use std::f64::consts::TAU;

use super::MeshOptions;
use super::geometry::{Frame, V3, point};
use super::surface::arc_step;
use crate::StepModel;
use crate::records::{
    Circle, EdgeCurve, Ellipse, Line, RecordId, SurfaceCurve, TrimmedCurve, VertexPoint,
};

/// The curves edges are sampled along.
enum Curve {
    Line,
    /// `C(t) = o + r·(cos t·x + sin t·y)`.
    Circle(Frame, f64),
    /// `C(t) = o + a·cos t·x + b·sin t·y`.
    Ellipse(Frame, f64, f64),
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
            // The sharpest bend of an ellipse has radius b²/a; its largest is a²/b.
            let step = arc_step(b * b / a, options).min(arc_step(a.max(b), options));
            arc(angle(start), angle(end), closed, forward, step, at)
        }
    };
    let last = points.len() - 1;
    points[0] = start;
    points[last] = end;
    Ok(points)
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
