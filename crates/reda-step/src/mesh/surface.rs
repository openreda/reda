//! The surfaces faces lie on: planes, cylinders, cones, spheres and tori in closed form, and
//! the freeform ones (B-spline surfaces, surfaces of revolution and of linear extrusion, see
//! [`super::freeform`]), each with its parameterisation (ISO 10303-42), the inverse that maps
//! a point back to `(u, v)`, and the parameter steps that keep a mesh within the chord
//! tolerance.
//!
//! Every parameterisation is set up so that `∂S/∂u × ∂S/∂v` points along the surface's own
//! normal: a triangle counter-clockwise in `(u, v)` faces the way the surface does.

use std::f64::consts::{PI, TAU};

use super::MeshOptions;
use super::bspline::SurfacePoint;
use super::freeform::{self, End, Extrusion, Revolution, Spline};
use super::geometry::{Frame, V3};
use crate::StepModel;
use crate::records::{
    ConicalSurface, CylindricalSurface, DegenerateToroidalSurface, Plane, RecordId,
    SphericalSurface, ToroidalSurface,
};

/// Which parameter means nothing at a singular point: `u` at a sphere's pole or a cone's apex
/// (where every `u` gives the point), `v` where a B-spline surface's row collapses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lost {
    U,
    V,
}

/// A box in parameter space: a face's `u` and `v` ranges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UvBox {
    pub u: (f64, f64),
    pub v: (f64, f64),
}

#[derive(Clone, Debug)]
pub(crate) enum Surface {
    /// `S(u, v) = o + u·x + v·y`.
    Plane(Frame),
    /// `S(u, v) = o + r·(cos u·x + sin u·y) + v·z`.
    Cylinder {
        frame: Frame,
        radius: f64,
    },
    /// `S(u, v) = o + (r + v·tan α)·(cos u·x + sin u·y) + v·z`.
    Cone {
        frame: Frame,
        radius: f64,
        tan: f64,
    },
    /// `S(u, v) = o + r·cos v·(cos u·x + sin u·y) + r·sin v·z`.
    Sphere {
        frame: Frame,
        radius: f64,
    },
    /// `S(u, v) = o + (R + r·cos v)·(cos u·x + sin u·y) + r·sin v·z`. A degenerate torus
    /// (`R ≤ r`) passes through its axis; `inner` picks the part inside the axis points
    /// (where `R + r·cos v < 0`) rather than the outer one.
    Torus {
        frame: Frame,
        major: f64,
        minor: f64,
        inner: bool,
    },
    Spline(Box<Spline>),
    Revolution(Box<Revolution>),
    Extrusion(Box<Extrusion>),
}

