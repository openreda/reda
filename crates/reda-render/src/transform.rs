//! Placing local geometry (a Symbol, a text run) in Document coordinates.

use reda_model::geom::{Millideg, Nm, Point};

/// Rotates (`x`, `y`) by `angle` counter-clockwise about the origin. Exact on quarter
/// turns; other angles round to the nearest nm.
pub(crate) fn rotate(x: i64, y: i64, angle: Millideg) -> (i64, i64) {
    match angle.normalized().0 {
        0 => (x, y),
        90_000 => (-y, x),
        180_000 => (-x, -y),
        270_000 => (y, -x),
        a => {
            let (sin, cos) = (a as f64 / 1000.0).to_radians().sin_cos();
            let (fx, fy) = (x as f64, y as f64);
            (
                (fx * cos - fy * sin).round() as i64,
                (fx * sin + fy * cos).round() as i64,
            )
        }
    }
}

/// Mirror in x, then rotate, then move: how a Unit places its Symbol, the same order
/// `reda-schematic` uses for Pin positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Placement {
    pub at: Point,
    pub rotation: Millideg,
    pub mirror: bool,
}

impl Placement {
    pub(crate) fn apply(&self, p: Point) -> Point {
        let x = if self.mirror { -p.x.0 } else { p.x.0 };
        let (x, y) = rotate(x, p.y.0, self.rotation);
        Point::new(self.at.x + Nm(x), self.at.y + Nm(y))
    }

    /// A direction (an angle in the Symbol) as placed.
    pub(crate) fn angle(&self, a: Millideg) -> Millideg {
        let a = if self.mirror {
            Millideg(180_000) - a
        } else {
            a
        };
        (a + self.rotation).normalized()
    }
}
