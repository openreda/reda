//! Lengths and areas that keep the unit they were typed in
//! (docs/spec/v0.3/input-and-defaults.md#the-defaults-page): an Object Default typed as
//! `"10mil"` is stored, written and shown as 10 mil, never as 254000 nm or 0.254 mm.
//! [`TypedLength`] lives in `reda-model`, since Board Grids store it too.
//!
//! The file holds `"<number><unit>"`, such as `"0.15mm"`; a bare integer is nanometres
//! (square nanometres for an area) with no unit of its own, which a front end shows in
//! its default units. Two values of the same length in different units are different
//! values: a Preference written in mm where the shipped value is in mil stays in the file.

use std::borrow::Cow;
use std::fmt;

use reda_command::AreaValue;
pub use reda_model::typed_length::{LengthUnit, TypedLength};
use reda_model::typed_length::{decimal, split_unit};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// An area with the unit it was typed in (`mm2`, `mil2`, ...), if it was typed with one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TypedArea {
    nm2: i64,
    unit: Option<LengthUnit>,
}

impl TypedArea {
    /// `nm2` square nanometres typed in square `unit`s.
    pub const fn new(nm2: i64, unit: LengthUnit) -> TypedArea {
        TypedArea {
            nm2,
            unit: Some(unit),
        }
    }

    /// Square nanometres.
    pub const fn nm2(self) -> i64 {
        self.nm2
    }

    /// The length unit squared it was typed in; `None` for bare square nanometres.
    pub const fn unit(self) -> Option<LengthUnit> {
        self.unit
    }

    /// Reads `"0.5mm2"`, `"0.5 mm²"`, `"100sqmil"` or a bare number of square nanometres.
    pub fn parse(text: &str) -> Result<TypedArea, String> {
        let t = text.trim();
        let (number, unit) = split_unit(t);
        if unit.is_empty() {
            return number
                .parse::<i64>()
                .map(|nm2| TypedArea { nm2, unit: None })
                .map_err(|_| format!("`{text}` needs an area unit such as mm2"));
        }
        let length = unit
            .strip_prefix("sq")
            .or_else(|| unit.strip_suffix('2'))
            .or_else(|| unit.strip_suffix('²'))
            .and_then(LengthUnit::of)
            .ok_or_else(|| format!("`{text}` has an unknown area unit"))?;
        let nm2 = AreaValue::parse(&format!("{number}{}2", length.symbol()))?.0;
        Ok(TypedArea {
            nm2,
            unit: Some(length),
        })
    }

    /// The value in its own unit, as the file writes it (`0.5mm2`); `None` without one.
    pub fn text(self) -> Option<String> {
        self.number().map(|(n, u)| format!("{n}{u}2"))
    }

    /// The value in its own unit as a person reads it (`0.5 mm²`); `None` without one.
    pub fn shown(self) -> Option<String> {
        self.number().map(|(n, u)| format!("{n} {u}²"))
    }

    fn number(self) -> Option<(String, &'static str)> {
        let unit = self.unit?;
        let per = i128::from(unit.nm()) * i128::from(unit.nm());
        Some((decimal(i128::from(self.nm2), per), unit.symbol()))
    }
}

impl Serialize for TypedArea {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.text() {
            Some(text) => serializer.serialize_str(&text),
            None => self.nm2.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TypedArea {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TypedArea, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = TypedArea;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("integer square nanometres or an area string such as \"0.5mm2\"")
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<TypedArea, E> {
                Ok(TypedArea { nm2: v, unit: None })
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<TypedArea, E> {
                i64::try_from(v)
                    .map(|nm2| TypedArea { nm2, unit: None })
                    .map_err(|_| E::custom("the value is too large"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<TypedArea, E> {
                TypedArea::parse(v).map_err(E::custom)
            }
        }
        deserializer.deserialize_any(V)
    }
}

impl JsonSchema for TypedArea {
    fn schema_name() -> Cow<'static, str> {
        "TypedArea".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An area that keeps the unit it was typed in: a number and an area unit (nm2, um2, mm2, mil2, in2, also written with ², or sqmil) such as \"0.5mm2\", or integer square nanometres.",
            "oneOf": [
                { "type": "integer", "description": "Square nanometres, with no unit of its own." },
                { "type": "string", "format": "area", "description": "A number and an area unit." }
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_area_reads_and_writes_in_its_own_unit() {
        let a = TypedArea::parse("0.5 mm²").unwrap();
        assert_eq!(a.nm2(), 500_000_000_000);
        assert_eq!(a.text().as_deref(), Some("0.5mm2"));
        assert_eq!(a.shown().as_deref(), Some("0.5 mm²"));
        let a = TypedArea::parse("100sqmil").unwrap();
        assert_eq!(a.nm2(), 64_516_000_000);
        assert_eq!(a.text().as_deref(), Some("100mil2"));
        assert_eq!(TypedArea::parse("0").unwrap().unit(), None);
        assert!(TypedArea::parse("2 mm").is_err());
    }
}
