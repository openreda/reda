//! The analytic surfaces faces lie on: planes, cylinders, cones, spheres and tori, each with
//! its parameterisation (ISO 10303-42), the inverse that maps a point back to `(u, v)`, and
//! the parameter steps that keep a mesh within the chord tolerance.
//!
//! Every parameterisation is set up so that `∂S/∂u × ∂S/∂v` points along the surface's own
//! normal: a triangle counter-clockwise in `(u, v)` faces the way the surface does.

use std::f64::consts::{PI, TAU};

use super::MeshOptions;
use super::geometry::{Frame, V3};
use crate::StepModel;
use crate::records::{
    ConicalSurface, CylindricalSurface, DegenerateToroidalSurface, Plane, RecordId,
    SphericalSurface, ToroidalSurface,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Surface {
    /// `S(u, v) = o + u·x + v·y`.
    Plane(Frame),
    /// `S(u, v) = o + r·(cos u·x + sin u·y) + v·z`.
    Cylinder { frame: Frame, radius: f64 },
    /// `S(u, v) = o + (r + v·tan α)·(cos u·x + sin u·y) + v·z`.
    Cone { frame: Frame, radius: f64, tan: f64 },
    /// `S(u, v) = o + r·cos v·(cos u·x + sin u·y) + r·sin v·z`.
    Sphere { frame: Frame, radius: f64 },
    /// `S(u, v) = o + (R + r·cos v)·(cos u·x + sin u·y) + r·sin v·z`. A degenerate torus
    /// (`R ≤ r`) passes through its axis; `inner` picks the part inside the axis points
    /// (where `R + r·cos v < 0`) rather than the outer one.
    Torus {
        frame: Frame,
        major: f64,
        minor: f64,
        inner: bool,
    },
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
            if s.major_radius <= minor || s.major_radius.is_nan() {
                return Err(format!(
                    "surface #{}: a torus whose major radius is not above its minor one",
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
        Err(format!(
            "surface #{} is a {}, which is not meshed yet",
            id.index() + 1,
            model.record(id).entity_name()
        ))
    }

    pub fn eval(&self, u: f64, v: f64) -> V3 {
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
        }
    }

    /// The unit normal at `(u, v)`, along `∂S/∂u × ∂S/∂v`.
    pub fn normal(&self, u: f64, v: f64) -> V3 {
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
        };
        let n = frame.x * local.0 + frame.y * local.1 + frame.z * local.2;
        n.unit().unwrap_or(frame.z)
    }

    /// `(u, v)` of a point on (or near) the surface, `u` in `(-π, π]` on the surfaces of
    /// revolution. At a pole or a cone's apex `u` is meaningless (see [`Self::is_singular`]).
    pub fn project(&self, p: V3) -> (f64, f64) {
        match *self {
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

    /// Whether `u` (first) and `v` (second) wrap around every 2π.
    pub fn periodic(&self) -> (bool, bool) {
        match self {
            Surface::Plane(_) => (false, false),
            Surface::Cylinder { .. } | Surface::Cone { .. } | Surface::Sphere { .. } => {
                (true, false)
            }
            Surface::Torus { .. } => (true, true),
        }
    }

    /// Whether `p` is a point where `u` is undefined: a sphere's pole or a cone's apex.
    pub fn is_singular(&self, p: V3) -> bool {
        // Within the usual STEP confusion tolerance (1e-7 mm) of the axis.
        let on_axis = |frame: Frame, size: f64| {
            let l = frame.to_local(p);
            l.0.hypot(l.1) <= SINGULAR * size.max(1.0)
        };
        match *self {
            Surface::Sphere { frame, radius } => on_axis(frame, radius),
            Surface::Cone { frame, radius, .. } => on_axis(frame, radius),
            // A degenerate torus meets its axis.
            Surface::Torus {
                frame,
                major,
                minor,
                ..
            } if major <= minor => on_axis(frame, minor),
            _ => false,
        }
    }

    /// Factors that turn `(u, v)` into roughly metric lengths, so the triangulation in
    /// parameter space gives well-shaped triangles on the surface. `v_range` is the face's.
    pub fn scale(&self, v_range: (f64, f64)) -> (f64, f64) {
        match *self {
            Surface::Plane(_) => (1.0, 1.0),
            Surface::Cylinder { radius, .. } => (radius, 1.0),
            Surface::Cone { radius, tan, .. } => {
                let rho = (radius + 0.5 * (v_range.0 + v_range.1) * tan).abs();
                (rho.max(1e-6), 1.0)
            }
            Surface::Sphere { radius, .. } => (radius, radius),
            Surface::Torus { major, minor, .. } => (major.max(0.5 * minor), minor),
        }
    }

    /// The largest parameter steps in `u` and `v` that keep chords within the tolerances, or
    /// `None` along a direction in which the surface is straight. `v_range` is the face's.
    pub fn steps(&self, options: &MeshOptions, v_range: (f64, f64)) -> (Option<f64>, Option<f64>) {
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
        }
    }

    /// The whole surface's parameter range, for a closed surface a face may cover without
    /// any edge: a sphere or a torus.
    pub fn whole(&self) -> Option<((f64, f64), (f64, f64))> {
        match self {
            Surface::Sphere { .. } => Some(((0.0, TAU), (-PI / 2.0, PI / 2.0))),
            Surface::Torus { major, minor, .. } if major > minor => Some(((0.0, TAU), (0.0, TAU))),
            _ => None,
        }
    }
}

/// How near its axis, relative to its size, a point of a surface of revolution counts as on it.
const SINGULAR: f64 = 1e-7;

/// The largest angle an arc of radius `r` may be cut into so that each chord stays within
/// the chord and angle tolerances.
pub(crate) fn arc_step(r: f64, options: &MeshOptions) -> f64 {
    let by_chord = if options.chord > 0.0 && options.chord < r {
        2.0 * (1.0 - options.chord / r).acos()
    } else {
        PI
    };
    by_chord.min(options.angle).max(1e-3)
}
