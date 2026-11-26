//! The Part 21 lexer: bytes to tokens, with strings decoded to Rust text.

use super::SyntaxError;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Token {
    /// A keyword or entity name, upper-cased (`ISO-10303-21`, `CARTESIAN_POINT`, `!USER`).
    Keyword(String),
    Ref(u64),
    Integer(i64),
    Real(f64),
    String(String),
    Enum(String),
    Binary(String),
    Unset,
    Derived,
    LParen,
    RParen,
    Comma,
    Equals,
    Semicolon,
    Eof,
}

pub(super) struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Lexer {
            bytes,
            at: 0,
            line: 1,
        }
    }

    pub(super) fn line(&self) -> usize {
        self.line
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.at + offset).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.bytes.get(self.at).copied()?;
        self.at += 1;
        if b == b'\n' {
            self.line += 1;
        }
        Some(b)
    }

    fn error(&self, message: impl Into<String>, fatal: bool) -> SyntaxError {
        SyntaxError {
            line: self.line,
            message: message.into(),
            fatal,
        }
    }

    /// Skips whitespace and `/* comments */`.
    fn skip_blank(&mut self) -> Result<(), SyntaxError> {
        loop {
            match self.peek_at(0) {
                Some(b' ' | b'\t' | b'\r' | b'\n' | 0x0C) => {
                    self.bump();
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    let start = self.line;
                    self.bump();
                    self.bump();
                    loop {
                        match self.bump() {
                            Some(b'*') if self.peek_at(0) == Some(b'/') => {
                                self.bump();
                                break;
                            }
                            Some(_) => {}
                            None => {
                                return Err(SyntaxError {
                                    line: start,
                                    message: "unterminated comment".into(),
                                    fatal: true,
                                });
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    pub(super) fn next_token(&mut self) -> Result<Token, SyntaxError> {
        self.skip_blank()?;
        let Some(b) = self.peek_at(0) else {
            return Ok(Token::Eof);
        };
        let simple = match b {
            b'(' => Some(Token::LParen),
            b')' => Some(Token::RParen),
            b',' => Some(Token::Comma),
            b'=' => Some(Token::Equals),
            b';' => Some(Token::Semicolon),
            b'$' => Some(Token::Unset),
            b'*' => Some(Token::Derived),
            _ => None,
        };
        if let Some(t) = simple {
            self.bump();
            return Ok(t);
        }
        match b {
            b'\'' => self.string(),
            b'"' => self.binary(),
            b'#' => self.reference(),
            b'.' => self.enumeration(),
            b'0'..=b'9' | b'+' | b'-' => self.number(),
            b'A'..=b'Z' | b'a'..=b'z' | b'_' | b'!' => Ok(self.keyword()),
            _ => {
                self.bump();
                Err(self.error(format!("unexpected character {:?}", b as char), false))
            }
        }
    }

    fn keyword(&mut self) -> Token {
        let start = self.at;
        self.bump();
        while let Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-') = self.peek_at(0) {
            self.bump();
        }
        let word = String::from_utf8_lossy(&self.bytes[start..self.at]).to_ascii_uppercase();
        Token::Keyword(word)
    }

    fn reference(&mut self) -> Result<Token, SyntaxError> {
        self.bump();
        let start = self.at;
        while let Some(b'0'..=b'9') = self.peek_at(0) {
            self.bump();
        }
        let digits = std::str::from_utf8(&self.bytes[start..self.at]).expect("ASCII digits");
        digits
            .parse()
            .map(Token::Ref)
            .map_err(|_| self.error("a # with no instance number", false))
    }

    fn enumeration(&mut self) -> Result<Token, SyntaxError> {
        self.bump();
        let start = self.at;
        while let Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_') = self.peek_at(0) {
            self.bump();
        }
        let word = String::from_utf8_lossy(&self.bytes[start..self.at]).to_ascii_uppercase();
        if word.is_empty() || self.peek_at(0) != Some(b'.') {
            return Err(self.error("malformed enumeration", false));
        }
        self.bump();
        Ok(Token::Enum(word))
    }

    fn number(&mut self) -> Result<Token, SyntaxError> {
        let start = self.at;
        if let Some(b'+' | b'-') = self.peek_at(0) {
            self.bump();
        }
        let digits_start = self.at;
        while let Some(b'0'..=b'9') = self.peek_at(0) {
            self.bump();
        }
        if self.at == digits_start {
            return Err(self.error("a sign with no number", false));
        }
        let mut real = false;
        if self.peek_at(0) == Some(b'.') {
            real = true;
            self.bump();
            while let Some(b'0'..=b'9') = self.peek_at(0) {
                self.bump();
            }
        }
        if let Some(b'E' | b'e') = self.peek_at(0) {
            real = true;
            self.bump();
            if let Some(b'+' | b'-') = self.peek_at(0) {
                self.bump();
            }
            let exp_start = self.at;
            while let Some(b'0'..=b'9') = self.peek_at(0) {
                self.bump();
            }
            if self.at == exp_start {
                return Err(self.error("an exponent with no digits", false));
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).expect("ASCII number");
        let text = text.strip_prefix('+').unwrap_or(text);
        if !real && let Ok(i) = text.parse::<i64>() {
            return Ok(Token::Integer(i));
        }
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(Token::Real(v)),
            _ => Err(self.error(format!("number {text} out of range"), false)),
        }
    }

    fn binary(&mut self) -> Result<Token, SyntaxError> {
        self.bump();
        let start = self.at;
        while let Some(b'0'..=b'9' | b'A'..=b'F' | b'a'..=b'f') = self.peek_at(0) {
            self.bump();
        }
        let hex = String::from_utf8_lossy(&self.bytes[start..self.at]).to_ascii_uppercase();
        if self.bump() != Some(b'"') {
            return Err(self.error("malformed binary value", false));
        }
        Ok(Token::Binary(hex))
    }

    fn string(&mut self) -> Result<Token, SyntaxError> {
        let start_line = self.line;
        self.bump();
        let mut raw = Vec::new();
        loop {
            match self.bump() {
                Some(b'\'') if self.peek_at(0) == Some(b'\'') => {
                    self.bump();
                    raw.push(b'\'');
                }
                Some(b'\'') => break,
                // Part 21 has no line breaks in strings; exporters that wrap long strings
                // expect the break to be ignored.
                Some(b'\r' | b'\n') => {}
                Some(b) => raw.push(b),
                None => {
                    return Err(SyntaxError {
                        line: start_line,
                        message: "unterminated string".into(),
                        fatal: true,
                    });
                }
            }
        }
        Ok(Token::String(decode_string(&raw)))
    }
}

/// Decodes the Part 21 escapes of a string's content (quotes already undoubled): `\\`,
/// `\X\HH`, `\S\c`, `\P?\`, `\X2\...\X0\` and `\X4\...\X0\`. Other bytes above 0x7F are read
/// as UTF-8 where they form a valid sequence, and as ISO 8859-1 where they do not. A
/// malformed escape is kept as written.
pub(super) fn decode_string(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b == b'\\' {
            if let Some(used) = escape(&raw[i..], &mut out) {
                i += used;
                continue;
            }
            out.push('\\');
            i += 1;
        } else if b < 0x80 {
            out.push(b as char);
            i += 1;
        } else {
            let width = match b {
                0xC0..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF7 => 4,
                _ => 1,
            };
            match raw
                .get(i..i + width)
                .and_then(|s| std::str::from_utf8(s).ok())
            {
                Some(s) if width > 1 => {
                    out.push_str(s);
                    i += width;
                }
                _ => {
                    out.push(b as char);
                    i += 1;
                }
            }
        }
    }
    out
}

/// Decodes one escape at the start of `s`, returning how many bytes it used.
fn escape(s: &[u8], out: &mut String) -> Option<usize> {
    let hex =
        |s: &[u8]| -> Option<u32> { u32::from_str_radix(std::str::from_utf8(s).ok()?, 16).ok() };
    match s.get(1)? {
        b'\\' => {
            out.push('\\');
            Some(2)
        }
        b'S' if s.get(2) == Some(&b'\\') => {
            let c = *s.get(3)?;
            out.push(char::from_u32(u32::from(c) + 0x80)?);
            Some(4)
        }
        b'P' if s.get(3) == Some(&b'\\') && s.get(2)?.is_ascii_uppercase() => Some(4),
        b'X' if s.get(2) == Some(&b'\\') => {
            let c = hex(s.get(3..5)?)?;
            out.push(char::from_u32(c)?);
            Some(5)
        }
        b'X' if s.get(3) == Some(&b'\\') && matches!(s.get(2), Some(b'2' | b'4')) => {
            let width = if s[2] == b'2' { 4 } else { 8 };
            let mut at = 4;
            let mut units = Vec::new();
            loop {
                if s.get(at..at + 4)? == b"\\X0\\" {
                    at += 4;
                    break;
                }
                units.push(hex(s.get(at..at + width)?)?);
                at += width;
            }
            let text = if width == 4 {
                let units: Vec<u16> = units.into_iter().map(|u| u as u16).collect();
                String::from_utf16(&units).ok()?
            } else {
                units
                    .into_iter()
                    .map(char::from_u32)
                    .collect::<Option<String>>()?
            };
            out.push_str(&text);
            Some(at)
        }
        _ => None,
    }
}
