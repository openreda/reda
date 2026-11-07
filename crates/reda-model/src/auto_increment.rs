//! Auto-increment: how a repeat placement names the next Designator, Net Label, Port or
//! Pin. Only a trailing number steps, keeping its zero padding (`D09` → `D10`); a trailing
//! overbar (`CS1\`) is ignored when finding it, and a name with no trailing number repeats
//! unchanged. A negative step counts down and stops at 0.
//!
//! Pin and Pad numbers ([`step_number`]) also step when they are made only of letters,
//! A → B → Z → AA, as BGA rows are numbered; names and Designators keep the
//! trailing-number rule.

/// `name` with its trailing number stepped `by`: `D09` → `D10`, `CS1\` → `CS2\`. Zero
/// padding is kept; a name with no trailing number comes back unchanged, and counting
/// down stops at 0.
pub fn step(name: &str, by: i64) -> String {
    let Some((prefix, number, suffix)) = split(name) else {
        return name.to_owned();
    };
    let Ok(n) = number.parse::<u128>() else {
        return name.to_owned();
    };
    let next = if by < 0 {
        n.saturating_sub(by.unsigned_abs().into())
    } else {
        n.saturating_add(by.unsigned_abs().into())
    };
    format!("{prefix}{next:0width$}{suffix}", width = number.len())
}

/// The Designator after `designator`, stepping its number `by` and skipping every one
/// `free` refuses. `None` when it has no trailing number, `by` is 0, or counting down
/// reaches 0 without finding a free one.
pub fn step_designator(designator: &str, by: i64, free: impl Fn(&str) -> bool) -> Option<String> {
    if by == 0 || split(designator).is_none() {
        return None;
    }
    let mut current = designator.to_owned();
    loop {
        let next = step(&current, by);
        if next == current {
            return None;
        }
        if free(&next) {
            return Some(next);
        }
        current = next;
    }
}

/// The Pin or Pad number after `number`, stepping `by`: a number made only of ASCII
/// letters counts A → B … Z → AA (keeping each letter's case, and stopping at A when
/// counting down); any other steps its trailing number like [`step`].
pub fn step_number(number: &str, by: i64) -> String {
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_alphabetic()) {
        return step(number, by);
    }
    let value = number.bytes().fold(0u128, |v, b| {
        v.saturating_mul(26)
            .saturating_add(u128::from(b.to_ascii_uppercase() - b'A' + 1))
    });
    let next = if by < 0 {
        value.saturating_sub(by.unsigned_abs().into()).max(1)
    } else {
        value.saturating_add(by.unsigned_abs().into())
    };
    let mut letters = Vec::new();
    let mut n = next;
    while n > 0 {
        n -= 1;
        letters.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    // Case follows the original letter at the same place from the right; new leading
    // letters take the first letter's case.
    let original = number.as_bytes();
    let lower_first = original[0].is_ascii_lowercase();
    letters
        .iter()
        .enumerate()
        .rev()
        .map(|(i, &b)| {
            let lower = original
                .len()
                .checked_sub(i + 1)
                .map_or(lower_first, |at| original[at].is_ascii_lowercase());
            char::from(if lower { b.to_ascii_lowercase() } else { b })
        })
        .collect()
}

/// The Pin or Pad number after `last` ([`step_number`]), skipping every one `free`
/// refuses, since numbers are unique in their Symbol or Footprint. `None` when `by` is 0
/// or `last` cannot step any further.
pub fn next_number(last: &str, by: i64, free: impl Fn(&str) -> bool) -> Option<String> {
    if by == 0 {
        return None;
    }
    let mut current = last.to_owned();
    loop {
        let next = step_number(&current, by);
        if next == current {
            return None;
        }
        if free(&next) {
            return Some(next);
        }
        current = next;
    }
}

/// `name` as (prefix, trailing digits, trailing overbar markers), when it ends in a number.
fn split(name: &str) -> Option<(&str, &str, &str)> {
    let body = name.trim_end_matches('\\');
    let suffix = &name[body.len()..];
    let prefix = body.trim_end_matches(|c: char| c.is_ascii_digit());
    let number = &body[prefix.len()..];
    (!number.is_empty()).then_some((prefix, number, suffix))
}