impl Surface {
    /// The surface record `id` names, or why it cannot be meshed.
    pub fn of(model: &StepModel, id: RecordId) -> Result<Surface, String> {
        let frame =
            |p| Frame::of(model, p).ok_or_else(|| format!("placement #{} cannot be read", p.0 + 1));
        let positive = |r: f64, what: &str| {
            if r > 0.0 && r.is_finite() {
                Ok(r)
            } else {
                Err(format!(
                    "surface #{}: its {what} is not positive",
                    id.index() + 1
                ))
            }
        };
        if let Some(s) = model.get::<Plane>(id) {
            return Ok(Surface::Plane(frame(s.position)?));
        }
        if let Some(s) = model.get::<CylindricalSurface>(id) {
            return Ok(Surface::Cylinder {
                frame: frame(s.position)?,
                radius: positive(s.radius, "radius")?,
            });
        }
        if let Some(s) = model.get::<ConicalSurface>(id) {
            let tan = s.semi_angle.tan();
            if !(tan.is_finite() && s.semi_angle.abs() < PI / 2.0 && s.radius >= 0.0) {
                return Err(format!(
                    "surface #{}: a cone with a bad angle",
                    id.index() + 1
                ));
            }
            return Ok(Surface::Cone {
                frame: frame(s.position)?,
                radius: s.radius,
                tan,
            });
        }
        if let Some(s) = model.get::<SphericalSurface>(id) {
            return Ok(Surface::Sphere {
                frame: frame(s.position)?,
                radius: positive(s.radius, "radius")?,
            });
        }
        if let Some(s) = model.get::<ToroidalSurface>(id) {
            let minor = positive(s.minor_radius, "minor radius")?;
            // A torus whose major radius is not above its minor one should have been written
            // as a degenerate torus; it is read as one, its outer part.
            if !(s.major_radius >= 0.0 && s.major_radius.is_finite()) {
                return Err(format!(
                    "surface #{}: a torus whose major radius is negative",
                    id.index() + 1
                ));
            }
            return Ok(Surface::Torus {
                frame: frame(s.position)?,
                major: s.major_radius,
                minor,
                inner: false,
            });
        }
        if let Some(s) = model.get::<DegenerateToroidalSurface>(id) {
            let minor = positive(s.minor_radius, "minor radius")?;
            if !(s.major_radius >= 0.0 && s.major_radius <= minor) {
                return Err(format!(
                    "surface #{}: a degenerate torus whose major radius is not within its minor one",
                    id.index() + 1
                ));
            }
            return Ok(Surface::Torus {
                frame: frame(s.position)?,
                major: s.major_radius,
                minor,
                inner: !s.select_outer,
            });
        }
        if let Some(s) = freeform::of(model, id) {
            return s;
        }
        Err(format!(
            "surface #{} is a {}, which is not meshed yet",
            id.index() + 1,
            model.record(id).entity_name()
        ))
    }

    /// The point and derivatives of a freeform surface.
    fn derivs(&self, u: f64, v: f64) -> Option<SurfacePoint> {
        match self {
            Surface::Spline(f) => Some(f.derivs(u, v)),
            Surface::Revolution(f) => Some(f.derivs(u, v)),
            Surface::Extrusion(f) => Some(f.derivs(u, v)),
            _ => None,
        }
    }

    pub fn eval(&self, u: f64, v: f64) -> V3 {
        if let Some(d) = self.derivs(u, v) {
            return d.p;
        }
        let (c, s) = (u.cos(), u.sin());
        match *self {
            Surface::Plane(f) => f.to_world(V3(u, v, 0.0)),
            Surface::Cylinder { frame, radius } => frame.to_world(V3(radius * c, radius * s, v)),
            Surface::Cone { frame, radius, tan } => {
                let rho = radius + v * tan;
                frame.to_world(V3(rho * c, rho * s, v))
            }
            Surface::Sphere { frame, radius } => {
                let rho = radius * v.cos();
                frame.to_world(V3(rho * c, rho * s, radius * v.sin()))
            }
            Surface::Torus {
                frame,
                major,
                minor,
                ..
            } => {
                let rho = major + minor * v.cos();
                frame.to_world(V3(rho * c, rho * s, minor * v.sin()))
            }
            Surface::Spline(_) | Surface::Revolution(_) | Surface::Extrusion(_) => {
                unreachable!("evaluated above")
            }
        }
    }

    /// The unit normal at `(u, v)`, along `∂S/∂u × ∂S/∂v`.
    pub fn normal(&self, u: f64, v: f64) -> V3 {
        if self.derivs(u, v).is_some() {
            return self.freeform_normal(u, v);
        }
        let (c, s) = (u.cos(), u.sin());
        let (frame, local) = match *self {
            Surface::Plane(f) => return f.z,
            Surface::Cylinder { frame, .. } => (frame, V3(c, s, 0.0)),
            Surface::Cone { frame, radius, tan } => {
                let rho = radius + v * tan;
                let sign = if rho < -1e-9 * radius.max(1.0) {
                    -1.0
                } else {
                    1.0
                };
                (frame, V3(c, s, -tan) * sign)
            }
            Surface::Sphere { frame, .. } => (frame, V3(v.cos() * c, v.cos() * s, v.sin())),
            Surface::Torus {
                frame,
                major,
                minor,
                ..
            } => {
                // ∂S/∂u × ∂S/∂v carries the factor R + r·cos v, which turns the normal over
                // on the inner part of a degenerate torus.
                let sign = if major + minor * v.cos() < -1e-9 * minor {
                    -1.0
                } else {
                    1.0
                };
                (frame, V3(v.cos() * c, v.cos() * s, v.sin()) * sign)
            }
            Surface::Spline(_) | Surface::Revolution(_) | Surface::Extrusion(_) => {
                unreachable!("handled above")
            }
        };
        let n = frame.x * local.0 + frame.y * local.1 + frame.z * local.2;
        n.unit().unwrap_or(frame.z)
    }

