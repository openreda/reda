//! UUIDs: the identity of every persisted object.
//!
//! Written in-house rather than taken from a crate: REDA needs only formatting, parsing and
//! the v4 bit layout. Generating them is the job of the engine's injected `IdSource`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A 128-bit identifier, written as a lowercase hyphenated string.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Uuid(u128);

impl Uuid {
    pub const NIL: Uuid = Uuid(0);

    pub const fn from_u128(value: u128) -> Uuid {
        Uuid(value)
    }

    pub const fn as_u128(self) -> u128 {
        self.0
    }

    /// A version-4 UUID from 128 bits of entropy: the version and variant bits are forced,
    /// the remaining 122 bits are taken from `bits`.
    pub const fn v4_from_bits(bits: u128) -> Uuid {
        const VERSION_MASK: u128 = 0xf << 76;
        const VARIANT_MASK: u128 = 0x3 << 62;
        let bits = (bits & !VERSION_MASK) | (0x4 << 76);
        Uuid((bits & !VARIANT_MASK) | (0x2 << 62))
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = format!("{:032x}", self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// A string that is not a hyphenated 8-4-4-4-12 hex UUID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseUuidError(String);

impl fmt::Display for ParseUuidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} is not a hyphenated UUID", self.0)
    }
}

impl std::error::Error for ParseUuidError {}

impl FromStr for Uuid {
    type Err = ParseUuidError;

    /// Accepts either case, so hand edits load; writing normalises to lowercase.
    fn from_str(s: &str) -> Result<Uuid, ParseUuidError> {
        let err = || ParseUuidError(s.to_owned());
        let groups: Vec<&str> = s.split('-').collect();
        let lengths: Vec<usize> = groups.iter().map(|g| g.len()).collect();
        if lengths != [8, 4, 4, 4, 12] {
            return Err(err());
        }
        let mut value: u128 = 0;
        for c in groups.concat().chars() {
            value = (value << 4) | u128::from(c.to_digit(16).ok_or_else(err)?);
        }
        Ok(Uuid(value))
    }
}

impl Serialize for Uuid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl schemars::JsonSchema for Uuid {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Uuid".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "format": "uuid" })
    }
}

impl<'de> Deserialize<'de> for Uuid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Uuid, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
