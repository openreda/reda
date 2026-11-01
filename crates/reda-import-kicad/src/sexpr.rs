//! A small S-expression reader for KiCad's text formats: lists, bare atoms and quoted
//! strings, nothing else. It never recurses, and it refuses nesting deeper than
//! [`MAX_DEPTH`], so hostile input can exhaust neither the stack nor, through a deep drop,
//! the heap walk.

use std::fmt;

/// The deepest list nesting accepted. Real symbol and footprint files stay under 10.
pub const MAX_DEPTH: usize = 64;

/// One node: a list, or a bare or quoted atom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sexp {
    List(Vec<Sexp>, Pos),
    /// A bare word or number, such as `smd` or `1.27`.
    Atom(String, Pos),
    /// A quoted string, escapes resolved.
    Str(String, Pos),
}

/// A 1-based line and column in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// A reading error and where it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SexpError {
    pub pos: Pos,
    pub message: String,
}

impl Sexp {
    pub fn pos(&self) -> Pos {
        match self {
            Sexp::List(_, p) | Sexp::Atom(_, p) | Sexp::Str(_, p) => *p,
        }
    }

    /// The text of an atom or string; `None` for a list.
    pub fn text(&self) -> Option<&str> {
        match self {
            Sexp::Atom(s, _) | Sexp::Str(s, _) => Some(s),
            Sexp::List(..) => None,
        }
    }

    /// A list's items; empty for an atom.
    pub fn items(&self) -> &[Sexp] {
        match self {
            Sexp::List(items, _) => items,
            _ => &[],
        }
    }

    /// The head word of a list, such as `pad` in `(pad "1" smd …)`.
    pub fn head(&self) -> Option<&str> {
        match self.items().first() {
            Some(Sexp::Atom(s, _)) => Some(s),
            _ => None,
        }
    }

    /// The items after the head.
    pub fn args(&self) -> &[Sexp] {
        self.items().get(1..).unwrap_or(&[])
    }

    /// The `n`th item after the head, as text.
    pub fn arg(&self, n: usize) -> Option<&str> {
        self.args().get(n).and_then(Sexp::text)
    }

    /// The child lists whose head is `name`.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sexp> + 'a {
        self.args().iter().filter(move |c| c.head() == Some(name))
    }

    /// The first child list whose head is `name`.
    pub fn child(&self, name: &str) -> Option<&Sexp> {
        self.args().iter().find(|c| c.head() == Some(name))
    }

    /// Whether a bare `word` appears among the items, as in `(effects … hide)`, or a child
    /// `(word yes)` does, as newer files write it.
    pub fn flag(&self, word: &str) -> bool {
        self.args().iter().any(|c| match c {
            Sexp::Atom(s, _) => s == word,
            Sexp::List(..) => c.head() == Some(word) && c.arg(0) != Some("no"),
            Sexp::Str(..) => false,
        })
    }
}

/// Reads one top-level list from `src`. Anything after it other than white space is an
/// error.
pub fn parse(src: &str) -> Result<Sexp, SexpError> {
    let mut reader = Reader {
        chars: src.char_indices().peekable(),
        src,
        line: 1,
        column: 1,
    };
    reader.skip_space();
    let first = reader.pos();
    if reader.peek() != Some('(') {
        return Err(SexpError {
            pos: first,
            message: "expected `(` at the start of the file".into(),
        });
    }
    let root = reader.list()?;
    reader.skip_space();
    if reader.peek().is_some() {
        return Err(SexpError {
            pos: reader.pos(),
            message: "unexpected text after the closing `)`".into(),
        });
    }
    Ok(root)
}

struct Reader<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    src: &'a str,
    line: u32,
    column: u32,
}

impl Reader<'_> {
    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            column: self.column,
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn bump(&mut self) -> Option<char> {
        let (_, c) = self.chars.next()?;
        if c == '\n' {
            self.line = self.line.saturating_add(1);
            self.column = 1;
        } else {
            self.column = self.column.saturating_add(1);
        }
        Some(c)
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn error<T>(&self, pos: Pos, message: impl Into<String>) -> Result<T, SexpError> {
        Err(SexpError {
            pos,
            message: message.into(),
        })
    }

    /// Reads the list starting at the current `(`, without recursion.
    fn list(&mut self) -> Result<Sexp, SexpError> {
        let mut stack: Vec<(Vec<Sexp>, Pos)> = vec![];
        loop {
            self.skip_space();
            let pos = self.pos();
            match self.peek() {
                None => {
                    let open = stack.last().map(|(_, p)| *p).unwrap_or(pos);
                    return self.error(open, "this `(` is never closed");
                }
                Some('(') => {
                    self.bump();
                    if stack.len() >= MAX_DEPTH {
                        return self.error(pos, format!("lists nest deeper than {MAX_DEPTH}"));
                    }
                    stack.push((vec![], pos));
                }
                Some(')') => {
                    self.bump();
                    let Some((items, open)) = stack.pop() else {
                        return self.error(pos, "unexpected `)`");
                    };
                    let list = Sexp::List(items, open);
                    match stack.last_mut() {
                        Some((parent, _)) => parent.push(list),
                        None => return Ok(list),
                    }
                }
                Some('"') => {
                    let s = self.string()?;
                    match stack.last_mut() {
                        Some((parent, _)) => parent.push(Sexp::Str(s, pos)),
                        None => return self.error(pos, "expected `(`"),
                    }
                }
                Some(_) => {
                    let atom = self.atom();
                    match stack.last_mut() {
                        Some((parent, _)) => parent.push(Sexp::Atom(atom, pos)),
                        None => return self.error(pos, "expected `(`"),
                    }
                }
            }
        }
    }

    fn atom(&mut self) -> String {
        let start = self.chars.peek().map(|(i, _)| *i).unwrap_or(self.src.len());
        let mut end = start;
        while let Some(&(i, c)) = self.chars.peek() {
            if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
                break;
            }
            end = i + c.len_utf8();
            self.bump();
        }
        self.src[start..end].to_owned()
    }

    fn string(&mut self) -> Result<String, SexpError> {
        let open = self.pos();
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return self.error(open, "this string is never closed"),
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some(c) => out.push(c),
                    None => return self.error(open, "this string is never closed"),
                },
                Some(c) => out.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lists_atoms_and_strings() {
        let s = parse("(pad \"1\" smd (at 1.5 -2))").unwrap();
        assert_eq!(s.head(), Some("pad"));
        assert_eq!(s.arg(0), Some("1"));
        assert_eq!(s.arg(1), Some("smd"));
        let at = s.child("at").unwrap();
        assert_eq!((at.arg(0), at.arg(1)), (Some("1.5"), Some("-2")));
    }

    #[test]
    fn strings_resolve_escapes() {
        let s = parse(r#"(t "a \"b\"\nc")"#).unwrap();
        assert_eq!(s.arg(0), Some("a \"b\"\nc"));
    }

    #[test]
    fn errors_name_the_place() {
        let e = parse("(a\n  (b").unwrap_err();
        assert_eq!(e.pos, Pos { line: 2, column: 3 });
        let e = parse("(a) x").unwrap_err();
        assert!(e.message.contains("after"), "{e:?}");
    }

    #[test]
    fn deep_nesting_is_refused() {
        let deep = "(".repeat(MAX_DEPTH + 1) + &")".repeat(MAX_DEPTH + 1);
        assert!(parse(&deep).unwrap_err().message.contains("deeper"));
        let ok = "(".repeat(MAX_DEPTH) + &")".repeat(MAX_DEPTH);
        assert!(parse(&ok).is_ok());
    }
}
