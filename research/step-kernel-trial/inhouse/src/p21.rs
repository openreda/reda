//! A small ISO 10303-21 (Part 21) tokenizer: splits a file into header statements and data
//! records, parses one record's parameters into a value tree, and rewrites `#n` references.
//! It works on bytes, so non-ASCII text inside strings and comments passes through unchanged.

use std::collections::HashMap;

/// One `#id = body;` statement of a DATA section. `body` has comments and line breaks removed
/// (outside strings) and no trailing semicolon.
pub struct Record {
    pub id: u64,
    pub body: Vec<u8>,
}

pub struct StepFile {
    /// Header statements (`FILE_DESCRIPTION(...)`, `FILE_NAME(...)`, `FILE_SCHEMA(...)`).
    pub header: Vec<Vec<u8>>,
    pub records: Vec<Record>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Ref(u64),
    /// The raw bytes between the quotes, with doubled quotes left doubled.
    Str(Vec<u8>),
    /// A number, an enumeration (`.T.`), `$` or `*`.
    Atom(String),
    List(Vec<Val>),
    /// A typed parameter such as `LENGTH_MEASURE(1.E-07)`.
    Typed(String, Vec<Val>),
}

impl Val {
    pub fn as_ref(&self) -> Option<u64> {
        if let Val::Ref(n) = self {
            Some(*n)
        } else {
            None
        }
    }
    pub fn as_list(&self) -> Option<&[Val]> {
        if let Val::List(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let Val::Atom(s) = self {
            s.parse().ok()
        } else {
            None
        }
    }
    pub fn as_str(&self) -> Option<String> {
        if let Val::Str(s) = self {
            Some(String::from_utf8_lossy(s).into_owned())
        } else {
            None
        }
    }
    pub fn is_true(&self) -> bool {
        matches!(self, Val::Atom(s) if s == ".T.")
    }
}

/// A parsed record: one `(type name, parameters)` pair for a simple instance, several for a
/// complex instance `#n=(A(...)B(...));`.
pub struct Entity {
    pub parts: Vec<(String, Vec<Val>)>,
}

/// Splits `bytes` into statements at `;`, honouring strings and dropping comments.
fn statements(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut i = 0;
    let n = bytes.len();
    while i < n {
        let c = bytes[i];
        match c {
            b'\'' | b'"' => {
                // A string runs to the next lone quote; a doubled quote stays inside it.
                cur.push(c);
                i += 1;
                loop {
                    if i >= n {
                        return Err("unterminated string".into());
                    }
                    let d = bytes[i];
                    if d == c {
                        if c == b'\'' && i + 1 < n && bytes[i + 1] == b'\'' {
                            cur.extend_from_slice(b"''");
                            i += 2;
                            continue;
                        }
                        cur.push(d);
                        i += 1;
                        break;
                    }
                    // Part 21 does not allow a line break inside a string; exporters that wrap
                    // long strings expect the break to be ignored.
                    if d != b'\n' && d != b'\r' {
                        cur.push(d);
                    }
                    i += 1;
                }
            }
            b'/' if i + 1 < n && bytes[i + 1] == b'*' => {
                i += 2;
                loop {
                    if i + 1 >= n {
                        return Err("unterminated comment".into());
                    }
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            b';' => {
                out.push(std::mem::take(&mut cur));
                i += 1;
            }
            b'\n' | b'\r' | b'\t' | b' ' => i += 1,
            _ => {
                cur.push(c);
                i += 1;
            }
        }
    }
    if !cur.is_empty() {
        return Err("text after the last ';'".into());
    }
    Ok(out)
}

pub fn parse_file(bytes: &[u8]) -> Result<StepFile, String> {
    let stmts = statements(bytes)?;
    if stmts.first().map(|s| s.as_slice()) != Some(b"ISO-10303-21".as_slice()) {
        return Err("not a Part 21 file: missing ISO-10303-21".into());
    }
    let mut header = Vec::new();
    let mut records = Vec::new();
    let mut section = 0; // 0 none, 1 header, 2 data
    for s in stmts.into_iter().skip(1) {
        if s == b"HEADER" {
            section = 1;
        } else if s == b"DATA" || s.starts_with(b"DATA(") {
            section = 2;
        } else if s == b"ENDSEC" {
            section = 0;
        } else if s == b"END-ISO-10303-21" {
            break;
        } else if section == 1 {
            header.push(s);
        } else if section == 2 {
            if s.first() != Some(&b'#') {
                return Err(format!(
                    "data statement does not start with '#': {}",
                    preview(&s)
                ));
            }
            let eq = s
                .iter()
                .position(|&c| c == b'=')
                .ok_or_else(|| format!("no '=' in {}", preview(&s)))?;
            let id: u64 = std::str::from_utf8(&s[1..eq])
                .ok()
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| format!("bad instance id in {}", preview(&s)))?;
            records.push(Record {
                id,
                body: s[eq + 1..].to_vec(),
            });
        }
    }
    Ok(StepFile { header, records })
}

pub fn preview(s: &[u8]) -> String {
    String::from_utf8_lossy(&s[..s.len().min(60)]).into_owned()
}

/// The type keyword of a simple instance, or `"("` for a complex instance.
pub fn type_name(body: &[u8]) -> &str {
    if body.first() == Some(&b'(') {
        return "(";
    }
    let end = body.iter().position(|&c| c == b'(').unwrap_or(body.len());
    std::str::from_utf8(&body[..end]).unwrap_or("")
}

/// Calls `f` for each `#n` reference outside strings and returns the body with the returned ids.
pub fn rewrite_refs(body: &[u8], mut f: impl FnMut(u64) -> u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 16);
    let mut i = 0;
    let n = body.len();
    while i < n {
        let c = body[i];
        if c == b'\'' || c == b'"' {
            let start = i;
            i += 1;
            while i < n {
                if body[i] == c {
                    if c == b'\'' && i + 1 < n && body[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.extend_from_slice(&body[start..i]);
        } else if c == b'#' {
            let mut j = i + 1;
            let mut v: u64 = 0;
            while j < n && body[j].is_ascii_digit() {
                v = v * 10 + u64::from(body[j] - b'0');
                j += 1;
            }
            out.push(b'#');
            out.extend_from_slice(f(v).to_string().as_bytes());
            i = j;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

pub fn refs(body: &[u8]) -> Vec<u64> {
    let mut v = Vec::new();
    rewrite_refs(body, |n| {
        v.push(n);
        n
    });
    v
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!(
                "expected '{}' at byte {} of {}",
                c as char,
                self.i,
                preview(self.b)
            ))
        }
    }
    fn keyword(&mut self) -> String {
        let s = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'!')
        {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.b[s..self.i]).into_owned()
    }
    /// Parses `( item, item, ... )` with the cursor on the opening parenthesis.
    fn list(&mut self) -> Result<Vec<Val>, String> {
        self.expect(b'(')?;
        let mut items = Vec::new();
        if self.peek() == Some(b')') {
            self.i += 1;
            return Ok(items);
        }
        loop {
            items.push(self.value()?);
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b')') => {
                    self.i += 1;
                    return Ok(items);
                }
                _ => {
                    return Err(format!(
                        "expected ',' or ')' at byte {} of {}",
                        self.i,
                        preview(self.b)
                    ));
                }
            }
        }
    }
    fn value(&mut self) -> Result<Val, String> {
        let c = self.peek().ok_or("unexpected end of record")?;
        match c {
            b'#' => {
                self.i += 1;
                let s = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.b[s..self.i]).unwrap_or("");
                t.parse()
                    .map(Val::Ref)
                    .map_err(|_| format!("bad reference in {}", preview(self.b)))
            }
            b'\'' | b'"' => {
                self.i += 1;
                let s = self.i;
                loop {
                    match self.peek() {
                        None => return Err("unterminated string".into()),
                        Some(d) if d == c => {
                            if c == b'\'' && self.b.get(self.i + 1) == Some(&b'\'') {
                                self.i += 2;
                                continue;
                            }
                            break;
                        }
                        _ => self.i += 1,
                    }
                }
                let v = self.b[s..self.i].to_vec();
                self.i += 1;
                Ok(Val::Str(v))
            }
            b'(' => Ok(Val::List(self.list()?)),
            b'$' | b'*' => {
                self.i += 1;
                Ok(Val::Atom((c as char).to_string()))
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let k = self.keyword();
                Ok(Val::Typed(k, self.list()?))
            }
            _ => {
                // A number or an enumeration: runs to the next delimiter.
                let s = self.i;
                while self
                    .peek()
                    .is_some_and(|c| c != b',' && c != b')' && c != b'(')
                {
                    self.i += 1;
                }
                if s == self.i {
                    return Err(format!("unexpected '{}' in {}", c as char, preview(self.b)));
                }
                Ok(Val::Atom(
                    String::from_utf8_lossy(&self.b[s..self.i]).into_owned(),
                ))
            }
        }
    }
}

