//! Lengths that keep the unit they were typed in: an Object Default typed as `"10mil"`
//! (docs/spec/v0.3/input-and-defaults.md#the-defaults-page) or a Board Grid's step
//! (docs/spec/v0.3/commands.md#grids-and-guides) is stored, written and shown as 10 mil,
//! never as 254000 nm or 0.254 mm.
//!
//! The text form is `"<number><unit>"`, such as `"0.15mm"`; a bare integer is nanometres
//! with no unit of its own, which a front end shows in its default units. Two values of
//! the same length in different units are different values.

use std::borrow::Cow;
use std::fmt;

use reda_geom::Nm;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// A unit a length can be typed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LengthUnit {
    Nm,
    Um,
    Mm,
    Mil,
    In,
}

impl LengthUnit {
    /// Every unit, smallest first.
    pub const ALL: [LengthUnit; 5] = [
        LengthUnit::Nm,
        LengthUnit::Um,
        LengthUnit::Mm,
        LengthUnit::Mil,
        LengthUnit::In,
    ];

    /// How many nanometres one of it is.
    pub const fn nm(self) -> i64 {
        match self {
            LengthUnit::Nm => 1,
            LengthUnit::Um => 1_000,
            LengthUnit::Mm => 1_000_000,
            LengthUnit::Mil => 25_400,
            LengthUnit::In => 25_400_000,
        }
    }

    /// The unit as the file writes it: `nm`, `um`, `mm`, `mil` or `in`.
    pub const fn symbol(self) -> &'static str {
        match self {
            LengthUnit::Nm => "nm",
            LengthUnit::Um => "um",
            LengthUnit::Mm => "mm",
            LengthUnit::Mil => "mil",
            LengthUnit::In => "in",
        }
    }

    /// The unit written as `text`; `µm` and `μm` are `um`.
    pub fn of(text: &str) -> Option<LengthUnit> {
        match text {
            "µm" | "μm" => Some(LengthUnit::Um),
            _ => LengthUnit::ALL.into_iter().find(|u| u.symbol() == text),
        }
    }
}

/// Splits `"0.15 mm"` into the number and the unit's text.
pub fn split_unit(text: &str) -> (&str, &str) {
    let t = text.trim();
    let at = t.find(|c: char| c.is_alphabetic()).unwrap_or(t.len());
    let (number, unit) = t.split_at(at);
    (number.trim(), unit.trim())
}

/// `value / per` as an exact decimal, such as `0.15`; at most nine places.
pub fn decimal(value: i128, per: i128) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    let mut text = format!("{sign}{}", value / per);
    let mut rest = value % per;
    if rest != 0 {
        text.push('.');
        for _ in 0..9 {
            if rest == 0 {
                break;
            }
            rest *= 10;
            text.push(char::from(b'0' + (rest / per) as u8));
            rest %= per;
        }
    }
    text
}

/// A length with the unit it was typed in, if it was typed with one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TypedLength {
    nm: Nm,
    unit: Option<LengthUnit>,
}

impl TypedLength {
    /// `nm` typed in `unit`.
    pub const fn new(nm: Nm, unit: LengthUnit) -> TypedLength {
        TypedLength {
            nm,
            unit: Some(unit),
        }
    }

    /// `nm` with no unit of its own.
    pub const fn bare(nm: Nm) -> TypedLength {
        TypedLength { nm, unit: None }
    }

    pub const fn nm(self) -> Nm {
        self.nm
    }

    /// The unit it was typed in; `None` for a bare number of nanometres.
    pub const fn unit(self) -> Option<LengthUnit> {
        self.unit
    }

    /// Reads `"0.15mm"`, `"0.15 mm"` or `"150000"` (nanometres, no unit). The value must
    /// be a whole number of nanometres.
    pub fn parse(text: &str) -> Result<TypedLength, String> {
        let nm = Nm::parse(&text.replace(' ', "")).map_err(|e| e.to_string())?;
        let (_, unit) = split_unit(text);
        Ok(TypedLength {
            nm,
            unit: LengthUnit::of(unit),
        })
    }

    /// The value in its own unit, as the file writes it (`0.15mm`); `None` without one.
    pub fn text(self) -> Option<String> {
        let unit = self.unit?;
        Some(format!(
            "{}{}",
            decimal(i128::from(self.nm.0), i128::from(unit.nm())),
            unit.symbol()
        ))
    }

    /// The value in its own unit as a person reads it (`0.15 mm`); `None` without one.
    pub fn shown(self) -> Option<String> {
        let unit = self.unit?;
        Some(format!(
            "{} {}",
            decimal(i128::from(self.nm.0), i128::from(unit.nm())),
            unit.symbol()
        ))
    }
}

impl From<Nm> for TypedLength {
    fn from(nm: Nm) -> TypedLength {
        TypedLength::bare(nm)
    }
}

impl fmt::Display for TypedLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.text() {
            Some(text) => f.write_str(&text),
            None => write!(f, "{}", self.nm.0),
        }
    }
}

impl Serialize for TypedLength {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.text() {
            Some(text) => serializer.serialize_str(&text),
            None => self.nm.0.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TypedLength {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TypedLength, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = TypedLength;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("integer nanometres or a length string such as \"1.27mm\"")
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<TypedLength, E> {
                Ok(TypedLength::bare(Nm(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<TypedLength, E> {
                i64::try_from(v)
                    .map(|v| TypedLength::bare(Nm(v)))
                    .map_err(|_| E::custom("the value is too large"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<TypedLength, E> {
                TypedLength::parse(v).map_err(E::custom)
            }
        }
        deserializer.deserialize_any(V)
    }
}

impl JsonSchema for TypedLength {
    fn schema_name() -> Cow<'static, str> {
        "TypedLength".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A length that keeps the unit it was typed in: a number and a unit (nm, um, mm, mil or in) such as \"0.15mm\" or \"10mil\", or integer nanometres.",
            "oneOf": [
                { "type": "integer", "description": "Nanometres, with no unit of its own." },
                { "type": "string", "format": "length", "description": "A number and a unit: nm, um, mm, mil or in." }
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_length_reads_and_writes_in_its_own_unit() {
        for (typed, nm, text, shown) in [
            ("0.15mm", 150_000, "0.15mm", "0.15 mm"),
            ("10 mil", 254_000, "10mil", "10 mil"),
            ("2.54um", 2_540, "2.54um", "2.54 um"),
            ("2µm", 2_000, "2um", "2 um"),
            ("0.5in", 12_700_000, "0.5in", "0.5 in"),
            ("-0.25mm", -250_000, "-0.25mm", "-0.25 mm"),
            ("1.0mm", 1_000_000, "1mm", "1 mm"),
        ] {
            let l = TypedLength::parse(typed).unwrap();
            assert_eq!(l.nm(), Nm(nm), "{typed}");
            assert_eq!(l.text().as_deref(), Some(text), "{typed}");
            assert_eq!(l.shown().as_deref(), Some(shown), "{typed}");
            assert_eq!(TypedLength::parse(text).unwrap(), l);
        }
        let bare = TypedLength::parse("7").unwrap();
        assert_eq!((bare.nm(), bare.unit()), (Nm(7), None));
        assert!(TypedLength::parse("0.1nm").is_err());
        assert!(TypedLength::parse("3 parsecs").is_err());
    }
}
