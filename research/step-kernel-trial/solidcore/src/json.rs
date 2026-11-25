//! The few JSON values the harness prints.

pub fn string(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

pub fn opt_string(s: Option<&str>) -> String {
    s.map_or_else(|| "null".into(), string)
}

pub fn number(x: f64) -> String {
    if x.is_finite() { format!("{x}") } else { "null".into() }
}

pub fn opt_number(x: Option<f64>) -> String {
    x.map_or_else(|| "null".into(), number)
}

pub fn opt_count(x: Option<usize>) -> String {
    x.map_or_else(|| "null".into(), |n| n.to_string())
}

pub fn opt_point(p: Option<[f64; 3]>) -> String {
    p.map_or_else(|| "null".into(), |p| format!("[{},{},{}]", number(p[0]), number(p[1]), number(p[2])))
}

/// The first 300 characters.
pub fn shortened(s: &str) -> String {
    s.chars().take(std::env::var("TRIAL_ERR").ok().and_then(|v| v.parse().ok()).unwrap_or(300)).collect()
}
