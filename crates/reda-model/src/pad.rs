//! The Pad model, shared by Footprint Pads and Free Pads: kind, side, shape, ratios and
//! corners, the copper's offset from the anchor, the hole, and the Zone connection.

use std::fmt;

use reda_geom::{Bounds, CornerCut, CornerRect, Millideg, Nm, Point};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Side, Uuid};

/// A copper area, with or without a hole, of a Footprint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pad {
    pub uuid: Uuid,
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    /// The Pad's anchor, relative to the Footprint origin: the hole sits there, Tracks
    /// connect there and the Pad turns about it.
    pub at: Point,
    pub rotation: Millideg,
    pub width: Nm,
    pub height: Nm,
    /// How round the corners of a `rounded_rect`, or the uncut corners of a
    /// `chamfered_rect`, are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_ratio: Option<Ratio>,
    /// How deep the chamfers of a `chamfered_rect` are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chamfer_ratio: Option<Ratio>,
    /// Which corners are rounded (`rounded_rect`) or chamfered (`chamfered_rect`), in the
    /// Pad's frame. Left out, all four.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corners: Option<Corners>,
    /// Where the copper sits relative to `at`, in the Pad's frame. Left out, on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<Point>,
    /// A round hole when width equals height, otherwise a slot. SMD Pads have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<Hole>,
    /// The side of an SMD Pad, in the Footprint's frame. Left out, the top. Pads with a
    /// hole have none: their copper is on every Layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// Per-side solder-mask expansion or tenting; omitted values inherit the board's rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<crate::Mask>,
    /// An SMD Pad's paste opening; omitted, it inherits the board's rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste: Option<crate::Paste>,
    /// How Zone fills of its Net join it. Each value left out inherits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_connection: Option<ZoneConnection>,
}

impl Pad {
    pub fn problem(&self) -> Option<(String, String)> {
        self.form().problem().or_else(|| {
            crate::opening_problem(
                self.kind,
                self.side,
                self.mask.as_ref(),
                self.paste.as_ref(),
            )
        })
    }

    /// Its shape fields, for the geometry and the checks every Pad shares.
    pub fn form(&self) -> PadForm {
        PadForm {
            kind: self.kind,
            shape: self.shape,
            width: self.width,
            height: self.height,
            corner_ratio: self.corner_ratio,
            chamfer_ratio: self.chamfer_ratio,
            corners: self.corners,
            offset: self.offset,
            hole: self.hole,
            side: self.side,
            zone_connection: self.zone_connection,
        }
    }

    /// Its copper outline in its own frame, the anchor at the origin.
    pub fn outline(&self) -> CornerRect {
        self.form().outline()
    }

    /// A box around its copper in the Footprint's frame.
    pub fn bounds(&self) -> Bounds {
        let corners = self
            .outline()
            .box_corners()
            .map(|p| (p + self.at).rotated_about(self.at, self.rotation));
        Bounds::of_points(corners).expect("four corners")
    }

    /// The side its copper is on in the Footprint's frame: `None` for a Pad with a hole.
    pub fn copper_side(&self) -> Option<Side> {
        self.form().copper_side()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PadKind {
    Smd,
    PlatedThroughHole,
    NonPlated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    Circle,
    Rect,
    /// A rectangle with rounded corners: `corner_ratio`, on `corners`.
    RoundedRect,
    Oval,
    /// A rectangle with `corners` cut off by `chamfer_ratio`, the others rounded by
    /// `corner_ratio`. An octagon is one.
    ChamferedRect,
}

/// A drilled hole: round when `width == height`, otherwise a slot along the longer side.
/// Sizes are the finished diameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hole {
    pub width: Nm,
    pub height: Nm,
    /// Turns a slot in the Pad's frame, counter-clockwise.
    #[serde(default, skip_serializing_if = "is_zero_angle")]
    pub rotation: Millideg,
    /// The allowed deviation from the finished size. Stored and shown; it changes no
    /// geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<HoleTolerance>,
}

fn is_zero_angle(a: &Millideg) -> bool {
    *a == Millideg::ZERO
}

impl Hole {
    /// A round hole or an unturned slot.
    pub const fn new(width: Nm, height: Nm) -> Hole {
        Hole {
            width,
            height,
            rotation: Millideg::ZERO,
            tolerance: None,
        }
    }

    /// The drill diameter: the shorter side.
    pub fn diameter(&self) -> Nm {
        self.width.min(self.height)
    }

