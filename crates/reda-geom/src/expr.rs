//! Numeric expressions typed into a Properties field (`editing.md#numeric-expressions`).
//!
//! A field accepts a plain value with optional units (`1.5`, `10mil`, `+0.1mm`), the
//! current value as `!` (`! + 0.1mm`, `! * 2`, `(! + 5mil) / 2`), and other attributes of
//! the same object by English schema name (`Width + 5mil`), with `+ - * /` and brackets.
//! A bare number takes the field's display units when it stands for a length or angle,
//! and is a plain factor when it multiplies or divides one.
//!
//! Arithmetic is exact (rationals over `i128`), and only the result is rounded to the
//! nearest nanometre or millidegree, halves away from zero. Function names such as `sqrt`
//! are reserved ([`RESERVED_FUNCTIONS`]) and refused, as is any call.
//!
//! [`evaluate_each`] runs one expression per object of a multi-selection and fails as a
//! whole if any object fails, so the caller commits all the results or none.

use std::fmt;

/// What a field holds: a length in nanometres or an angle in millidegrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Length,
    Angle,
}

/// A numeric field: its kind and the display unit a bare number is read in (`mm`, `mil`,
/// `in`, `um`, `nm` for lengths; `deg` for angles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field<'a> {
    pub kind: Kind,
    pub unit: &'a str,
}

/// One attribute value an expression can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attr {
    Length(crate::Nm),
    Angle(crate::Millideg),
    /// A plain number, such as a priority.
    Number(i64),
}

/// The object an expression runs on: the field's current value (`!`), in nanometres or
/// millidegrees, and its other numeric attributes by schema name (`width`, `clearance`).
/// `current` is `None` when the field has no value of its own, such as a Zone clearance
/// left to the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Object<'a> {
    pub current: Option<i64>,
    pub attributes: &'a [(&'a str, Attr)],
}

/// Names held for functions in a later milestone. They are refused in expressions, and no
/// attribute may be named by them.
pub const RESERVED_FUNCTIONS: &[&str] = &[
    "abs", "acos", "asin", "atan", "atan2", "ceil", "cos", "exp", "floor", "hypot", "ln", "log",
    "log10", "max", "min", "mod", "pow", "round", "sin", "sqrt", "tan", "trunc",
];

/// Why an expression was refused, written for the person who typed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprError(String);

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExprError {}

/// Which object of [`evaluate_each`] failed, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EachError {
    /// The index of the object in the slice given.
    pub object: usize,
    pub error: ExprError,
}

impl fmt::Display for EachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for EachError {}

/// Evaluates `text` for `object` in `field`: nanometres for a length field, millidegrees
/// for an angle field.
pub fn evaluate(text: &str, field: Field, object: &Object) -> Result<i64, ExprError> {
    let expr = parse(text)?;
    resolve(&expr, field, object)
}

/// Evaluates `text` once per object, in order. Any failure fails the whole run, naming the
/// object; a syntax error is reported against the first.
pub fn evaluate_each(text: &str, field: Field, objects: &[Object]) -> Result<Vec<i64>, EachError> {
    let expr = parse(text).map_err(|error| EachError { object: 0, error })?;
    objects
        .iter()
        .enumerate()
        .map(|(i, object)| {
            resolve(&expr, field, object).map_err(|error| EachError { object: i, error })
        })
        .collect()
}

fn refuse<T>(why: impl Into<String>) -> Result<T, ExprError> {
    Err(ExprError(why.into()))
}

// ---------------------------------------------------------------------------------------
// Exact rationals.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ratio {
    num: i128,
    /// Always positive.
    den: i128,
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn too_large() -> ExprError {
    ExprError("the result is too large".into())
}

impl Ratio {
    fn int(n: i128) -> Ratio {
        Ratio { num: n, den: 1 }
    }

    fn new(num: i128, den: i128) -> Result<Ratio, ExprError> {
        if den == 0 {
            return refuse("it divides by zero");
        }
        let g = gcd(num, den).max(1);
        let sign = if den < 0 { -1 } else { 1 };
        Ok(Ratio {
            num: num / g * sign,
            den: den / g * sign,
        })
    }

    fn add(self, other: Ratio) -> Result<Ratio, ExprError> {
        let num = self
            .num
            .checked_mul(other.den)
            .zip(other.num.checked_mul(self.den))
            .and_then(|(a, b)| a.checked_add(b))
            .ok_or_else(too_large)?;
        let den = self.den.checked_mul(other.den).ok_or_else(too_large)?;
        Ratio::new(num, den)
    }

