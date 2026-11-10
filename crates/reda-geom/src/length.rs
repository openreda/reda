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

    /// The length in millimetres, exactly, without trailing zeros: `12.7`, `-0.000001`,
    /// `0`.
    pub fn mm_exact(self) -> String {
        exact_decimal(self.0, 1_000_000)
    }
}

/// `value / unit` as an exact decimal without trailing zeros, for a `unit` that is a power
/// of ten: `exact_decimal(87_500, 1_000)` is `87.5`, `exact_decimal(-5, 10)` is `-0.5`.
pub fn exact_decimal(value: i64, unit: u64) -> String {
    debug_assert!(
        unit > 0 && 10u64.pow(unit.ilog10()) == unit,
        "{unit} is not a power of ten"
    );
    let sign = if value < 0 { "-" } else { "" };
    let a = value.unsigned_abs();
    let (whole, fraction) = (a / unit, a % unit);
    if fraction == 0 {
        return format!("{sign}{whole}");
    }
    let width = unit.ilog10() as usize;
    let fraction = format!("{fraction:0width$}");
    format!("{sign}{whole}.{}", fraction.trim_end_matches('0'))
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

    /// This point turned counter-clockwise by `angle` about `about` ([`crate::rotate`]).
    pub fn rotated_about(self, about: Point, angle: crate::Millideg) -> Point {
        let (x, y) = crate::rotate((self.x - about.x).0, (self.y - about.y).0, angle);
        Point::new(about.x + Nm(x), about.y + Nm(y))
    }

    /// Whether this point lies exactly on the segment from `a` to `b`, ends included, at
    /// any angle. Exact in integers: a point a nanometre off the line is not on it.
    pub fn on_segment(self, a: Point, b: Point) -> bool {
        let (ax, ay) = (i128::from(a.x.0), i128::from(a.y.0));
        let (bx, by) = (i128::from(b.x.0), i128::from(b.y.0));
        let (px, py) = (i128::from(self.x.0), i128::from(self.y.0));
        (bx - ax) * (py - ay) == (by - ay) * (px - ax)
            && ax.min(bx) <= px
            && px <= ax.max(bx)
            && ay.min(by) <= py
            && py <= ay.max(by)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_decimals_drop_trailing_zeros_and_keep_every_digit() {
        assert_eq!(Nm(12_700_000).mm_exact(), "12.7");
        assert_eq!(Nm(-1).mm_exact(), "-0.000001");
        assert_eq!(Nm(0).mm_exact(), "0");
        assert_eq!(Nm(-3_000_000).mm_exact(), "-3");
        assert_eq!(Nm(1_234_567_891).mm_exact(), "1234.567891");
        assert_eq!(exact_decimal(87_500, 1_000), "87.5");
        assert_eq!(exact_decimal(35_000, 1_000), "35");
        assert_eq!(exact_decimal(-5, 10), "-0.5");
        assert_eq!(exact_decimal(7, 1), "7");
        assert_eq!(exact_decimal(i64::MIN, 1_000_000), "-9223372036854.775808");
        assert_eq!(
            exact_decimal(2_500_000_000_000, 1_000_000_000_000),
            "2.5"
        );
    }
}
