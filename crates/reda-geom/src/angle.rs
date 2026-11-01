//! Angles in integer millidegrees. Any rotation is allowed.

use std::ops::{Add, Neg, Sub};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An angle in integer millidegrees, counter-clockwise. Serialises as a bare integer.
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
pub struct Millideg(pub i64);

impl Millideg {
    pub const ZERO: Millideg = Millideg(0);
    /// One full turn.
    pub const TURN: Millideg = Millideg(360_000);

    pub const fn from_deg(deg: i64) -> Millideg {
        Millideg(deg * 1_000)
    }

    /// The same direction expressed in `[0, 360°)`.
    pub const fn normalized(self) -> Millideg {
        Millideg(self.0.rem_euclid(Self::TURN.0))
    }
}

impl Add for Millideg {
    type Output = Millideg;
    fn add(self, rhs: Millideg) -> Millideg {
        Millideg(self.0 + rhs.0)
    }
}

impl Sub for Millideg {
    type Output = Millideg;
    fn sub(self, rhs: Millideg) -> Millideg {
        Millideg(self.0 - rhs.0)
    }
}

impl Neg for Millideg {
    type Output = Millideg;
    fn neg(self) -> Millideg {
        Millideg(-self.0)
    }
}