    /// The normal of a freeform surface: along `∂S/∂u × ∂S/∂v`, or where that vanishes (at a
    /// pole), from a point a little way off it.
    fn freeform_normal(&self, u: f64, v: f64) -> V3 {
        let d = |u: f64, v: f64| self.derivs(u, v).expect("a freeform surface");
        let s = d(u, v);
        if let Some(n) = s.u.cross(s.v).unit() {
            return n;
        }
        let h = 1e-6 * (1.0 + u.abs() + v.abs());
        for (du, dv) in [(0.0, h), (0.0, -h), (h, 0.0), (-h, 0.0), (h, h), (-h, -h)] {
            let s = d(u + du, v + dv);
            if let Some(n) = s.u.cross(s.v).unit() {
                return n;
            }
        }
        V3::Z
    }

    /// `(u, v)` of a point on (or near) the surface, `u` in `(-π, π]` on the analytic
    /// surfaces of revolution; `seed` is the `(u, v)` of a point near it where there is one,
    /// which the freeform surfaces search from. At a singular point one of them is
    /// meaningless (see [`Self::lost`]).
    pub fn project(&self, p: V3, seed: Option<(f64, f64)>) -> (f64, f64) {
        match *self {
            Surface::Spline(ref f) => f.project(p, seed),
            Surface::Revolution(ref f) => f.project(p),
            Surface::Extrusion(ref f) => f.project(p),
            Surface::Plane(f) => {
                let l = f.to_local(p);
                (l.0, l.1)
            }
            Surface::Cylinder { frame, .. } => {
                let l = frame.to_local(p);
                (l.1.atan2(l.0), l.2)
            }
            Surface::Cone { frame, radius, tan } => {
                let l = frame.to_local(p);
                let u = if radius + l.2 * tan < 0.0 {
                    (-l.1).atan2(-l.0)
                } else {
                    l.1.atan2(l.0)
                };
                (u, l.2)
            }
            Surface::Sphere { frame, .. } => {
                let l = frame.to_local(p);
                (l.1.atan2(l.0), l.2.atan2(l.0.hypot(l.1)))
            }
            Surface::Torus {
                frame,
                major,
                inner,
                ..
            } => {
                let l = frame.to_local(p);
                if inner {
                    ((-l.1).atan2(-l.0), l.2.atan2(-l.0.hypot(l.1) - major))
                } else {
                    (l.1.atan2(l.0), l.2.atan2(l.0.hypot(l.1) - major))
                }
            }
        }
    }

    /// The periods `u` (first) and `v` (second) wrap around with, if they do.
    pub fn periods(&self) -> (Option<f64>, Option<f64>) {
        match self {
            Surface::Plane(_) => (None, None),
            Surface::Cylinder { .. } | Surface::Cone { .. } | Surface::Sphere { .. } => {
                (Some(TAU), None)
            }
            Surface::Torus { .. } => (Some(TAU), Some(TAU)),
            Surface::Spline(f) => f.periods(),
            Surface::Revolution(f) => f.periods(),
            Surface::Extrusion(f) => f.periods(),
        }
    }

