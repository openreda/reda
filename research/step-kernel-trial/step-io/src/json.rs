//! A one-line JSON object writer; the harness needs nothing more.

pub struct Obj(String);

impl Obj {
    pub fn new() -> Self {
        Self(String::from("{"))
    }

    fn key(&mut self, k: &str) {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        self.0.push_str(&quote(k));
        self.0.push(':');
    }

    pub fn raw(&mut self, k: &str, v: &str) -> &mut Self {
        self.key(k);
        self.0.push_str(v);
        self
    }

    pub fn null(&mut self, k: &str) -> &mut Self {
        self.raw(k, "null")
    }

    pub fn bool(&mut self, k: &str, v: bool) -> &mut Self {
        self.raw(k, if v { "true" } else { "false" })
    }

    pub fn num(&mut self, k: &str, v: f64) -> &mut Self {
        if v.is_finite() {
            self.raw(k, &format!("{v}"))
        } else {
            self.null(k)
        }
    }

    pub fn int(&mut self, k: &str, v: usize) -> &mut Self {
        self.raw(k, &v.to_string())
    }

    pub fn opt_int(&mut self, k: &str, v: Option<usize>) -> &mut Self {
        match v {
            Some(v) => self.int(k, v),
            None => self.null(k),
        }
    }

    pub fn str(&mut self, k: &str, v: &str) -> &mut Self {
        self.raw(k, &quote(v))
    }

    pub fn opt_str(&mut self, k: &str, v: Option<&str>) -> &mut Self {
        match v {
            Some(v) => self.str(k, v),
            None => self.null(k),
        }
    }

    pub fn finish(&self) -> String {
        format!("{}}}", self.0)
    }
}

pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The first 300 characters of an error message.
pub fn shorten(s: &str) -> String {
    s.chars().take(300).collect()
}
