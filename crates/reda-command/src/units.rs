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

impl Serialize for Angle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// The JSON Schema `format` the registry checks with [`Nm::parse`].
pub(crate) const LENGTH_FORMAT: &str = "length";
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