    /// The ends of a slot's centre line in the Pad's frame, about the anchor; both at the
    /// anchor for a round hole.
    pub fn ends(&self) -> (Point, Point) {
        let half = (self.width.0 - self.height.0).abs() / 2;
        let along = if self.width >= self.height {
            (half, 0)
        } else {
            (0, half)
        };
        let (x, y) = reda_geom::rotate(along.0, along.1, self.rotation);
        (Point::new(Nm(-x), Nm(-y)), Point::new(Nm(x), Nm(y)))
    }
}

/// How far a finished hole may come out larger (`plus`) or smaller (`minus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HoleTolerance {
    pub plus: Nm,
    pub minus: Nm,
}

/// A share of half a Pad's shorter side, in thousandths of a percent: 100 000 is 100%, a
/// fully round corner. A resized Pad keeps its look. Serialises as a bare integer.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Ratio(pub u32);

impl JsonSchema for Ratio {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Ratio".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "integer",
            "minimum": 0,
            "maximum": Ratio::FULL.0,
            "description": "Thousandths of a percent of half the shorter side: 100000 is fully round."
        })
    }
}

impl Ratio {
    /// 100%.
    pub const FULL: Ratio = Ratio(100_000);

    /// A whole number of percent.
    pub const fn percent(p: u32) -> Ratio {
        Ratio(p * 1000)
    }

    /// This share of half the shorter of `width` and `height`, to the nearest nanometre.
    pub fn of(self, width: Nm, height: Nm) -> Nm {
        let half = i128::from(width.0.min(height.0)) / 2;
        let full = i128::from(Ratio::FULL.0);
        Nm(((half * i128::from(self.0) * 2 + full) / (2 * full)) as i64)
    }

    /// The ratio that gives `length` on a `width` × `height` Pad, rounded and capped at
    /// 100%; 0 for a Pad with no size.
    pub fn from_length(length: Nm, width: Nm, height: Nm) -> Ratio {
        let half = width.0.min(height.0) / 2;
        if half <= 0 || length.0 <= 0 {
            return Ratio(0);
        }
        let r = (i128::from(length.0) * i128::from(Ratio::FULL.0) + i128::from(half) / 2)
            / i128::from(half);
        Ratio(r.min(i128::from(Ratio::FULL.0)) as u32)
    }
}

/// A corner of a Pad, in its own frame (+y up).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    /// In [`CornerRect::corners`] order.
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];

    const fn bit(self) -> u8 {
        1 << self as u8
    }

    /// The corner it becomes when the Pad is mirrored left-right.
    pub const fn mirrored(self) -> Corner {
        match self {
            Corner::TopLeft => Corner::TopRight,
            Corner::TopRight => Corner::TopLeft,
            Corner::BottomLeft => Corner::BottomRight,
            Corner::BottomRight => Corner::BottomLeft,
        }
    }

    /// The corner it becomes when the Pad is mirrored top-bottom.
    pub const fn flipped(self) -> Corner {
        match self {
            Corner::TopLeft => Corner::BottomLeft,
            Corner::TopRight => Corner::BottomRight,
            Corner::BottomLeft => Corner::TopLeft,
            Corner::BottomRight => Corner::TopRight,
        }
    }
}

/// A set of corners, stored as a list in the order top-left, top-right, bottom-left,
/// bottom-right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Corners(u8);

impl Corners {
    pub const ALL: Corners = Corners(0b1111);
    pub const NONE: Corners = Corners(0);

    pub fn contains(self, corner: Corner) -> bool {
        self.0 & corner.bit() != 0
    }

    pub fn with(self, corner: Corner) -> Corners {
        Corners(self.0 | corner.bit())
    }

    pub fn iter(self) -> impl Iterator<Item = Corner> {
        Corner::ALL.into_iter().filter(move |c| self.contains(*c))
    }

    /// The same corners after the Pad is mirrored left-right.
    pub fn mirrored(self) -> Corners {
        self.iter().map(Corner::mirrored).collect()
    }

    /// The same corners after the Pad is mirrored top-bottom.
    pub fn flipped(self) -> Corners {
        self.iter().map(Corner::flipped).collect()
    }
}

impl FromIterator<Corner> for Corners {
    fn from_iter<I: IntoIterator<Item = Corner>>(iter: I) -> Corners {
        iter.into_iter().fold(Corners::NONE, Corners::with)
    }
}

impl Serialize for Corners {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.iter().count()))?;
        for corner in self.iter() {
            seq.serialize_element(&corner)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Corners {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Corners, D::Error> {
        struct List;
        impl<'de> Visitor<'de> for List {
            type Value = Corners;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of corners")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Corners, A::Error> {
                let mut corners = Corners::NONE;
                while let Some(corner) = seq.next_element::<Corner>()? {
                    corners = corners.with(corner);
                }
                Ok(corners)
            }
        }
        deserializer.deserialize_seq(List)
    }
}