    /// The parameter that means nothing at `p`, if `p` is a singular point: a pole, an apex,
    /// a point on the axis of a surface of revolution or a collapsed B-spline boundary.
    pub fn lost(&self, p: V3) -> Option<Lost> {
        // Within the usual STEP confusion tolerance (1e-7 mm) of the axis.
        let on_axis = |frame: Frame, size: f64| {
            let l = frame.to_local(p);
            l.0.hypot(l.1) <= SINGULAR * size.max(1.0)
        };
        let lost_u = |b: bool| b.then_some(Lost::U);
        match *self {
            Surface::Sphere { frame, radius } => lost_u(on_axis(frame, radius)),
            Surface::Cone { frame, radius, .. } => lost_u(on_axis(frame, radius)),
            // A degenerate torus meets its axis.
            Surface::Torus {
                frame,
                major,
                minor,
                ..
            } if major <= minor => lost_u(on_axis(frame, minor)),
            Surface::Spline(ref f) => f.lost(p),
            Surface::Revolution(ref f) => f.lost(p),
            _ => None,
        }
    }

    /// Where the parameter range across direction `around` ends, low end first: for a face
    /// running around `u` (`around` 0), the ends of `v`. An end is a pole where the surface
    /// meets its axis, else an open edge; there is none where the range is unbounded or wraps.
    pub fn ends(&self, around: usize) -> (Option<End>, Option<End>) {
        let pole = |at: f64| Some(End { at, pole: true });
        match self {
            Surface::Sphere { .. } if around == 0 => (pole(-PI / 2.0), pole(PI / 2.0)),
            Surface::Cone { radius, tan, .. } if around == 0 && *tan != 0.0 => {
                let apex = -radius / tan;
                if *tan > 0.0 {
                    (pole(apex), None)
                } else {
                    (None, pole(apex))
                }
            }
            Surface::Spline(f) => f.ends(1 - around),
            Surface::Revolution(f) if around == 0 => f.ends(),
            _ => (None, None),
        }
    }

    /// Factors that turn `(u, v)` into roughly metric lengths, so the triangulation in
    /// parameter space gives well-shaped triangles on the surface. `b` is the face's box.
    pub fn scale(&self, options: &MeshOptions, b: UvBox) -> (f64, f64) {
        let v_range = b.v;
        match *self {
            Surface::Plane(_) => (1.0, 1.0),
            Surface::Cylinder { radius, .. } => (radius, 1.0),
            Surface::Cone { radius, tan, .. } => {
                let rho = (radius + 0.5 * (v_range.0 + v_range.1) * tan).abs();
                (rho.max(1e-6), 1.0)
            }
            Surface::Sphere { radius, .. } => (radius, radius),
            Surface::Torus { major, minor, .. } => (major.max(0.5 * minor), minor),
            Surface::Spline(_) | Surface::Revolution(_) | Surface::Extrusion(_) => {
                self.freeform_steps(options, b).1
            }
        }
    }

    /// The largest parameter steps in `u` and `v` that keep chords within the tolerances, or
    /// `None` along a direction in which the surface is straight. `b` is the face's box.
    pub fn steps(&self, options: &MeshOptions, b: UvBox) -> (Option<f64>, Option<f64>) {
        let v_range = b.v;
        let arc = |r: f64| Some(arc_step(r, options));
        match *self {
            Surface::Plane(_) => (None, None),
            Surface::Cylinder { radius, .. } => (arc(radius), None),
            Surface::Cone { radius, tan, .. } => {
                let rho = (radius + v_range.0 * tan)
                    .abs()
                    .max((radius + v_range.1 * tan).abs());
                (arc(rho), None)
            }
            Surface::Sphere { radius, .. } => (arc(radius), arc(radius)),
            Surface::Torus { major, minor, .. } => (arc(major + minor), arc(minor)),
            Surface::Spline(_) | Surface::Revolution(_) | Surface::Extrusion(_) => {
                self.freeform_steps(options, b).0
            }
        }
    }