    fn neg(self) -> Ratio {
        Ratio {
            num: -self.num,
            den: self.den,
        }
    }

    fn mul(self, other: Ratio) -> Result<Ratio, ExprError> {
        // Cross-reduce first to keep the numbers small.
        let g1 = gcd(self.num, other.den).max(1);
        let g2 = gcd(other.num, self.den).max(1);
        let num = (self.num / g1)
            .checked_mul(other.num / g2)
            .ok_or_else(too_large)?;
        let den = (self.den / g2)
            .checked_mul(other.den / g1)
            .ok_or_else(too_large)?;
        Ratio::new(num, den)
    }

    fn div(self, other: Ratio) -> Result<Ratio, ExprError> {
        if other.num == 0 {
            return refuse("it divides by zero");
        }
        self.mul(Ratio {
            num: other.den,
            den: other.num,
        })
        .and_then(|r| Ratio::new(r.num, r.den))
    }

    /// The nearest integer, halves away from zero.
    fn round(self) -> Result<i64, ExprError> {
        let half_up = (self.num.abs() * 2 + self.den) / (self.den * 2);
        let n = if self.num < 0 { -half_up } else { half_up };
        i64::try_from(n).map_err(|_| too_large())
    }
}

// ---------------------------------------------------------------------------------------
// Units.

/// Length units, in nanometres.
const LENGTH_UNITS: &[(&str, i128)] = &[
    ("nm", 1),
    ("um", 1_000),
    ("µm", 1_000),
    ("μm", 1_000),
    ("mm", 1_000_000),
    ("mil", 25_400),
    ("in", 25_400_000),
];

/// Angle units, in millidegrees.
const ANGLE_UNITS: &[(&str, i128)] = &[("deg", 1_000)];

fn unit(name: &str) -> Option<(Dim, i128)> {
    let find = |table: &[(&str, i128)]| {
        table
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, f)| *f)
    };
    find(LENGTH_UNITS)
        .map(|f| (Dim::Length, f))
        .or_else(|| find(ANGLE_UNITS).map(|f| (Dim::Angle, f)))
}

fn unit_names() -> String {
    LENGTH_UNITS
        .iter()
        .chain(ANGLE_UNITS)
        .map(|(n, _)| *n)
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------------------
// Parsing.

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    /// A number as typed: `value` in `unit`, or bare.
    Number(Ratio, Option<(Dim, i128)>),
    Current,
    Attribute(String),
    Neg(Box<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(Ratio, Option<(Dim, i128)>),
    Bang,
    Name(String),
    Op(char),
    Open,
    Close,
}

fn describe(token: Option<&Token>) -> String {
    match token {
        None => "the end".into(),
        Some(Token::Number(r, _)) if r.den == 1 => format!("`{}`", r.num),
        Some(Token::Number(..)) => "a number".into(),
        Some(Token::Bang) => "`!`".into(),
        Some(Token::Name(n)) => format!("`{n}`"),
        Some(Token::Op(c)) => format!("`{c}`"),
        Some(Token::Open) => "`(`".into(),
        Some(Token::Close) => "`)`".into(),
    }
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == 'µ'
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

fn tokens(text: &str) -> Result<Vec<Token>, ExprError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let digits: String = chars[start..i].iter().collect();
            let value = number(&digits)?;
            // A unit right after the number, or after spaces when the word is a unit.
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let word_end = |mut k: usize| {
                while k < chars.len() && is_name_char(chars[k]) {
                    k += 1;
                }
                k
            };
            let end = word_end(j);
            let word: String = chars[j..end].iter().collect();
            let unit = if word.is_empty() {
                None
            } else if let Some(u) = unit(&word) {
                i = end;
                Some(u)
            } else if j == i {
                return refuse(format!(
                    "unknown unit `{word}` after {digits}; use one of {}",
                    unit_names()
                ));
            } else {
                None
            };
            out.push(Token::Number(value, unit));
        } else if is_name_start(c) {
            let start = i;
            while i < chars.len() && is_name_char(chars[i]) {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            if is_reserved(&name) {
                return refuse(format!(
                    "`{name}` is a reserved function name; functions are not supported yet"
                ));
            }
            out.push(Token::Name(name));
        } else {
            out.push(match c {
                '!' => Token::Bang,
                '+' | '-' | '*' | '/' => Token::Op(c),
                '(' => Token::Open,
                ')' => Token::Close,
                other => {
                    return refuse(format!(
                        "`{other}` is not allowed; use numbers with units, `!`, attribute \
                         names, + - * / and brackets"
                    ));
                }
            });
            i += 1;
        }
    }
    Ok(out)
}