impl JsonSchema for Corners {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Corners".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = <Vec<Corner>>::json_schema(generator);
        schema.insert("uniqueItems".into(), true.into());
        schema
    }
}

/// How a Zone fill joins a Pad of its own Net. Each value left out inherits from the
/// `zone_connection` rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneConnection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ConnectionStyle>,
    /// 2 or 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spokes: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoke_width: Option<Nm>,
    /// The clear ring around a relieved or unconnected Pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<Nm>,
    /// 0, 45 or 90 degrees in the Pad's frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<Millideg>,
}

impl ZoneConnection {
    /// What is wrong with the values, if anything: a JSON pointer below the connection and
    /// a message.
    pub fn problem(&self) -> Option<(&'static str, &'static str)> {
        if self.spokes.is_some_and(|n| n != 2 && n != 4) {
            return Some(("/spokes", "a Pad has 2 or 4 spokes"));
        }
        if self.spoke_width.is_some_and(|w| w <= Nm::ZERO) {
            return Some(("/spoke_width", "the spoke width must be positive"));
        }
        if self.gap.is_some_and(|g| g < Nm::ZERO) {
            return Some(("/gap", "the gap cannot be negative"));
        }
        if self
            .angle
            .is_some_and(|a| ![0, 45_000, 90_000].contains(&a.0))
        {
            return Some(("/angle", "the spoke angle is 0, 45 or 90 degrees"));
        }
        None
    }
}

/// How a fill meets a Pad or Via.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStyle {
    /// Through spokes across a clear ring, so the Pad solders easily.
    Relief,
    /// The fill runs straight into it.
    Solid,
    /// The fill keeps the gap clear and doesn't connect.
    None,
}

/// The shape fields every Pad has, Footprint Pad or Free Pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadForm {
    pub kind: PadKind,
    pub shape: PadShape,
    pub width: Nm,
    pub height: Nm,
    pub corner_ratio: Option<Ratio>,
    pub chamfer_ratio: Option<Ratio>,
    pub corners: Option<Corners>,
    pub offset: Option<Point>,
    pub hole: Option<Hole>,
    pub side: Option<Side>,
    pub zone_connection: Option<ZoneConnection>,
}

impl PadForm {
    /// The copper outline in the Pad's frame, the anchor at the origin.
    pub fn outline(&self) -> CornerRect {
        let (w, h) = (self.width, self.height);
        let center = self.offset.unwrap_or_default();
        let corners = self.corners.unwrap_or(Corners::ALL);
        let round = |ratio: Option<Ratio>| match ratio.unwrap_or_default().of(w, h) {
            Nm::ZERO => CornerCut::Square,
            r => CornerCut::Round(r),
        };
        match self.shape {
            PadShape::Circle => CornerRect::uniform(w, w, center, CornerCut::Round(Nm(w.0 / 2))),
            PadShape::Rect => CornerRect::uniform(w, h, center, CornerCut::Square),
            PadShape::Oval => {
                CornerRect::uniform(w, h, center, CornerCut::Round(Nm(w.0.min(h.0) / 2)))
            }
            PadShape::RoundedRect => {
                let cut = round(self.corner_ratio);
                CornerRect {
                    width: w,
                    height: h,
                    center,
                    corners: Corner::ALL.map(|c| match corners.contains(c) {
                        true => cut,
                        false => CornerCut::Square,
                    }),
                }
            }
            PadShape::ChamferedRect => {
                let chamfer = match self.chamfer_ratio.unwrap_or_default().of(w, h) {
                    Nm::ZERO => CornerCut::Square,
                    c => CornerCut::Chamfer(c),
                };
                let other = round(self.corner_ratio);
                CornerRect {
                    width: w,
                    height: h,
                    center,
                    corners: Corner::ALL.map(|c| match corners.contains(c) {
                        true => chamfer,
                        false => other,
                    }),
                }
            }
        }
    }

    /// The side its copper is on: `None` for a Pad with a hole, which reaches every Layer.
    pub fn copper_side(&self) -> Option<Side> {
        (self.kind == PadKind::Smd).then(|| self.side.unwrap_or(Side::Top))
    }

    /// The narrowest copper between the hole and the copper's edge, negative when the
    /// hole reaches outside the copper; `None` without a hole. The copper is convex, so
    /// the narrowest point is at an end of the slot's centre line.
    pub fn annular_ring(&self) -> Option<Nm> {
        let hole = self.hole?;
        let outline = self.outline();
        let (a, b) = hole.ends();
        let inside = outline.edge_distance(a).min(outline.edge_distance(b));
        Some(Nm((inside - hole.diameter().0 as f64 / 2.0).floor() as i64))
    }

