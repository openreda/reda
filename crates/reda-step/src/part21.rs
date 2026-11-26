//! The ISO 10303-21 (Part 21) exchange structure: the untyped layer under the records.
//!
//! [`parse`] turns a file's bytes into header entities and data instances whose parameters
//! are [`Param`] trees; [`write`](crate::write) goes the other way through [`encode_string`]
//! and [`encode_real`]. The lexer works on bytes: a string may hold UTF-8, stray ISO 8859-1
//! bytes and the Part 21 escapes, which are all decoded to Rust text here.

mod lex;

use lex::{Lexer, Token};

/// One parameter of an entity instance.
#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    /// `$`: no value.
    Unset,
    /// `*`: a value derived from other attributes.
    Derived,
    Integer(i64),
    Real(f64),
    /// A string, escapes decoded.
    String(String),
    /// An enumeration value, without its dots: `.T.` is `Enum("T")`.
    Enum(String),
    /// A reference to another instance. In the exchange structure it is the file's `#n`; in a
    /// [`RawRecord`](crate::records::RawRecord) it is the index of a
    /// [`RecordId`](crate::RecordId).
    Ref(u64),
    /// A binary value, as its hex digits (the first one counts the unused bits).
    Binary(String),
    List(Vec<Param>),
    /// A typed parameter such as `LENGTH_MEASURE(1.E-07)`.
    Typed(String, Box<Param>),
}

/// One `NAME(params)` of an instance: the whole of a simple instance, one part of a complex one.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub params: Vec<Param>,
}

/// One `#id = ...;` of a DATA section.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub id: u64,
    /// One part for a simple instance; several, in file order, for a complex instance.
    pub parts: Vec<Part>,
    pub complex: bool,
    /// The line the instance starts on, for messages.
    pub line: usize,
}

/// A parsed file: its header entities and its data instances, in file order.
#[derive(Clone, Debug, Default)]
pub struct Exchange {
    pub header: Vec<Part>,
    pub data: Vec<Instance>,
    /// Instances that could not be parsed and were skipped, one message each.
    pub skipped: Vec<String>,
}

/// Why a file is not Part 21 at all.
#[derive(Clone, Debug, PartialEq)]
pub struct SyntaxError {
    pub line: usize,
    pub message: String,
    /// A lexical fault (an unterminated string or comment) that no recovery can step over.
    pub fatal: bool,
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Lists and typed parameters nest no deeper than this; a deeper instance is skipped rather
/// than risking the stack.
const MAX_DEPTH: usize = 64;

/// Parses a Part 21 file. A structural fault (no `ISO-10303-21;`, no header, an unterminated
/// string or section) refuses the file; a malformed data instance is skipped and noted in
/// [`Exchange::skipped`].
pub fn parse(bytes: &[u8]) -> Result<Exchange, SyntaxError> {
    let mut p = Parser {
        lex: Lexer::new(bytes),
        peeked: None,
        after_semicolon: false,
    };
    p.expect_keyword("ISO-10303-21")?;
    p.expect(&Token::Semicolon)?;
    let mut out = Exchange::default();
    p.expect_keyword("HEADER")?;
    p.expect(&Token::Semicolon)?;
    loop {
        match p.next()? {
            Token::Keyword(k) if k == "ENDSEC" => {
                p.expect(&Token::Semicolon)?;
                break;
            }
            Token::Keyword(name) => {
                let part = p.part_body(name)?;
                p.expect(&Token::Semicolon)?;
                out.header.push(part);
            }
            t => return Err(p.error(format!("unexpected {t:?} in the header"))),
        }
    }
    let mut data_sections = 0;
    loop {
        match p.next()? {
            Token::Keyword(k) if k == "END-ISO-10303-21" => {
                p.expect(&Token::Semicolon)?;
                break;
            }
            Token::Keyword(k) if k == "DATA" => {
                data_sections += 1;
                // Edition 3 may name the section: DATA('name',('SCHEMA'));
                if p.peek()? == &Token::LParen {
                    p.params(0)?;
                }
                p.expect(&Token::Semicolon)?;
                p.data_section(&mut out)?;
            }
            Token::Keyword(k) if matches!(k.as_str(), "ANCHOR" | "REFERENCE" | "SIGNATURE") => {
                p.skip_section()?;
            }
            Token::Eof => return Err(p.error("no END-ISO-10303-21".into())),
            t => return Err(p.error(format!("unexpected {t:?} between sections"))),
        }
    }
    if data_sections == 0 {
        return Err(p.error("no DATA section".into()));
    }
    Ok(out)
}

struct Parser<'a> {
    lex: Lexer<'a>,
    peeked: Option<Token>,
    /// Whether the token last taken was a `;`, so recovery knows the statement has ended.
    after_semicolon: bool,
}

