//! Lengths as Board Setup shows and reads them: mm unless a unit is typed.

use reda_engine::model::geom::Nm;

use crate::tr;

/// `nm` in mm, with no trailing zeros (`160000` → `0.16`).
pub fn mm(nm: Nm) -> String {
    let sign = if nm.0 < 0 { "-" } else { "" };
    let abs = nm.0.unsigned_abs();
    let (whole, frac) = (abs / 1_000_000, abs % 1_000_000);
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let frac = format!("{frac:06}");
    format!("{sign}{whole}.{}", frac.trim_end_matches('0'))
}

/// A typed length that may be left empty (`Ok(None)`) or negative: mm unless it names a
/// unit (`0.2`, `-0.05mm`, `8mil`).
pub fn signed(text: &str) -> Result<Option<Nm>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let with_unit = if text.ends_with(|c: char| c.is_ascii_digit() || c == '.') {
        format!("{text}mm")
    } else {
        text.to_owned()
    };
    Nm::parse(&with_unit)
        .map(Some)
        .map_err(|_| tr!("board-setup-not-a-length"))
}

/// A typed length that may be left empty (`Ok(None)`): mm unless it names a unit
/// (`0.2`, `0.2mm`, `8mil`), never negative.
pub fn optional(text: &str) -> Result<Option<Nm>, String> {
    match signed(text)? {
        Some(nm) if nm.0 < 0 => Err(tr!("board-setup-negative")),
        other => Ok(other),
    }
}