    /// What is wrong with these fields, if anything: a JSON pointer to the field and a
    /// message.
    pub fn problem(&self) -> Option<(String, String)> {
        let at = |pointer: &str, message: &str| Some((pointer.to_string(), message.to_string()));
        if self.width <= Nm::ZERO {
            return at("/width", "the Pad width must be positive");
        }
        if self.height <= Nm::ZERO {
            return at("/height", "the Pad height must be positive");
        }
        let cornered = matches!(self.shape, PadShape::RoundedRect | PadShape::ChamferedRect);
        let full = |r: Option<Ratio>| r.is_some_and(|r| r > Ratio::FULL);
        if self.corner_ratio.is_some() && !cornered {
            return at(
                "/corner_ratio",
                "only a rounded_rect or chamfered_rect Pad has a corner ratio",
            );
        }
        if self.shape == PadShape::RoundedRect && self.corner_ratio.is_none() {
            return at("/corner_ratio", "a rounded_rect Pad needs a corner ratio");
        }
        if full(self.corner_ratio) {
            return at("/corner_ratio", "a ratio is at most 100%");
        }
        match (self.shape, self.chamfer_ratio) {
            (PadShape::ChamferedRect, None) => {
                return at(
                    "/chamfer_ratio",
                    "a chamfered_rect Pad needs a chamfer ratio",
                );
            }
            (PadShape::ChamferedRect, r) if full(r) => {
                return at("/chamfer_ratio", "a ratio is at most 100%");
            }
            (PadShape::ChamferedRect, _) | (_, None) => {}
            (_, Some(_)) => {
                return at(
                    "/chamfer_ratio",
                    "only a chamfered_rect Pad has a chamfer ratio",
                );
            }
        }
        if self.corners.is_some() && !cornered {
            return at(
                "/corners",
                "only a rounded_rect or chamfered_rect Pad picks its corners",
            );
        }
        match (self.kind, self.hole) {
            (PadKind::Smd, Some(_)) => return at("/hole", "an SMD Pad has no hole"),
            (PadKind::Smd, None) => {}
            (_, None) => return at("/hole", "a through-hole or non-plated Pad needs a hole"),
            (_, Some(hole)) => {
                if hole.width <= Nm::ZERO || hole.height <= Nm::ZERO {
                    return at("/hole", "the hole size must be positive");
                }
                if let Some(t) = hole.tolerance
                    && (t.plus < Nm::ZERO || t.minus < Nm::ZERO)
                {
                    return at("/hole/tolerance", "a tolerance cannot be negative");
                }
                let (hw, hh) = turned_size(hole.width, hole.height, hole.rotation);
                if hw > self.width || hh > self.height {
                    return at("/hole", "the hole is larger than the Pad");
                }
            }
        }
        if self.side.is_some() && self.kind != PadKind::Smd {
            return at("/side", "a Pad with a hole is on every copper Layer");
        }
        if let Some(connection) = self.zone_connection {
            if self.kind == PadKind::NonPlated {
                return at(
                    "/zone_connection",
                    "a non-plated Pad has no copper for a Zone to connect",
                );
            }
            if let Some((pointer, message)) = connection.problem() {
                return Some((format!("/zone_connection{pointer}"), message.into()));
            }
        }
        None
    }
}

