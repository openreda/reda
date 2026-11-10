//! Length and angle params: an integer in base units, or a unit string.
//!
//! Params use [`Length`] and [`Angle`]; results and stored data use [`Nm`] and [`Millideg`],
//! which are always bare integers.

use std::borrow::Cow;
use std::fmt;

use reda_model::geom::{Millideg, Nm};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// A length param: integer nanometres, or a string such as `"1.27mm"`, `"10mil"` or `"0.5in"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Length(pub Nm);

impl Length {
    pub const fn nm(self) -> Nm {
        self.0
    }
}

impl From<Nm> for Length {
    fn from(nm: Nm) -> Length {
        Length(nm)
    }
}

/// A copper thickness param: integer nanometres, a length string such as `"35um"`, or a
/// copper weight such as `"1oz"`, where 1 oz is exactly 35 µm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CopperThickness(pub Nm);

impl CopperThickness {
    pub const fn nm(self) -> Nm {
        self.0
    }
}

/// An angle param: integer millidegrees, or a string such as `"45deg"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Angle(pub Millideg);

impl Angle {
    pub const fn millideg(self) -> Millideg {
        self.0
    }
}

impl From<Millideg> for Angle {
    fn from(angle: Millideg) -> Angle {
        Angle(angle)
    }
}

/// An area param: integer square nanometres, or a string such as `"0.0645mm2"`,
/// `"0.0645mm²"` or `"100sqmil"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AreaValue(pub i64);

impl AreaValue {
    /// Parses a number and an area unit: `nm2`, `um2`, `mm2`, `mil2`, `in2` (each also with
    /// `²`, and `µm`), or `sqmil`.
    pub fn parse(text: &str) -> Result<AreaValue, String> {
        let t = text.trim();
        let split = t
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
            .ok_or_else(|| format!("`{text}` needs an area unit such as mm2"))?;
        let (number, unit) = t.split_at(split);
        let number: f64 = number
            .parse()
            .map_err(|_| format!("`{text}` does not start with a number"))?;
        let unit = unit.trim();
        let length = if let Some(u) = unit.strip_prefix("sq") {
            u
        } else if let Some(u) = unit.strip_suffix('2').or_else(|| unit.strip_suffix('²')) {
            u
        } else {
            return Err(format!("`{text}` needs an area unit such as mm2"));
        };
        let side = Nm::parse(&format!("1{length}"))
            .map_err(|_| format!("`{text}` has an unknown area unit"))?
            .0 as f64;
        let nm2 = number * side * side;
        if !nm2.is_finite() || nm2.abs() > i64::MAX as f64 {
            return Err(format!("`{text}` is too large"));
        }
        Ok(AreaValue(nm2.round() as i64))
    }
}

/// Accepts an integer, or a string handed to `parse`.
struct UnitVisitor<T> {
    expecting: &'static str,
    parse: fn(&str) -> Result<T, reda_model::geom::UnitError>,
    from_int: fn(i64) -> T,
}

impl<T> Visitor<'_> for UnitVisitor<T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.expecting)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<T, E> {
        Ok((self.from_int)(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<T, E> {
        let v = i64::try_from(v).map_err(|_| E::custom("the value is too large"))?;
        Ok((self.from_int)(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<T, E> {
        (self.parse)(v).map_err(E::custom)
    }
}

impl<'de> Deserialize<'de> for Length {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Length, D::Error> {
        deserializer.deserialize_any(UnitVisitor {
            expecting: "integer nanometres or a length string such as \"1.27mm\"",
            parse: |s| Nm::parse(s).map(Length),
            from_int: |v| Length(Nm(v)),
        })
    }
}

impl<'de> Deserialize<'de> for AreaValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<AreaValue, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = AreaValue;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("integer square nanometres or an area string such as \"0.0645mm2\"")
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<AreaValue, E> {
                Ok(AreaValue(v))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<AreaValue, E> {
                i64::try_from(v)
                    .map(AreaValue)
                    .map_err(|_| E::custom("the value is too large"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<AreaValue, E> {
                AreaValue::parse(v).map_err(E::custom)
            }
        }
        deserializer.deserialize_any(V)
    }
}

impl Serialize for AreaValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl JsonSchema for AreaValue {
    fn schema_name() -> Cow<'static, str> {
        "AreaValue".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An area: integer square nanometres, or a number and an area unit (nm2, um2, mm2, mil2, in2, also written with ², or sqmil) such as \"0.0645mm2\" or \"100sqmil\".",
            "oneOf": [
                { "type": "integer", "description": "Square nanometres." },
                { "type": "string", "format": AREA_FORMAT, "description": "A number and an area unit." }
            ]
        })
    }
}

impl<'de> Deserialize<'de> for CopperThickness {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CopperThickness, D::Error> {
        deserializer.deserialize_any(UnitVisitor {
            expecting: "integer nanometres, a length string such as \"35um\" or a copper \
                        weight such as \"1oz\"",
            parse: |s| Nm::parse_copper(s).map(CopperThickness),
            from_int: |v| CopperThickness(Nm(v)),
        })
    }
}

impl<'de> Deserialize<'de> for Angle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Angle, D::Error> {
        deserializer.deserialize_any(UnitVisitor {
            expecting: "integer millidegrees or an angle string such as \"45deg\"",
            parse: |s| Millideg::parse(s).map(Angle),
            from_int: |v| Angle(Millideg(v)),
        })
    }
}

/// Params are written back (by the Journal, for example) as plain integers.
impl Serialize for Length {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl Serialize for CopperThickness {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl Serialize for Angle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// The JSON Schema `format` the registry checks with [`Nm::parse`].
pub(crate) const LENGTH_FORMAT: &str = "length";
/// The JSON Schema `format` the registry checks with [`Nm::parse_copper`].
pub(crate) const COPPER_FORMAT: &str = "copper_thickness";
/// The JSON Schema `format` the registry checks with [`AreaValue::parse`].
pub(crate) const AREA_FORMAT: &str = "area";
/// The JSON Schema `format` the registry checks with [`Millideg::parse`].
pub(crate) const ANGLE_FORMAT: &str = "angle";

impl JsonSchema for Length {
    fn schema_name() -> Cow<'static, str> {
        "Length".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A length: integer nanometres, or a number and a unit (nm, um, mm, mil or in) such as \"1.27mm\", \"10mil\" or \"0.5in\".",
            "oneOf": [
                { "type": "integer", "description": "Nanometres." },
                { "type": "string", "format": LENGTH_FORMAT, "description": "A number and a unit: nm, um, mm, mil or in." }
            ]
        })
    }
}

impl JsonSchema for CopperThickness {
    fn schema_name() -> Cow<'static, str> {
        "CopperThickness".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A copper thickness: integer nanometres, a length such as \"35um\", or a copper weight in ounces such as \"1oz\", where 1 oz is exactly 35 µm.",
            "oneOf": [
                { "type": "integer", "description": "Nanometres." },
                { "type": "string", "format": COPPER_FORMAT, "description": "A number and a unit: nm, um, µm, mm, mil, in or oz." }
            ]
        })
    }
}

impl JsonSchema for Angle {
    fn schema_name() -> Cow<'static, str> {
        "Angle".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An angle, counter-clockwise: integer millidegrees, or degrees such as \"45deg\".",
            "oneOf": [
                { "type": "integer", "description": "Millidegrees." },
                { "type": "string", "format": ANGLE_FORMAT, "description": "A number of degrees followed by deg." }
            ]
        })
    }
}
