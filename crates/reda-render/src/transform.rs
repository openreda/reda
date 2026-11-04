//! Placing local geometry (a Symbol, a text run) in Document coordinates.

use reda_model::geom::{Millideg, Nm, Point};

pub(crate) use reda_model::geom::rotate;

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