pub fn parse_entity(body: &[u8]) -> Result<Entity, String> {
    let mut p = Parser { b: body, i: 0 };
    let mut parts = Vec::new();
    if p.peek() == Some(b'(') {
        p.i += 1;
        while p.peek() != Some(b')') {
            let k = p.keyword();
            if k.is_empty() {
                return Err(format!("bad complex instance {}", preview(body)));
            }
            parts.push((k, p.list()?));
        }
        p.i += 1;
    } else {
        let k = p.keyword();
        if k.is_empty() {
            return Err(format!("missing type name in {}", preview(body)));
        }
        parts.push((k, p.list()?));
    }
    if p.i != body.len() {
        return Err(format!("trailing text in {}", preview(body)));
    }
    Ok(Entity { parts })
}

/// Records by id, with the parsed form produced on demand.
pub struct Index<'a> {
    pub by_id: HashMap<u64, &'a Record>,
}

impl<'a> Index<'a> {
    pub fn new(file: &'a StepFile) -> Self {
        Index {
            by_id: file.records.iter().map(|r| (r.id, r)).collect(),
        }
    }
    pub fn get(&self, id: u64) -> Result<Entity, String> {
        let r = self
            .by_id
            .get(&id)
            .ok_or_else(|| format!("#{id} is not defined"))?;
        parse_entity(&r.body)
    }
    /// The parameters of simple instance `id`, which must be of type `ty`.
    pub fn args(&self, id: u64, ty: &str) -> Result<Vec<Val>, String> {
        let e = self.get(id)?;
        match e.parts.into_iter().find(|(n, _)| n == ty) {
            Some((_, a)) => Ok(a),
            None => Err(format!("#{id} is not a {ty}")),
        }
    }
    pub fn type_of(&self, id: u64) -> &str {
        self.by_id.get(&id).map_or("", |r| type_name(&r.body))
    }
}
