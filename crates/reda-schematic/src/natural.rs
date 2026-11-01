//! Natural sort order: digit runs compare as numbers, so `R2` sorts before `R10`.

use std::cmp::Ordering;

/// Compares `a` and `b` in natural order. Runs of ASCII digits compare by numeric value,
/// everything else character by character. Strings that tie (`R01` and `R1`) fall back to
/// plain string order, so the order is total.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    loop {
        match (x.first(), y.first()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let (m, rest_x) = digits(x);
                let (n, rest_y) = digits(y);
                let (m, n) = (trim_zeros(m), trim_zeros(n));
                let by_value = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if by_value != Ordering::Equal {
                    return by_value;
                }
                (x, y) = (rest_x, rest_y);
            }
            (Some(c), Some(d)) => {
                if c != d {
                    // Compare whole characters, so non-ASCII text orders by code point.
                    let cx = char_at(x);
                    let cy = char_at(y);
                    return cx.cmp(&cy);
                }
                (x, y) = (&x[1..], &y[1..]);
            }
        }
    }
}

fn digits(s: &[u8]) -> (&[u8], &[u8]) {
    let end = s
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(s.len());
    s.split_at(end)
}

fn trim_zeros(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&b| b != b'0').unwrap_or(s.len());
    &s[start..]
}

/// The (possibly multi-byte) character starting at `s`; the bytes are valid UTF-8 from a
/// `&str` split only at ASCII boundaries or equal prefixes.
fn char_at(s: &[u8]) -> char {
    let len = s.len().min(4);
    (1..=len)
        .find_map(|n| std::str::from_utf8(&s[..n]).ok())
        .and_then(|t| t.chars().next())
        .unwrap_or(char::REPLACEMENT_CHARACTER)
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering::*;

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(natural_cmp("R2", "R10"), Less);
        assert_eq!(natural_cmp("R10", "R9"), Greater);
        assert_eq!(natural_cmp("2", "10"), Less);
        assert_eq!(natural_cmp("C1", "R1"), Less);
        assert_eq!(natural_cmp("R1", "R1"), Equal);
        assert_eq!(natural_cmp("R1", "R1A"), Less);
        assert_eq!(natural_cmp("A10", "A9B"), Greater);
    }

    #[test]
    fn ties_by_value_fall_back_to_string_order() {
        assert_eq!(natural_cmp("R01", "R1"), Less);
        assert_eq!(natural_cmp("R1", "R01"), Greater);
    }
}
