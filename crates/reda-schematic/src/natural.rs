//! Natural sort order: digit runs compare by number, so `R2` comes before `R10`.

use std::cmp::Ordering;

/// Compares `a` and `b` in natural order: runs of ASCII digits compare by their numeric
/// value, everything else character by character. Strings that differ only in leading
/// zeros fall back to plain order, so the order is total.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a, b);
    loop {
        match (x.chars().next(), y.chars().next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) if p.is_ascii_digit() && q.is_ascii_digit() => {
                let (m, rest_x) = digits(x);
                let (n, rest_y) = digits(y);
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let order = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if order != Ordering::Equal {
                    return order;
                }
                (x, y) = (rest_x, rest_y);
            }
            (Some(p), Some(q)) => {
                if p != q {
                    return p.cmp(&q);
                }
                (x, y) = (&x[p.len_utf8()..], &y[q.len_utf8()..]);
            }
        }
    }
}

/// The leading run of ASCII digits and the rest.
fn digits(s: &str) -> (&str, &str) {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    s.split_at(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_compare_by_value() {
        let mut names = ["R10", "R2", "C1", "R1", "R02", "U1A", "U1", "R"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["C1", "R", "R1", "R02", "R2", "R10", "U1", "U1A"]);
    }
}
