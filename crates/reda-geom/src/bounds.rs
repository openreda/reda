//! Axis-aligned boxes in nanometres.

use crate::{Nm, Point};

/// An axis-aligned box, its edges included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min: Point,
    pub max: Point,
}

impl Bounds {
    pub fn of_point(p: Point) -> Bounds {
        Bounds { min: p, max: p }
    }

    /// The square of half-width `r` about `p`.
    pub fn around(p: Point, r: Nm) -> Bounds {
        Bounds {
            min: Point::new(p.x - r, p.y - r),
            max: Point::new(p.x + r, p.y + r),
        }
    }

    pub fn union(self, other: Bounds) -> Bounds {
        Bounds {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// The smallest box holding every point; `None` when there are none.
    pub fn of_points(points: impl IntoIterator<Item = Point>) -> Option<Bounds> {
        points
            .into_iter()
            .map(Bounds::of_point)
            .reduce(Bounds::union)
    }

    /// The same box moved by `by`.
    pub fn moved(self, by: Point) -> Bounds {
        Bounds {
            min: self.min + by,
            max: self.max + by,
        }
    }

    /// The centre, rounded towards negative infinity.
    pub fn center(self) -> Point {
        Point::new(
            Nm((self.min.x.0 + self.max.x.0).div_euclid(2)),
            Nm((self.min.y.0 + self.max.y.0).div_euclid(2)),
        )
    }

    pub fn width(self) -> Nm {
        self.max.x - self.min.x
    }

    pub fn height(self) -> Nm {
        self.max.y - self.min.y
    }
}