    /// The steps and scale of a freeform surface over a box (see
    /// [`freeform::steps_and_scale`]).
    #[allow(clippy::type_complexity)]
    fn freeform_steps(
        &self,
        options: &MeshOptions,
        b: UvBox,
    ) -> ((Option<f64>, Option<f64>), (f64, f64)) {
        let samples = match self {
            Surface::Spline(f) => freeform::spline_samples(f, b),
            Surface::Revolution(f) => (12, freeform::curve_samples(f.curve(), b.v)),
            Surface::Extrusion(f) => (freeform::curve_samples(f.curve(), b.u), 2),
            _ => unreachable!("freeform surfaces only"),
        };
        freeform::steps_and_scale(
            |u, v| self.derivs(u, v).expect("a freeform surface"),
            b,
            options,
            samples,
        )
    }

    /// The whole surface, for a closed surface a face may cover without any edge: a sphere,
    /// a torus, or a freeform surface that wraps around in one direction and in the other
    /// wraps too or ends in poles.
    pub fn whole(&self) -> Option<Whole> {
        let range = |u: (f64, f64), v: (f64, f64)| UvBox { u, v };
        match self {
            Surface::Sphere { .. } => Some(Whole {
                range: range((0.0, TAU), (-PI / 2.0, PI / 2.0)),
                wrap: [true, false],
                pole: [[false; 2], [true; 2]],
            }),
            Surface::Torus { major, minor, .. } if major > minor => Some(Whole {
                range: range((0.0, TAU), (0.0, TAU)),
                wrap: [true, true],
                pole: [[false; 2]; 2],
            }),
            Surface::Spline(f) => {
                let (pu, pv) = f.periods();
                let d = f.domain();
                let poles = |k: usize| {
                    let (lo, hi) = f.ends(k);
                    [lo.is_some_and(|e| e.pole), hi.is_some_and(|e| e.pole)]
                };
                let pole = [poles(0), poles(1)];
                let wrap = [pu.is_some(), pv.is_some()];
                let closed = |k: usize| wrap[k] || pole[k] == [true, true];
                ((wrap[0] || wrap[1]) && closed(0) && closed(1)).then_some(Whole {
                    range: d,
                    wrap,
                    pole,
                })
            }
            Surface::Revolution(f) => {
                let (_, pv) = f.periods();
                let (lo, hi) = f.ends();
                let ends = [lo.is_some_and(|e| e.pole), hi.is_some_and(|e| e.pole)];
                if pv.is_some() {
                    let (a, b) = f.curve().range()?;
                    Some(Whole {
                        range: range((0.0, TAU), (a, b)),
                        wrap: [true, true],
                        pole: [[false; 2]; 2],
                    })
                } else if ends == [true, true] {
                    Some(Whole {
                        range: range((0.0, TAU), (lo?.at, hi?.at)),
                        wrap: [true, false],
                        pole: [[false; 2], [true; 2]],
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// All of a closed surface: its parameter range, which parameters wrap around, and which
/// ends of each range are poles (`pole[0]`: the ends of `u`, whose rows collapse).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Whole {
    pub range: UvBox,
    pub wrap: [bool; 2],
    pub pole: [[bool; 2]; 2],
}

/// How near its axis, relative to its size, a point of a surface of revolution counts as on it.
const SINGULAR: f64 = 1e-7;

/// The largest angle an arc of radius `r` may be cut into so that each chord stays within
/// the chord and angle tolerances.
pub(crate) fn arc_step(r: f64, options: &MeshOptions) -> f64 {
    curve_step(r, 1.0, options)
}

/// The largest parameter step along a curve whose chords stray from it at most as a
/// circle's of radius `r` do, and whose tangent turns at most `turn` radians per unit of
/// parameter, within the chord and angle tolerances.
pub(crate) fn curve_step(r: f64, turn: f64, options: &MeshOptions) -> f64 {
    let by_chord = if options.chord > 0.0 && options.chord < r {
        2.0 * (1.0 - options.chord / r).acos()
    } else {
        PI
    };
    by_chord.min(options.angle / turn.max(1e-9)).max(1e-3)
}