/// The box a `width` × `height` rectangle turned by `rotation` fills, to the nanometre.
fn turned_size(width: Nm, height: Nm, rotation: Millideg) -> (Nm, Nm) {
    let (hw, hh) = (width.0 / 2, height.0 / 2);
    let corners = [(hw, hh), (hw, -hh)].map(|(x, y)| reda_geom::rotate(x, y, rotation));
    let x = corners.iter().map(|c| c.0.abs()).max().unwrap_or(0);
    let y = corners.iter().map(|c| c.1.abs()).max().unwrap_or(0);
    (Nm(2 * x), Nm(2 * y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(shape: PadShape) -> PadForm {
        PadForm {
            kind: PadKind::Smd,
            shape,
            width: Nm(2_000_000),
            height: Nm(1_000_000),
            corner_ratio: None,
            chamfer_ratio: None,
            corners: None,
            offset: None,
            hole: None,
            side: None,
            zone_connection: None,
        }
    }

    #[test]
    fn a_ratio_is_a_share_of_half_the_shorter_side() {
        assert_eq!(
            Ratio::percent(50).of(Nm(2_000_000), Nm(1_000_000)),
            Nm(250_000)
        );
        assert_eq!(Ratio::FULL.of(Nm(2_000_000), Nm(1_000_000)), Nm(500_000));
        assert_eq!(
            Ratio::from_length(Nm(250_000), Nm(2_000_000), Nm(1_000_000)),
            Ratio::percent(50)
        );
    }

    #[test]
    fn a_rounded_rect_rounds_only_its_listed_corners() {
        let mut f = form(PadShape::RoundedRect);
        f.corner_ratio = Some(Ratio::percent(50));
        f.corners = Some([Corner::TopLeft].into_iter().collect());
        let outline = f.outline();
        assert_eq!(outline.corners[0], CornerCut::Round(Nm(250_000)));
        assert_eq!(outline.corners[1..], [CornerCut::Square; 3]);
    }

    #[test]
    fn a_chamfered_rect_rounds_its_other_corners() {
        let mut f = form(PadShape::ChamferedRect);
        f.chamfer_ratio = Some(Ratio::percent(40));
        f.corner_ratio = Some(Ratio::percent(20));
        f.corners = Some(
            [Corner::TopRight, Corner::BottomRight]
                .into_iter()
                .collect(),
        );
        f.offset = Some(Point::new(Nm(100), Nm(0)));
        let outline = f.outline();
        assert_eq!(outline.center, Point::new(Nm(100), Nm(0)));
        assert_eq!(
            outline.corners,
            [
                CornerCut::Round(Nm(100_000)),
                CornerCut::Chamfer(Nm(200_000)),
                CornerCut::Round(Nm(100_000)),
                CornerCut::Chamfer(Nm(200_000)),
            ]
        );
    }

    #[test]
    fn corners_serialise_as_an_ordered_list() {
        let corners: Corners = [Corner::BottomRight, Corner::TopLeft].into_iter().collect();
        let json = serde_json::to_string(&corners).unwrap();
        assert_eq!(json, r#"["top_left","bottom_right"]"#);
        assert_eq!(serde_json::from_str::<Corners>(&json).unwrap(), corners);
        assert_eq!(
            corners.mirrored(),
            [Corner::TopRight, Corner::BottomLeft].into_iter().collect()
        );
    }

    #[test]
    fn shape_fields_that_do_not_fit_the_shape_are_refused() {
        let mut f = form(PadShape::Rect);
        f.corner_ratio = Some(Ratio::percent(10));
        assert_eq!(f.problem().unwrap().0, "/corner_ratio");
        let mut f = form(PadShape::ChamferedRect);
        assert_eq!(f.problem().unwrap().0, "/chamfer_ratio");
        f.chamfer_ratio = Some(Ratio::percent(30));
        assert_eq!(f.problem(), None);
        f.side = Some(Side::Bottom);
        assert_eq!(f.problem(), None);
        f.kind = PadKind::PlatedThroughHole;
        f.hole = Some(Hole::new(Nm(500_000), Nm(500_000)));
        assert_eq!(f.problem().unwrap().0, "/side");
        f.side = None;
        f.zone_connection = Some(ZoneConnection {
            spokes: Some(3),
            ..ZoneConnection::default()
        });
        assert_eq!(f.problem().unwrap().0, "/zone_connection/spokes");
    }

    #[test]
    fn a_turned_slot_must_fit_the_pad() {
        let mut f = form(PadShape::Rect);
        f.kind = PadKind::PlatedThroughHole;
        f.hole = Some(Hole::new(Nm(1_500_000), Nm(500_000)));
        assert_eq!(f.problem(), None);
        f.hole = Some(Hole {
            rotation: Millideg::from_deg(90),
            ..Hole::new(Nm(1_500_000), Nm(500_000))
        });
        assert_eq!(f.problem().unwrap().0, "/hole");
    }
}

/// The board's solder-mask seed values. Tenting here applies only to Vias.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskExpansion {
    pub top: Nm,
    pub bottom: Nm,
    pub from_hole_edge: bool,
    pub tent_vias_top: bool,
    pub tent_vias_bottom: bool,
}

/// The board's paste seed: length per edge or signed change in area in percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum PasteExpansion {
    Expansion { expansion: Nm },
    Percent { percent: i32 },
}

impl Default for MaskExpansion {
    fn default() -> Self {
        Self {
            top: Nm(50_000),
            bottom: Nm(50_000),
            from_hole_edge: false,
            tent_vias_top: true,
            tent_vias_bottom: true,
        }
    }
}
impl Default for PasteExpansion {
    fn default() -> Self {
        Self::Expansion {
            expansion: Nm::ZERO,
        }
    }
}
