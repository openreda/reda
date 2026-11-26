//! Small 3D maths for the mesher: vectors, frames and rigid transforms.

use std::ops::{Add, Mul, Neg, Sub};

use crate::StepModel;
use crate::records::{Axis2Placement3d, CartesianPoint, Direction};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct V3(pub f64, pub f64, pub f64);

impl V3 {
    pub const X: V3 = V3(1.0, 0.0, 0.0);
    pub const Y: V3 = V3(0.0, 1.0, 0.0);
    pub const Z: V3 = V3(0.0, 0.0, 1.0);

    pub fn from_slice(c: &[f64]) -> V3 {
        let at = |i: usize| c.get(i).copied().unwrap_or(0.0);
        V3(at(0), at(1), at(2))
    }

    pub fn dot(self, o: V3) -> f64 {
        self.0 * o.0 + self.1 * o.1 + self.2 * o.2
    }

    pub fn cross(self, o: V3) -> V3 {
        V3(
            self.1 * o.2 - self.2 * o.1,
            self.2 * o.0 - self.0 * o.2,
            self.0 * o.1 - self.1 * o.0,
        )
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    /// The unit vector, or `None` for a zero (or non-finite) one.
    pub fn unit(self) -> Option<V3> {
        let l = self.length();
        (l > 1e-300 && l.is_finite()).then(|| self * (1.0 / l))
    }

    pub fn array(self) -> [f64; 3] {
        [self.0, self.1, self.2]
    }

    pub fn is_finite(self) -> bool {
        self.0.is_finite() && self.1.is_finite() && self.2.is_finite()
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        V3(self.0 + o.0, self.1 + o.1, self.2 + o.2)
    }
}

impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        V3(self.0 - o.0, self.1 - o.1, self.2 - o.2)
    }
}

impl Mul<f64> for V3 {
    type Output = V3;
    fn mul(self, s: f64) -> V3 {
        V3(self.0 * s, self.1 * s, self.2 * s)
    }
}

impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        V3(-self.0, -self.1, -self.2)
    }
}

/// A right-handed orthonormal frame: an origin and its x, y and z axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub origin: V3,
    pub x: V3,
    pub y: V3,
    pub z: V3,
}

impl Frame {
    /// The frame of an `AXIS2_PLACEMENT_3D` (ISO 10303-42): z is the axis (default +z), x is
    /// the reference direction made perpendicular to it (default +x, or another axis when +x
    /// is parallel to z), y completes it.
    pub fn of(model: &StepModel, id: crate::RecordId) -> Option<Frame> {
        let p = model.get::<Axis2Placement3d>(id)?;
        let origin = point(model, p.location)?;
        let z = match p.axis {
            Some(a) => direction(model, a)?,
            None => V3::Z,
        };
        let r = match p.ref_direction {
            Some(r) => direction(model, r)?,
            None => V3::X,
        };
        Frame::from_axes(origin, z, r)
    }

    pub fn from_axes(origin: V3, z: V3, reference: V3) -> Option<Frame> {
        let z = z.unit()?;
        let mut x = (reference - z * reference.dot(z)).unit();
        if x.is_none() {
            let other = if z.0.abs() < 0.9 { V3::X } else { V3::Y };
            x = (other - z * other.dot(z)).unit();
        }
        let x = x?;
        Some(Frame {
            origin,
            x,
            y: z.cross(x),
            z,
        })
    }

    /// A point given in this frame, in the frame's parent coordinates.
    pub fn to_world(self, local: V3) -> V3 {
        self.origin + self.x * local.0 + self.y * local.1 + self.z * local.2
    }

    /// A point in the frame's coordinates.
    pub fn to_local(self, p: V3) -> V3 {
        let d = p - self.origin;
        V3(d.dot(self.x), d.dot(self.y), d.dot(self.z))
    }
}

pub(crate) fn point(model: &StepModel, id: crate::RecordId) -> Option<V3> {
    let p = model.get::<CartesianPoint>(id)?;
    let v = V3::from_slice(&p.coordinates);
    v.is_finite().then_some(v)
}

pub(crate) fn direction(model: &StepModel, id: crate::RecordId) -> Option<V3> {
    V3::from_slice(&model.get::<Direction>(id)?.ratios).unit()
}

/// A rigid placement: `p ↦ rotation · p + translation`, rows of `rotation` first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub rotation: [[f64; 3]; 3],
    pub translation: [f64; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0, 0.0, 0.0],
    };

    /// The transform taking a frame's local coordinates to its parent's.
    pub(crate) fn of_frame(f: &Frame) -> Transform {
        Transform {
            rotation: [
                [f.x.0, f.y.0, f.z.0],
                [f.x.1, f.y.1, f.z.1],
                [f.x.2, f.y.2, f.z.2],
            ],
            translation: f.origin.array(),
        }
    }

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let r = &self.rotation;
        let t = &self.translation;
        [
            r[0][0] * p[0] + r[0][1] * p[1] + r[0][2] * p[2] + t[0],
            r[1][0] * p[0] + r[1][1] * p[1] + r[1][2] * p[2] + t[1],
            r[2][0] * p[0] + r[2][1] * p[1] + r[2][2] * p[2] + t[2],
        ]
    }

    /// A direction turned by the rotation (no translation).
    pub fn apply_vector(&self, v: [f64; 3]) -> [f64; 3] {
        let r = &self.rotation;
        [
            r[0][0] * v[0] + r[0][1] * v[1] + r[0][2] * v[2],
            r[1][0] * v[0] + r[1][1] * v[1] + r[1][2] * v[2],
            r[2][0] * v[0] + r[2][1] * v[1] + r[2][2] * v[2],
        ]
    }

    /// `self` after `inner`: `p ↦ self(inner(p))`.
    pub fn compose(&self, inner: &Transform) -> Transform {
        let a = &self.rotation;
        let b = &inner.rotation;
        let mut rotation = [[0.0; 3]; 3];
        for (i, row) in rotation.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
            }
        }
        Transform {
            rotation,
            translation: self.apply(inner.translation),
        }
    }

    /// The inverse of a rigid transform.
    pub fn inverse(&self) -> Transform {
        let r = &self.rotation;
        let rotation = [
            [r[0][0], r[1][0], r[2][0]],
            [r[0][1], r[1][1], r[2][1]],
            [r[0][2], r[1][2], r[2][2]],
        ];
        let t = self.translation;
        let back = Transform {
            rotation,
            translation: [0.0; 3],
        }
        .apply(t);
        Transform {
            rotation,
            translation: [-back[0], -back[1], -back[2]],
        }
    }
}

/// An axis-aligned box in mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl BoundingBox {
    pub(crate) fn of_points<'a>(points: impl IntoIterator<Item = &'a [f64; 3]>) -> Option<Self> {
        let mut b: Option<BoundingBox> = None;
        for p in points {
            b = Some(match b {
                None => BoundingBox { min: *p, max: *p },
                Some(b) => b.with(*p),
            });
        }
        b
    }

    pub(crate) fn with(mut self, p: [f64; 3]) -> Self {
        for (i, x) in p.into_iter().enumerate() {
            self.min[i] = self.min[i].min(x);
            self.max[i] = self.max[i].max(x);
        }
        self
    }

    pub fn size(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}
