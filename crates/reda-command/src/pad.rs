//! Params every Pad Command shares, Footprint Pads and Free Pads alike: corner and chamfer
//! ratios, the hole and the Zone connection.

use std::borrow::Cow;
use std::fmt;

use reda_model::geom::Millideg;
use reda_model::{ConnectionStyle, Hole, HoleTolerance, Ratio, ZoneConnection};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use crate::{Angle, Length};

/// A ratio param: integer thousandths of a percent of half the Pad's shorter side, or a
/// percentage string such as `"25%"`. 100% is fully round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RatioParam(pub Ratio);

impl RatioParam {
    pub const fn ratio(self) -> Ratio {
        self.0
    }
}

/// `"25%"` or `"12.5%"` as a [`Ratio`].
fn parse_percent(text: &str) -> Result<Ratio, String> {
    let bad = || format!("{text:?} is not a percentage such as \"25%\"");
    let number = text.trim().strip_suffix('%').ok_or_else(bad)?.trim();
    let (whole, frac) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty()
        || frac.len() > 3
        || !(whole.chars().chain(frac.chars())).all(|c| c.is_ascii_digit())
    {
        return Err(bad());
    }
    let whole: u32 = whole.parse().map_err(|_| bad())?;
    let frac: u32 = format!("{frac:0<3}").parse().map_err(|_| bad())?;
    whole
        .checked_mul(1000)
        .and_then(|w| w.checked_add(frac))
        .map(Ratio)
        .ok_or_else(bad)
}

impl<'de> Deserialize<'de> for RatioParam {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<RatioParam, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = RatioParam;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("integer thousandths of a percent, or a percentage such as \"25%\"")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<RatioParam, E> {
                u32::try_from(v)
                    .map(|v| RatioParam(Ratio(v)))
                    .map_err(|_| E::custom("the ratio is too large"))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<RatioParam, E> {
                u32::try_from(v)
                    .map(|v| RatioParam(Ratio(v)))
                    .map_err(|_| E::custom("a ratio cannot be negative"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<RatioParam, E> {
                parse_percent(v).map(RatioParam).map_err(E::custom)
            }
        }
        deserializer.deserialize_any(V)
    }
}

impl Serialize for RatioParam {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl JsonSchema for RatioParam {
    fn schema_name() -> Cow<'static, str> {
        "RatioParam".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A share of half the Pad's shorter side, 100% being fully round: integer thousandths of a percent, or a percentage such as \"25%\".",
            "oneOf": [
                { "type": "integer", "minimum": 0, "maximum": 100000, "description": "Thousandths of a percent." },
                { "type": "string", "pattern": "^\\s*[0-9]+(\\.[0-9]{1,3})?\\s*%$", "description": "A percentage." }
            ]
        })
    }
}

/// A drilled hole param.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HoleParam {
    /// The finished diameter, or a slot's width.
    pub width: Length,
    /// Left out, the same as `width`: a round hole. Otherwise a slot along the longer side.
    #[serde(default)]
    pub height: Option<Length>,
    /// Turns a slot in the Pad's frame. Left out, 0.
    #[serde(default)]
    pub rotation: Option<Angle>,
    /// How far the finished hole may come out larger or smaller. Stored and shown only.
    #[serde(default)]
    pub tolerance: Option<HoleToleranceParam>,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HoleToleranceParam {
    pub plus: Length,
    pub minus: Length,
}

impl HoleParam {
    pub fn hole(self) -> Hole {
        Hole {
            width: self.width.nm(),
            height: self.height.unwrap_or(self.width).nm(),
            rotation: self
                .rotation
                .map_or(Millideg::ZERO, |r| normalized_half_turn(r.millideg())),
            tolerance: self.tolerance.map(|t| HoleTolerance {
                plus: t.plus.nm(),
                minus: t.minus.nm(),
            }),
        }
    }
}

/// A slot's direction in `[0, 180°)`: a slot turned half a turn is the same slot.
fn normalized_half_turn(angle: Millideg) -> Millideg {
    Millideg(angle.0.rem_euclid(180_000))
}

/// A Zone connection param. Each value left out inherits from the `zone_connection` rule.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneConnectionParam {
    /// `relief`, `solid` or `none`.
    #[serde(default)]
    pub style: Option<ConnectionStyle>,
    /// 2 or 4.
    #[serde(default)]
    pub spokes: Option<u8>,
    #[serde(default)]
    pub spoke_width: Option<Length>,
    /// The clear ring around a relieved or unconnected Pad.
    #[serde(default)]
    pub gap: Option<Length>,
    /// 0, 45 or 90 degrees in the Pad's frame.
    #[serde(default)]
    pub angle: Option<Angle>,
}

impl ZoneConnectionParam {
    pub fn connection(self) -> ZoneConnection {
        ZoneConnection {
            style: self.style,
            spokes: self.spokes,
            spoke_width: self.spoke_width.map(Length::nm),
            gap: self.gap.map(Length::nm),
            angle: self.angle.map(|a| a.millideg().normalized()),
        }
    }
}

/// `None` when every value is left out, so an empty connection isn't stored.
pub fn zone_connection(param: Option<ZoneConnectionParam>) -> Option<ZoneConnection> {
    param
        .map(ZoneConnectionParam::connection)
        .filter(|c| *c != ZoneConnection::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentages_parse_to_thousandths() {
        assert_eq!(parse_percent("25%"), Ok(Ratio(25_000)));
        assert_eq!(parse_percent("12.5 %"), Ok(Ratio(12_500)));
        assert!(parse_percent("25").is_err());
        assert!(parse_percent("-3%").is_err());
    }
}

/// Mask overrides accept the same length syntax as every Command.
pub type MaskParam = reda_model::Mask<Length>;
/// Paste windows are in the Pad's frame.
pub type PasteParam = reda_model::Paste<Length, OpeningPointParam, RatioParam>;
pub fn mask(param: Option<MaskParam>) -> Option<reda_model::Mask> {
    param.map(|m| m.map(Length::nm))
}
pub fn paste(param: Option<PasteParam>) -> Option<reda_model::Paste> {
    param.map(|p| p.map(Length::nm, OpeningPointParam::point, RatioParam::ratio))
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpeningPointParam {
    pub x: Length,
    pub y: Length,
}
impl OpeningPointParam {
    pub fn point(self) -> reda_model::geom::Point {
        reda_model::geom::Point::new(self.x.nm(), self.y.nm())
    }
}