impl Parser<'_> {
    fn next(&mut self) -> Result<Token, SyntaxError> {
        let t = match self.peeked.take() {
            Some(t) => t,
            None => self.lex.next_token()?,
        };
        self.after_semicolon = t == Token::Semicolon;
        Ok(t)
    }

    fn peek(&mut self) -> Result<&Token, SyntaxError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.lex.next_token()?);
        }
        Ok(self.peeked.as_ref().expect("just peeked"))
    }

    fn error(&self, message: String) -> SyntaxError {
        SyntaxError {
            line: self.lex.line(),
            message,
            fatal: false,
        }
    }

    fn expect(&mut self, want: &Token) -> Result<(), SyntaxError> {
        let got = self.next()?;
        if &got == want {
            Ok(())
        } else {
            Err(self.error(format!("expected {want:?}, found {got:?}")))
        }
    }

    fn expect_keyword(&mut self, want: &str) -> Result<(), SyntaxError> {
        match self.next()? {
            Token::Keyword(k) if k == want => Ok(()),
            t => Err(self.error(format!("expected {want}, found {t:?}"))),
        }
    }

    fn skip_section(&mut self) -> Result<(), SyntaxError> {
        loop {
            match self.next()? {
                Token::Keyword(k) if k == "ENDSEC" => return self.expect(&Token::Semicolon),
                Token::Eof => return Err(self.error("unterminated section".into())),
                _ => {}
            }
        }
    }

    /// Skips to just after the `;` that ends a malformed instance.
    fn recover(&mut self) -> Result<(), SyntaxError> {
        if self.after_semicolon && self.peeked.is_none() {
            return Ok(());
        }
        loop {
            match self.next()? {
                Token::Semicolon => return Ok(()),
                Token::Eof => return Err(self.error("unterminated DATA section".into())),
                _ => {}
            }
        }
    }

    fn data_section(&mut self, out: &mut Exchange) -> Result<(), SyntaxError> {
        loop {
            let line = self.lex.line();
            match self.next()? {
                Token::Keyword(k) if k == "ENDSEC" => return self.expect(&Token::Semicolon),
                Token::Ref(id) => match self.instance(id, line) {
                    Ok(instance) => out.data.push(instance),
                    Err(e) => {
                        if e.fatal {
                            return Err(e);
                        }
                        out.skipped
                            .push(format!("#{id} (line {line}) skipped: {}", e.message));
                        self.recover()?;
                    }
                },
                Token::Eof => return Err(self.error("unterminated DATA section".into())),
                t => {
                    out.skipped
                        .push(format!("line {line}: unexpected {t:?} skipped"));
                    self.recover()?;
                }
            }
        }
    }

    /// The rest of `#id = ... ;` after the id. On a fault inside it the caller recovers; a
    /// lexer fault (an unterminated string) is returned as is and stops the file.
    fn instance(&mut self, id: u64, line: usize) -> Result<Instance, SyntaxError> {
        self.expect(&Token::Equals)?;
        let (parts, complex) = match self.next()? {
            Token::Keyword(name) => (vec![self.part_body(name)?], false),
            Token::LParen => {
                let mut parts = Vec::new();
                loop {
                    match self.next()? {
                        Token::Keyword(name) => parts.push(self.part_body(name)?),
                        Token::RParen if !parts.is_empty() => break,
                        t => {
                            return Err(
                                self.error(format!("unexpected {t:?} in a complex instance"))
                            );
                        }
                    }
                }
                (parts, true)
            }
            t => return Err(self.error(format!("unexpected {t:?} after ="))),
        };
        // Leave the `;` peeked if it is wrong, so recovery does not eat the next instance.
        match self.peek()? {
            Token::Semicolon => {
                self.peeked = None;
                Ok(Instance {
                    id,
                    parts,
                    complex,
                    line,
                })
            }
            t => {
                let t = t.clone();
                Err(self.error(format!("expected ';', found {t:?}")))
            }
        }
    }

    fn part_body(&mut self, name: String) -> Result<Part, SyntaxError> {
        self.expect(&Token::LParen)?;
        let params = self.params_after_paren(1)?;
        Ok(Part { name, params })
    }

    /// `( params )`, the opening parenthesis not yet read.
    fn params(&mut self, depth: usize) -> Result<Vec<Param>, SyntaxError> {
        self.expect(&Token::LParen)?;
        self.params_after_paren(depth + 1)
    }

    fn params_after_paren(&mut self, depth: usize) -> Result<Vec<Param>, SyntaxError> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply".into()));
        }
        let mut out = Vec::new();
        if self.peek()? == &Token::RParen {
            self.peeked = None;
            return Ok(out);
        }
        loop {
            out.push(self.param(depth)?);
            match self.next()? {
                Token::Comma => {}
                Token::RParen => return Ok(out),
                t => return Err(self.error(format!("expected ',' or ')', found {t:?}"))),
            }
        }
    }

    fn param(&mut self, depth: usize) -> Result<Param, SyntaxError> {
        Ok(match self.next()? {
            Token::Unset => Param::Unset,
            Token::Derived => Param::Derived,
            Token::Integer(i) => Param::Integer(i),
            Token::Real(r) => Param::Real(r),
            Token::String(s) => Param::String(s),
            Token::Enum(e) => Param::Enum(e),
            Token::Ref(r) => Param::Ref(r),
            Token::Binary(b) => Param::Binary(b),
            Token::LParen => Param::List(self.params_after_paren(depth + 1)?),
            Token::Keyword(name) => {
                let mut inner = self.params(depth)?;
                if inner.len() != 1 {
                    return Err(self.error(format!("typed parameter {name} needs one value")));
                }
                Param::Typed(name, Box::new(inner.remove(0)))
            }
            t => return Err(self.error(format!("unexpected {t:?} as a parameter"))),
        })
    }
}

