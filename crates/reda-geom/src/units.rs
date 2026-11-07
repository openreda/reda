//! Unit strings: `"1.27mm"`, `"10mil"`, `"0.5in"`, `"45deg"`, and copper in `"1oz"`.
//!
//! Parsing is exact: the decimal is read as an integer mantissa and a power of ten, then
//! scaled by an integer factor, so `"1.27mm"` is exactly 1,270,000 nm. A value that does not
//! land on a whole nanometre or millidegree is refused rather than rounded, because stored
//! data is never rounded.

use std::fmt;

use crate::{Millideg, Nm};

/// Why a unit string was refused. The message is written for the person or agent who typed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitError(String);

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UnitError {}

/// Length units and how many nanometres one of each is. A bare number means nanometres.
const LENGTH_UNITS: &[(&str, i128)] = &[
    ("", 1),
    ("nm", 1),
    ("um", 1_000),
    ("µm", 1_000),
    ("μm", 1_000),
    ("mm", 1_000_000),
    ("mil", 25_400),
    ("in", 25_400_000),
];

/// Copper thickness units: the length units and the copper weight in ounces, where 1 oz is
/// exactly 35 µm.
const COPPER_UNITS: &[(&str, i128)] = &[
    ("", 1),
    ("nm", 1),
    ("um", 1_000),
    ("µm", 1_000),
    ("μm", 1_000),
    ("mm", 1_000_000),
    ("mil", 25_400),
    ("in", 25_400_000),
    ("oz", NM_PER_OZ as i128),
];

/// The copper thickness of one ounce of copper weight: exactly 35 µm.
pub const NM_PER_OZ: i64 = 35_000;

/// Angle units and how many millidegrees one of each is. A bare number means millidegrees.
const ANGLE_UNITS: &[(&str, i128)] = &[("", 1), ("deg", 1_000)];

impl Nm {
    /// Parses a length such as `"1.27mm"`, `"10mil"`, `"0.5in"`, `"2.54um"` or `"7nm"`. A bare
    /// number is nanometres.
    pub fn parse(text: &str) -> Result<Nm, UnitError> {
        parse(text, LENGTH_UNITS, "length", "nanometre").map(Nm)
    }

    /// Parses a copper thickness: a length, or a copper weight such as `"1oz"` at exactly
    /// 35 µm per ounce.
    pub fn parse_copper(text: &str) -> Result<Nm, UnitError> {
        parse(text, COPPER_UNITS, "copper thickness", "nanometre").map(Nm)
    }
}

impl Millideg {
    /// Parses an angle such as `"45deg"` or `"0.5deg"`. A bare number is millidegrees.
    pub fn parse(text: &str) -> Result<Millideg, UnitError> {
        parse(text, ANGLE_UNITS, "angle", "millidegree").map(Millideg)
    }
}

fn parse(text: &str, units: &[(&str, i128)], what: &str, base: &str) -> Result<i64, UnitError> {
    let refuse = |why: String| UnitError(format!("{text:?} is not a valid {what}: {why}"));
    let trimmed = text.trim();
    let split = trimmed
        .find(|c: char| c.is_alphabetic())
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let number = number.trim_end();
    let Some(&(_, factor)) = units.iter().find(|(name, _)| *name == unit) else {
        let names: Vec<&str> = units
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| !n.is_empty())
            .collect();
        return Err(refuse(format!(
            "unknown unit {unit:?}; use one of {}",
            names.join(", ")
        )));
    };
    let (negative, digits) = match number.as_bytes().first() {
        Some(b'-') => (true, &number[1..]),
        Some(b'+') => (false, &number[1..]),
        _ => (false, number),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.len() + fraction.len() == 0 || !all_digits(whole) || !all_digits(fraction) {
        return Err(refuse(
            "expected a decimal number followed by a unit".into(),
        ));
    }
    let too_large = || refuse("it is too large".into());
    let mut mantissa: i128 = 0;
    for b in whole.bytes().chain(fraction.bytes()) {
        mantissa = mantissa
            .checked_mul(10)
            .and_then(|m| m.checked_add(i128::from(b - b'0')))
            .ok_or_else(too_large)?;
    }
    let scaled = mantissa.checked_mul(factor).ok_or_else(too_large)?;
    let divisor = u32::try_from(fraction.len())
        .ok()
        .and_then(|n| 10i128.checked_pow(n))
        .ok_or_else(too_large)?;
    if scaled % divisor != 0 {
        return Err(refuse(format!("it is not a whole number of {base}s")));
    }
    let value = scaled / divisor;
    let value = if negative { -value } else { value };
    i64::try_from(value).map_err(|_| too_large())
}
