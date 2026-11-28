//! The length units a drawing can be read in, and `$INSUNITS` (DXF Reference, "HEADER
//! Section Group Codes": the drawing units for the `$INSUNITS` variable).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A drawing unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Nm,
    Um,
    Mm,
    Cm,
    Dm,
    M,
    Microinch,
    Mil,
    Inch,
    Foot,
    Yard,
}

impl Unit {
    /// Nanometres in one unit.
    pub fn nm(self) -> f64 {
        match self {
            Unit::Nm => 1.0,
            Unit::Um => 1e3,
            Unit::Mm => 1e6,
            Unit::Cm => 1e7,
            Unit::Dm => 1e8,
            Unit::M => 1e9,
            Unit::Microinch => 25.4,
            Unit::Mil => 25_400.0,
            Unit::Inch => 25_400_000.0,
            Unit::Foot => 304_800_000.0,
            Unit::Yard => 914_400_000.0,
        }
    }

    /// The unit an `$INSUNITS` value names; `None` for 0 (unitless) and for the units no
    /// board drawing is made in, such as kilometres or light years.
    pub fn from_insunits(code: i64) -> Option<Unit> {
        Some(match code {
            1 => Unit::Inch,
            2 => Unit::Foot,
            4 => Unit::Mm,
            5 => Unit::Cm,
            6 => Unit::M,
            8 => Unit::Microinch,
            9 => Unit::Mil,
            10 => Unit::Yard,
            12 => Unit::Nm,
            13 => Unit::Um,
            14 => Unit::Dm,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insunits_codes_follow_the_dxf_reference() {
        assert_eq!(Unit::from_insunits(1), Some(Unit::Inch));
        assert_eq!(Unit::from_insunits(4), Some(Unit::Mm));
        assert_eq!(Unit::from_insunits(9), Some(Unit::Mil));
        assert_eq!(Unit::from_insunits(13), Some(Unit::Um));
        assert_eq!(Unit::from_insunits(0), None);
        assert_eq!(Unit::from_insunits(7), None);
        assert_eq!(Unit::Inch.nm(), 1000.0 * Unit::Mil.nm());
        assert_eq!(Unit::Foot.nm(), 12.0 * Unit::Inch.nm());
    }
}
