//! The field types of records and how each reads from and writes to a Part 21 parameter.

use super::RecordId;
use crate::part21::{Param, encode_real, encode_string};

/// One record field: read from a resolved parameter, written back, references walked.
pub(crate) trait Field {
    fn read(p: &Param) -> Option<Self>
    where
        Self: Sized;
    fn write(&self, out: &mut String);
    fn refs(&self, _f: &mut dyn FnMut(RecordId)) {}
    fn refs_mut(&mut self, _f: &mut dyn FnMut(&mut RecordId)) {}
}

/// Text. An unset `$` reads as the empty string, which is written back as `''`.
impl Field for String {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::String(s) => Some(s.clone()),
            Param::Unset => Some(String::new()),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        encode_string(self, out);
    }
}

impl Field for f64 {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Real(r) => Some(*r),
            Param::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        encode_real(*self, out);
    }
}

impl Field for i64 {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Integer(i) => Some(*i),
            Param::Real(r) if r.fract() == 0.0 && r.abs() < 1e15 => Some(*r as i64),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        out.push_str(&self.to_string());
    }
}

/// A BOOLEAN, `.T.` or `.F.`.
impl Field for bool {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Enum(e) if e == "T" || e == "TRUE" => Some(true),
            Param::Enum(e) if e == "F" || e == "FALSE" => Some(false),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        out.push_str(if *self { ".T." } else { ".F." });
    }
}

impl Field for RecordId {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Ref(r) => u32::try_from(*r).ok().map(RecordId),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        out.push('#');
        out.push_str(&(u64::from(self.0) + 1).to_string());
    }
    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        f(*self);
    }
    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        f(self);
    }
}

/// An OPTIONAL attribute: `$` is `None`.
impl<T: Field> Field for Option<T> {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Unset => Some(None),
            p => T::read(p).map(Some),
        }
    }
    fn write(&self, out: &mut String) {
        match self {
            Some(v) => v.write(out),
            None => out.push('$'),
        }
    }
    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        if let Some(v) = self {
            v.refs(f);
        }
    }
    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        if let Some(v) = self {
            v.refs_mut(f);
        }
    }
}

impl<T: Field> Field for Vec<T> {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::List(items) => items.iter().map(T::read).collect(),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        out.push('(');
        for (i, v) in self.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            v.write(out);
        }
        out.push(')');
    }
    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        self.iter().for_each(|v| v.refs(f));
    }
    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        self.iter_mut().for_each(|v| v.refs_mut(f));
    }
}

/// An attribute a subtype redeclares as derived, written `*` (the ends of an
/// [`OrientedEdge`](super::OrientedEdge), the dimensions of an SI unit). Whatever the file
/// holds there is ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Derived;

impl Field for Derived {
    fn read(_: &Param) -> Option<Self> {
        Some(Derived)
    }
    fn write(&self, out: &mut String) {
        out.push('*');
    }
}

/// An enumeration value, upper case, without its dots (`MILLI`, `UNSPECIFIED`, `POSITIVE`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EnumValue(pub String);

impl EnumValue {
    pub fn new(s: &str) -> Self {
        EnumValue(s.to_string())
    }

    pub fn is(&self, s: &str) -> bool {
        self.0 == s
    }
}

impl Field for EnumValue {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Enum(e) => Some(EnumValue(e.clone())),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        out.push('.');
        out.push_str(&self.0);
        out.push('.');
    }
}

/// A LOGICAL: true, false or unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Logical {
    False,
    True,
    Unknown,
}

impl Field for Logical {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Enum(e) if e == "T" || e == "TRUE" => Some(Logical::True),
            Param::Enum(e) if e == "F" || e == "FALSE" => Some(Logical::False),
            Param::Enum(e) if e == "U" || e == "UNKNOWN" => Some(Logical::Unknown),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        out.push_str(match self {
            Logical::False => ".F.",
            Logical::True => ".T.",
            Logical::Unknown => ".U.",
        });
    }
}

/// A measure value, usually typed: `LENGTH_MEASURE(25.4)` has kind `LENGTH_MEASURE`.
#[derive(Clone, Debug, PartialEq)]
pub struct Measure {
    /// The measure type, or `None` for a bare number.
    pub kind: Option<String>,
    pub value: f64,
}

impl Measure {
    pub fn new(kind: &str, value: f64) -> Self {
        Measure {
            kind: Some(kind.to_string()),
            value,
        }
    }
}

impl Field for Measure {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Typed(kind, inner) => Some(Measure {
                kind: Some(kind.clone()),
                value: f64::read(inner)?,
            }),
            p => Some(Measure {
                kind: None,
                value: f64::read(p)?,
            }),
        }
    }
    fn write(&self, out: &mut String) {
        match &self.kind {
            Some(kind) => {
                out.push_str(kind);
                out.push('(');
                encode_real(self.value, out);
                out.push(')');
            }
            None => encode_real(self.value, out),
        }
    }
}

/// One trimming value of a [`TrimmedCurve`](super::TrimmedCurve): a point, or a parameter
/// on the basis curve written as `PARAMETER_VALUE(t)`.
#[derive(Clone, Debug, PartialEq)]
pub enum Trim {
    Point(RecordId),
    Parameter(f64),
}

impl Field for Trim {
    fn read(p: &Param) -> Option<Self> {
        match p {
            Param::Ref(_) => RecordId::read(p).map(Trim::Point),
            Param::Typed(kind, inner) if kind == "PARAMETER_VALUE" => {
                f64::read(inner).map(Trim::Parameter)
            }
            Param::Real(_) | Param::Integer(_) => f64::read(p).map(Trim::Parameter),
            _ => None,
        }
    }
    fn write(&self, out: &mut String) {
        match self {
            Trim::Point(id) => id.write(out),
            Trim::Parameter(t) => {
                out.push_str("PARAMETER_VALUE(");
                encode_real(*t, out);
                out.push(')');
            }
        }
    }
    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        if let Trim::Point(id) = self {
            f(*id);
        }
    }
    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        if let Trim::Point(id) = self {
            f(id);
        }
    }
}
