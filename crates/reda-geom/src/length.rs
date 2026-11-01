//! Lengths in integer nanometres, and points built from them.

use std::ops::{Add, Mul, Neg, Sub};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A length or coordinate in integer nanometres. Serialises as a bare integer.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
#[schemars(inline)]
pub struct Nm(pub i64);

impl Nm {
    pub const ZERO: Nm = Nm(0);

    pub const fn from_um(um: i64) -> Nm {
        Nm(um * 1_000)
    }

    pub const fn from_mm(mm: i64) -> Nm {
        Nm(mm * 1_000_000)
    }

    /// Thousandths of an inch: 1 mil is exactly 25,400 nm.
    pub const fn from_mil(mil: i64) -> Nm {
        Nm(mil * 25_400)
    }

    pub const fn from_inch(inch: i64) -> Nm {
        Nm(inch * 25_400_000)
    }
}

impl Add for Nm {
    type Output = Nm;
    fn add(self, rhs: Nm) -> Nm {
        Nm(self.0 + rhs.0)
    }
}

impl Sub for Nm {
    type Output = Nm;
    fn sub(self, rhs: Nm) -> Nm {
        Nm(self.0 - rhs.0)
    }
}

impl Neg for Nm {
    type Output = Nm;
    fn neg(self) -> Nm {
        Nm(-self.0)
    }
}

impl Mul<i64> for Nm {
    type Output = Nm;
    fn mul(self, rhs: i64) -> Nm {
        Nm(self.0 * rhs)
    }
}

/// A position on a Schematic Sheet or the PCB Document, which share one coordinate type.
// Points order by x, then y (sorted Junction lists rely on it).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: Nm,
    pub y: Nm,
}

impl Point {
    pub const fn new(x: Nm, y: Nm) -> Point {
        Point { x, y }
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}
