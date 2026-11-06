//! Auto-increment: how a repeat placement names the next Designator, Net Label, Port or
//! Pin. Only a trailing number steps, keeping its zero padding (`D09` → `D10`); a trailing
//! overbar (`CS1\`) is ignored when finding it, and a name with no trailing number repeats
//! unchanged. A negative step counts down and stops at 0.

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

/// `name` as (prefix, trailing digits, trailing overbar markers), when it ends in a number.
fn split(name: &str) -> Option<(&str, &str, &str)> {
    let body = name.trim_end_matches('\\');
    let suffix = &name[body.len()..];
    let prefix = body.trim_end_matches(|c: char| c.is_ascii_digit());
    let number = &body[prefix.len()..];
    (!number.is_empty()).then_some((prefix, number, suffix))
}