/// A decimal such as `1.25` or `.5`, exactly.
fn number(digits: &str) -> Result<Ratio, ExprError> {
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if fraction.contains('.') || whole.len() + fraction.len() == 0 {
        return refuse(format!("`{digits}` is not a number"));
    }
    let mut mantissa: i128 = 0;
    for b in whole.bytes().chain(fraction.bytes()) {
        mantissa = mantissa
            .checked_mul(10)
            .and_then(|m| m.checked_add(i128::from(b - b'0')))
            .ok_or_else(too_large)?;
    }
    let den = u32::try_from(fraction.len())
        .ok()
        .and_then(|n| 10i128.checked_pow(n))
        .ok_or_else(too_large)?;
    Ratio::new(mantissa, den)
}

fn parse(text: &str) -> Result<Expr, ExprError> {
    let tokens = tokens(text)?;
    if tokens.is_empty() {
        return refuse("the field is empty; type a value");
    }
    let mut parser = Parser { tokens, at: 0 };
    let expr = parser.sum()?;
    match parser.peek() {
        None => Ok(expr),
        Some(Token::Close) => refuse("there is a `)` without a matching `(`"),
        other => refuse(format!("expected an operator before {}", describe(other))),
    }
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.at).cloned();
        self.at += 1;
        t
    }

    fn sum(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.product()?;
        while let Some(Token::Op(c @ ('+' | '-'))) = self.peek() {
            let op = if *c == '+' { Op::Add } else { Op::Sub };
            self.at += 1;
            let right = self.product()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn product(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.unary()?;
        while let Some(Token::Op(c @ ('*' | '/'))) = self.peek() {
            let op = if *c == '*' { Op::Mul } else { Op::Div };
            self.at += 1;
            let right = self.unary()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        match self.peek() {
            Some(Token::Op('+')) => {
                self.at += 1;
                self.unary()
            }
            Some(Token::Op('-')) => {
                self.at += 1;
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, ExprError> {
        match self.next() {
            Some(Token::Number(value, unit)) => Ok(Expr::Number(value, unit)),
            Some(Token::Bang) => Ok(Expr::Current),
            Some(Token::Name(name)) => {
                if self.peek() == Some(&Token::Open) {
                    return refuse(format!(
                        "`{name}(…)` looks like a function call; functions are not \
                         supported yet"
                    ));
                }
                Ok(Expr::Attribute(name))
            }
            Some(Token::Open) => {
                let inner = self.sum()?;
                match self.next() {
                    Some(Token::Close) => Ok(inner),
                    other => refuse(format!(
                        "expected `)` to close a bracket, found {}",
                        describe(other.as_ref())
                    )),
                }
            }
            other => refuse(format!(
                "expected a value, found {}",
                describe(other.as_ref())
            )),
        }
    }
}

fn is_reserved(name: &str) -> bool {
    RESERVED_FUNCTIONS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(name))
}

// ---------------------------------------------------------------------------------------
// Evaluation.

/// What a value measures. `Bare` is a number typed without units: it takes the display
/// units when added to a length or angle, or as the result, and is a factor otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dim {
    Bare,
    Number,
    Length,
    Angle,
}

impl Dim {
    fn name(self) -> &'static str {
        match self {
            Dim::Bare | Dim::Number => "a plain number",
            Dim::Length => "a length",
            Dim::Angle => "an angle",
        }
    }

    fn of(kind: Kind) -> Dim {
        match kind {
            Kind::Length => Dim::Length,
            Kind::Angle => Dim::Angle,
        }
    }
}

/// A value in base units: nanometres, millidegrees, or as typed for numbers.
#[derive(Clone, Copy, Debug)]
struct Value {
    dim: Dim,
    value: Ratio,
}

fn resolve(expr: &Expr, field: Field, object: &Object) -> Result<i64, ExprError> {
    let want = Dim::of(field.kind);
    let result = eval(expr, field, object)?;
    let result = match result.dim {
        Dim::Bare => scale(result, want, field)?,
        dim if dim == want => result,
        Dim::Number => {
            return refuse(format!(
                "the result is a plain number; this field needs {}",
                want.name()
            ));
        }
        dim => {
            return refuse(format!(
                "the result is {}; this field needs {}",
                dim.name(),
                want.name()
            ));
        }
    };
    result.value.round()
}

/// A bare number as `dim`, in the field's display unit when the field holds `dim`, else
/// in mm or degrees.
fn scale(bare: Value, dim: Dim, field: Field) -> Result<Value, ExprError> {
    let factor = if dim == Dim::of(field.kind) {
        match unit(field.unit) {
            Some((d, f)) if d == dim => f,
            _ => return refuse(format!("`{}` is not a display unit", field.unit)),
        }
    } else {
        match dim {
            Dim::Length => 1_000_000,
            Dim::Angle => 1_000,
            Dim::Bare | Dim::Number => 1,
        }
    };
    Ok(Value {
        dim,
        value: bare.value.mul(Ratio::int(factor))?,
    })
}

fn eval(expr: &Expr, field: Field, object: &Object) -> Result<Value, ExprError> {
    match expr {
        Expr::Number(value, None) => Ok(Value {
            dim: Dim::Bare,
            value: *value,
        }),
        Expr::Number(value, Some((dim, factor))) => Ok(Value {
            dim: *dim,
            value: value.mul(Ratio::int(*factor))?,
        }),
        Expr::Current => match object.current {
            Some(v) => Ok(Value {
                dim: Dim::of(field.kind),
                value: Ratio::int(i128::from(v)),
            }),
            None => refuse("`!` has no value here: this object has no value of its own"),
        },
        Expr::Attribute(name) => attribute(name, object),
        Expr::Neg(inner) => {
            let v = eval(inner, field, object)?;
            Ok(Value {
                dim: v.dim,
                value: v.value.neg(),
            })
        }
        Expr::Binary(op, a, b) => {
            let a = eval(a, field, object)?;
            let b = eval(b, field, object)?;
            binary(*op, a, b, field)
        }
    }
}

fn attribute(name: &str, object: &Object) -> Result<Value, ExprError> {
    let found = object
        .attributes
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name));
    let Some((_, attr)) = found else {
        let names: Vec<&str> = object.attributes.iter().map(|(n, _)| *n).collect();
        return refuse(if names.is_empty() {
            format!("unknown attribute `{name}`; this object has none to use")
        } else {
            format!(
                "unknown attribute `{name}`; this object has {}",
                names.join(", ")
            )
        });
    };
    let (dim, v) = match *attr {
        Attr::Length(nm) => (Dim::Length, nm.0),
        Attr::Angle(md) => (Dim::Angle, md.0),
        Attr::Number(n) => (Dim::Number, n),
    };
    Ok(Value {
        dim,
        value: Ratio::int(i128::from(v)),
    })
}