/// Writes `s` as a Part 21 string literal, quotes included. Printable ASCII is written as is
/// (`'` doubled, `\` doubled); everything else goes in `\X2\` runs of UTF-16 code units, or
/// `\X4\` for a character outside the Basic Multilingual Plane.
pub fn encode_string(s: &str, out: &mut String) {
    use std::fmt::Write as _;
    out.push('\'');
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ if (c as u32) < 0x1_0000 => {
                out.push_str("\\X2\\");
                write!(out, "{:04X}", c as u32).expect("string write");
                while let Some(&n) = chars.peek() {
                    if (' '..='~').contains(&n) || (n as u32) >= 0x1_0000 {
                        break;
                    }
                    write!(out, "{:04X}", n as u32).expect("string write");
                    chars.next();
                }
                out.push_str("\\X0\\");
            }
            _ => {
                out.push_str("\\X4\\");
                write!(out, "{:08X}", c as u32).expect("string write");
                while let Some(&n) = chars.peek() {
                    if (n as u32) < 0x1_0000 {
                        break;
                    }
                    write!(out, "{:08X}", n as u32).expect("string write");
                    chars.next();
                }
                out.push_str("\\X0\\");
            }
        }
    }
    out.push('\'');
}

/// Writes a real in Part 21 form (`1.`, `0.25`, `1.E-07`), with the fewest digits that read
/// back as the same `f64`. A value that is not finite has no Part 21 form and is written as 0.
pub fn encode_real(v: f64, out: &mut String) {
    let v = if v.is_finite() { v } else { 0.0 };
    // Rust's `{:?}` is the shortest round-trip form: `1.0`, `0.25`, `1e-7`, `1.5e21`.
    let s = format!("{v:?}");
    let (mantissa, exponent) = match s.split_once('e') {
        Some((m, e)) => (m, Some(e)),
        None => (s.as_str(), None),
    };
    out.push_str(mantissa.strip_suffix(".0").unwrap_or(mantissa));
    if !mantissa.contains('.') || mantissa.ends_with(".0") {
        out.push('.');
    }
    if let Some(e) = exponent {
        out.push('E');
        out.push_str(e);
    }
}

/// Writes one parameter. A reference `Ref(n)` is written as `#(n + ref_base)`.
pub fn encode_param(p: &Param, ref_base: u64, out: &mut String) {
    match p {
        Param::Unset => out.push('$'),
        Param::Derived => out.push('*'),
        Param::Integer(i) => out.push_str(&i.to_string()),
        Param::Real(r) => encode_real(*r, out),
        Param::String(s) => encode_string(s, out),
        Param::Enum(e) => {
            out.push('.');
            out.push_str(e);
            out.push('.');
        }
        Param::Ref(r) => {
            out.push('#');
            out.push_str(&(r + ref_base).to_string());
        }
        Param::Binary(b) => {
            out.push('"');
            out.push_str(b);
            out.push('"');
        }
        Param::List(items) => {
            out.push('(');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                encode_param(item, ref_base, out);
            }
            out.push(')');
        }
        Param::Typed(name, inner) => {
            out.push_str(name);
            out.push('(');
            encode_param(inner, ref_base, out);
            out.push(')');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reals_are_written_in_part_21_form_and_read_back_exactly() {
        for (v, want) in [
            (1.0, "1."),
            (0.25, "0.25"),
            (-2.0, "-2."),
            (1e-7, "1.E-7"),
            (1.5e21, "1.5E21"),
            (0.1, "0.1"),
            (-0.0, "-0."),
        ] {
            let mut s = String::new();
            encode_real(v, &mut s);
            assert_eq!(s, want, "{v}");
            assert_eq!(s.parse::<f64>().unwrap(), v, "{s}");
        }
    }
}
