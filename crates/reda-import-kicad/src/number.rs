//! Decimal numbers from KiCad files, read straight into fixed point without floats: lengths
//! in millimetres become nanometres, angles in degrees become millidegrees.

use reda_model::geom::{Millideg, Nm};

/// `text` as an integer count of `10^-decimals` units, rounding half away from zero past
/// the last kept digit. Only plain decimals are accepted: an optional sign, digits and an
/// optional fraction. `None` for anything else, or on overflow.
pub fn fixed(text: &str, decimals: u32) -> Option<i64> {
    let (negative, rest) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    let (int, frac) = rest.split_once('.').unwrap_or((rest, ""));
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    if !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut value: i64 = 0;
    for b in int.bytes() {
        value = value.checked_mul(10)?.checked_add(i64::from(b - b'0'))?;
    }
    let mut digits = frac.bytes();
    for _ in 0..decimals {
        let d = digits.next().map_or(0, |b| b - b'0');
        value = value.checked_mul(10)?.checked_add(i64::from(d))?;
    }
    if digits.next().is_some_and(|b| b >= b'5') {
        value = value.checked_add(1)?;
    }
    Some(if negative { -value } else { value })
}

/// Millimetres as nanometres.
pub fn mm(text: &str) -> Option<Nm> {
    fixed(text, 6).map(Nm)
}

/// Degrees as millidegrees.
pub fn degrees(text: &str) -> Option<Millideg> {
    fixed(text, 3).map(Millideg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_become_exact_fixed_point() {
        assert_eq!(mm("1.27"), Some(Nm(1_270_000)));
        assert_eq!(mm("-0.9125"), Some(Nm(-912_500)));
        assert_eq!(mm("3"), Some(Nm(3_000_000)));
        assert_eq!(mm(".5"), Some(Nm(500_000)));
        assert_eq!(mm("0.0000005"), Some(Nm(1)), "half rounds away from zero");
        assert_eq!(mm("-0.0000004"), Some(Nm(0)));
        assert_eq!(degrees("90"), Some(Millideg(90_000)));
        assert_eq!(degrees("-45.5"), Some(Millideg(-45_500)));
    }

    #[test]
    fn anything_but_a_plain_decimal_is_refused() {
        for bad in ["", "-", ".", "1e3", "1.2.3", "abc", "0x10", "1,5", " 1"] {
            assert_eq!(mm(bad), None, "{bad:?}");
        }
        assert_eq!(mm("99999999999999999999"), None, "overflow");
    }
}