fn binary(op: Op, a: Value, b: Value, field: Field) -> Result<Value, ExprError> {
    use Dim::{Angle, Bare, Length, Number};
    let with = |dim, value| Ok(Value { dim, value });
    match op {
        Op::Add | Op::Sub => {
            let (a, b) = match (a.dim, b.dim) {
                (x, y) if x == y => (a, b),
                (Bare, Number) | (Number, Bare) => {
                    (Value { dim: Number, ..a }, Value { dim: Number, ..b })
                }
                (Bare, d @ (Length | Angle)) => (scale(a, d, field)?, b),
                (d @ (Length | Angle), Bare) => (a, scale(b, d, field)?),
                (x, y) => {
                    return refuse(format!(
                        "can't {} {} and {}",
                        if op == Op::Add { "add" } else { "subtract" },
                        x.name(),
                        y.name()
                    ));
                }
            };
            let b = if op == Op::Sub {
                b.value.neg()
            } else {
                b.value
            };
            with(a.dim, a.value.add(b)?)
        }
        Op::Mul => {
            let dim = match (a.dim, b.dim) {
                (Bare, Bare) => Bare,
                (Bare | Number, Bare | Number) => Number,
                (Bare | Number, d) | (d, Bare | Number) => d,
                (x, y) => {
                    let why = if x == Length && y == Length {
                        " (that would be an area)".to_owned()
                    } else {
                        String::new()
                    };
                    return refuse(format!("can't multiply {} by {}{why}", x.name(), y.name()));
                }
            };
            with(dim, a.value.mul(b.value)?)
        }
        Op::Div => {
            let dim = match (a.dim, b.dim) {
                (Bare, Bare) => Bare,
                (Bare | Number, Bare | Number) => Number,
                (d, Bare | Number) => d,
                (x, y) if x == y => Number,
                (x, y) => {
                    return refuse(format!("can't divide {} by {}", x.name(), y.name()));
                }
            };
            with(dim, a.value.div(b.value)?)
        }
    }
}
