//! Where an entity's coordinates land in the world: its object coordinate system (DXF
//! Reference, "Object Coordinate Systems (OCS)" and "Arbitrary Axis Algorithm"), and the
//! INSERTs it sits in. Everything ends in the world's XY plane, seen from above.

use crate::shape::Xy;

/// A 3D point or direction.
pub(crate) type V3 = [f64; 3];

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(v: V3) -> Option<V3> {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (length > 0.0 && length.is_finite()).then(|| [v[0] / length, v[1] / length, v[2] / length])
}

/// The unit normal of an extrusion direction (groups 210, 220, 230), or +Z when it has no
/// length.
pub(crate) fn normal(extrusion: V3) -> V3 {
    unit(extrusion).unwrap_or([0.0, 0.0, 1.0])
}

/// `normal × v`: the direction a quarter turn counter-clockwise from `v` in the plane
/// `normal` faces.
pub(crate) fn turned(normal: V3, v: V3) -> V3 {
    cross(normal, v)
}

/// A 3D affine map: `columns[i]` is where the unit vector along axis `i` goes, and
/// `origin` is where the origin goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Affine {
    pub columns: [V3; 3],
    pub origin: V3,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        columns: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        origin: [0.0, 0.0, 0.0],
    };

    /// The object coordinate system of an entity whose extrusion direction is `extrusion`:
    /// its X axis by the Arbitrary Axis Algorithm, Y the normal × X, Z the normal.
    pub fn ocs(extrusion: V3) -> Affine {
        let n = normal(extrusion);
        if n == [0.0, 0.0, 1.0] {
            return Affine::IDENTITY;
        }
        let limit = 1.0 / 64.0;
        let world = if n[0].abs() < limit && n[1].abs() < limit {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let ax = unit(cross(world, n)).expect("the normal is not along that world axis");
        let ay = unit(cross(n, ax)).expect("X is square to the normal");
        Affine {
            columns: [ax, ay, n],
            origin: [0.0; 3],
        }
    }

    /// Moves by `by`, then scales each axis, then turns `degrees` counter-clockwise about Z.
    pub fn placed(by: V3, scale: V3, degrees: f64) -> Affine {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Affine {
            columns: [
                [cos * scale[0], sin * scale[0], 0.0],
                [-sin * scale[1], cos * scale[1], 0.0],
                [0.0, 0.0, scale[2]],
            ],
            origin: [0.0; 3],
        }
        .then_after(Affine::translation(by))
    }

    pub fn translation(by: V3) -> Affine {
        Affine {
            origin: by,
            ..Affine::IDENTITY
        }
    }

    /// `self` after `inner`: `inner` maps first.
    pub fn of(self, inner: Affine) -> Affine {
        Affine {
            columns: inner.columns.map(|c| self.vector(c)),
            origin: self.point(inner.origin),
        }
    }

    /// `outer` after `self`.
    fn then_after(self, outer: Affine) -> Affine {
        outer.of(self)
    }

    pub fn vector(&self, v: V3) -> V3 {
        let c = &self.columns;
        [0, 1, 2].map(|i| c[0][i] * v[0] + c[1][i] * v[1] + c[2][i] * v[2])
    }

    pub fn point(&self, p: V3) -> V3 {
        let v = self.vector(p);
        [0, 1, 2].map(|i| v[i] + self.origin[i])
    }

    /// Where `p` lands, seen from above.
    pub fn xy(&self, p: V3) -> Xy {
        let [x, y, _] = self.point(p);
        Xy::new(x, y)
    }

    /// Where direction `v` points, seen from above.
    pub fn dxy(&self, v: V3) -> Xy {
        let [x, y, _] = self.vector(v);
        Xy::new(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flipped_extrusion_mirrors_x() {
        let ocs = Affine::ocs([0.0, 0.0, -1.0]);
        assert_eq!(ocs.xy([2.0, 3.0, 0.0]), Xy::new(-2.0, 3.0));
    }

    #[test]
    fn the_arbitrary_axis_algorithm_picks_world_z_for_a_normal_far_from_it() {
        // Normal +X: Ax = Wz × N = (0, 1, 0), Ay = N × Ax = (0, 0, 1).
        let ocs = Affine::ocs([5.0, 0.0, 0.0]);
        assert_eq!(
            ocs.columns,
            [[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]]
        );
    }

    #[test]
    fn placing_scales_then_turns_then_moves() {
        let a = Affine::placed([10.0, 0.0, 0.0], [2.0, 3.0, 1.0], 90.0);
        let p = a.xy([1.0, 1.0, 0.0]);
        assert!(
            (p.x - 7.0).abs() < 1e-12 && (p.y - 2.0).abs() < 1e-12,
            "{p:?}"
        );
    }
}
