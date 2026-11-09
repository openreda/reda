//! The rule values Board Setup edits: which value of which rule kind each one is, what
//! type it holds, its name for the user, and how its text is read and written. Every
//! value is drafted as text: lengths in mm, flags as `true`/`false`, a connection style
//! as `relief`/`solid`/`none`, spokes and angles (in degrees) and percentages as numbers.

use std::collections::BTreeMap;

use reda_engine::model::geom::Nm;
use reda_engine::model::{
    self as m, ConnectionTarget, ConnectionValue, FabRules, PcbDocument, RuleKind, RuleValue,
    RuleValues, Uuid,
};
use serde_json::{Map, Value, json};

use super::super::length::{mm, optional, signed};
use crate::tr;

/// One rule value the dialog edits: a value of one rule kind's records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleField {
    Clearance,
    TrackWidth,
    TrackPreferred,
    TrackMax,
    ViaDiameter,
    ViaDiameterPreferred,
    ViaDiameterMax,
    ViaHole,
    ViaHolePreferred,
    ViaHoleMax,
    HoleMin,
    HoleMax,
    AnnularRing,
    HoleToHole,
    EdgeClearance,
    SilkToEdge,
    SilkToMask,
    CourtyardClearance,
    DanglingTolerance,
    MicroSegmentLength,
    MaskTop,
    MaskBottom,
    MaskFromHoleEdge,
    TentViasTop,
    TentViasBottom,
    /// The paste opening grown (or shrunk) by a length.
    PasteExpansion,
    /// The paste opening's area changed by a percentage.
    PastePercent,
    /// One value of the `zone_connection` rule.
    Connection(ConnectionTarget, ConnectionValue),
}

/// What a value holds, which picks its widget and how its text is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    /// A length, never negative.
    Length,
    /// A length that may be negative (a mask or paste expansion).
    Signed,
    Flag,
    /// A signed change in area, above -100 %.
    Percent,
    /// `relief`, `solid` or `none`.
    Style,
    /// 2 or 4.
    Spokes,
    /// 0, 45 or 90 degrees.
    Angle,
}

/// The values the Net Classes grid has a column for: clearance, Track width and Via size.
pub const GRID_FIELDS: [RuleField; 10] = [
    RuleField::Clearance,
    RuleField::TrackWidth,
    RuleField::TrackPreferred,
    RuleField::TrackMax,
    RuleField::ViaDiameter,
    RuleField::ViaDiameterPreferred,
    RuleField::ViaDiameterMax,
    RuleField::ViaHole,
    RuleField::ViaHolePreferred,
    RuleField::ViaHoleMax,
];

/// The rule kinds of the grid's columns, each with its own `enabled` and comment.
pub const GRID_KINDS: [RuleKind; 3] =
    [RuleKind::Clearance, RuleKind::TrackWidth, RuleKind::ViaSize];

/// The connection styles, as drafted.
pub const STYLES: [&str; 3] = ["relief", "solid", "none"];
/// The spoke counts, as drafted.
pub const SPOKES: [&str; 2] = ["2", "4"];
/// The spoke angles in degrees, as drafted.
pub const ANGLES: [&str; 3] = ["0", "45", "90"];

/// A rule kind's name for the user, such as "Via size".
pub fn kind_name(kind: RuleKind) -> String {
    match kind {
        RuleKind::Clearance => tr!("board-setup-clearance"),
        RuleKind::TrackWidth => tr!("board-setup-track-width"),
        RuleKind::ViaSize => tr!("board-setup-via-size"),
        RuleKind::MaskExpansion => tr!("board-setup-mask-expansion"),
        RuleKind::PasteExpansion => tr!("board-setup-paste-expansion"),
        RuleKind::ZoneConnection => tr!("board-setup-zone-connection"),
        other => other.id().to_owned(),
    }
}

/// A connection target's name, such as "SMD Pads".
pub fn target_name(target: ConnectionTarget) -> String {
    match target {
        ConnectionTarget::ThroughHole => tr!("board-setup-target-through-hole"),
        ConnectionTarget::Smd => tr!("board-setup-target-smd"),
        ConnectionTarget::Via => tr!("board-setup-target-via"),
    }
}

/// A connection value's name, such as "Spokes".
pub fn connection_value_name(value: ConnectionValue) -> String {
    match value {
        ConnectionValue::Style => tr!("board-setup-connection-style"),
        ConnectionValue::Spokes => tr!("board-setup-connection-spokes"),
        ConnectionValue::SpokeWidth => tr!("board-setup-connection-spoke-width"),
        ConnectionValue::Gap => tr!("board-setup-connection-gap"),
        ConnectionValue::Angle => tr!("board-setup-connection-angle"),
    }
}

/// A drafted connection style's name, such as "Relief".
pub fn style_name(style: &str) -> String {
    match style {
        "relief" => tr!("board-setup-style-relief"),
        "solid" => tr!("board-setup-style-solid"),
        "none" => tr!("board-setup-style-none"),
        other => other.to_owned(),
    }
}

impl RuleField {
    pub const ALL: [RuleField; 42] = {
        use ConnectionTarget as T;
        use ConnectionValue as V;
        use RuleField as R;
        [
            R::Clearance,
            R::TrackWidth,
            R::TrackPreferred,
            R::TrackMax,
            R::ViaDiameter,
            R::ViaDiameterPreferred,
            R::ViaDiameterMax,
            R::ViaHole,
            R::ViaHolePreferred,
            R::ViaHoleMax,
            R::HoleMin,
            R::HoleMax,
            R::AnnularRing,
            R::HoleToHole,
            R::EdgeClearance,
            R::SilkToEdge,
            R::SilkToMask,
            R::CourtyardClearance,
            R::DanglingTolerance,
            R::MicroSegmentLength,
            R::MaskTop,
            R::MaskBottom,
            R::MaskFromHoleEdge,
            R::TentViasTop,
            R::TentViasBottom,
            R::PasteExpansion,
            R::PastePercent,
            R::Connection(T::ThroughHole, V::Style),
            R::Connection(T::ThroughHole, V::Spokes),
            R::Connection(T::ThroughHole, V::SpokeWidth),
            R::Connection(T::ThroughHole, V::Gap),
            R::Connection(T::ThroughHole, V::Angle),
            R::Connection(T::Smd, V::Style),
            R::Connection(T::Smd, V::Spokes),
            R::Connection(T::Smd, V::SpokeWidth),
            R::Connection(T::Smd, V::Gap),
            R::Connection(T::Smd, V::Angle),
            R::Connection(T::Via, V::Style),
            R::Connection(T::Via, V::Spokes),
            R::Connection(T::Via, V::SpokeWidth),
            R::Connection(T::Via, V::Gap),
            R::Connection(T::Via, V::Angle),
        ]
    };

    /// One target's `zone_connection` values, in order.
    pub fn connection(target: ConnectionTarget) -> [RuleField; 5] {
        ConnectionValue::ALL.map(|v| RuleField::Connection(target, v))
    }

    /// The min, preferred and max values typed together (one range of a grid kind), or
    /// nothing.
    pub fn range(self) -> &'static [RuleField] {
        use RuleField as R;
        match self {
            R::TrackWidth | R::TrackPreferred | R::TrackMax => {
                &[R::TrackWidth, R::TrackPreferred, R::TrackMax]
            }
            R::ViaDiameter | R::ViaDiameterPreferred | R::ViaDiameterMax => {
                &[R::ViaDiameter, R::ViaDiameterPreferred, R::ViaDiameterMax]
            }
            R::ViaHole | R::ViaHolePreferred | R::ViaHoleMax => {
                &[R::ViaHole, R::ViaHolePreferred, R::ViaHoleMax]
            }
            _ => &[],
        }
    }

    /// Whether it is a maximum.
    pub fn is_max(self) -> bool {
        matches!(
            self.rule().1,
            m::RuleField::Max | m::RuleField::DiameterMax | m::RuleField::HoleMax
        )
    }

    /// What it holds.
    pub fn kind(self) -> ValueKind {
        use RuleField as R;
        match self {
            R::MaskTop | R::MaskBottom | R::PasteExpansion => ValueKind::Signed,
            R::MaskFromHoleEdge | R::TentViasTop | R::TentViasBottom => ValueKind::Flag,
            R::PastePercent => ValueKind::Percent,
            R::Connection(_, ConnectionValue::Style) => ValueKind::Style,
            R::Connection(_, ConnectionValue::Spokes) => ValueKind::Spokes,
            R::Connection(_, ConnectionValue::Angle) => ValueKind::Angle,
            _ => ValueKind::Length,
        }
    }

    /// The rule kind and the value of its records.
    pub fn rule(self) -> (RuleKind, m::RuleField) {
        use m::RuleField as F;
        match self {
            RuleField::Clearance => (RuleKind::Clearance, F::Value),
            RuleField::TrackWidth => (RuleKind::TrackWidth, F::Min),
            RuleField::TrackPreferred => (RuleKind::TrackWidth, F::Preferred),
            RuleField::TrackMax => (RuleKind::TrackWidth, F::Max),
            RuleField::ViaDiameter => (RuleKind::ViaSize, F::DiameterMin),
            RuleField::ViaDiameterPreferred => (RuleKind::ViaSize, F::DiameterPreferred),
            RuleField::ViaDiameterMax => (RuleKind::ViaSize, F::DiameterMax),
            RuleField::ViaHole => (RuleKind::ViaSize, F::HoleMin),
            RuleField::ViaHolePreferred => (RuleKind::ViaSize, F::HolePreferred),
            RuleField::ViaHoleMax => (RuleKind::ViaSize, F::HoleMax),
            RuleField::HoleMin => (RuleKind::HoleSize, F::Min),
            RuleField::HoleMax => (RuleKind::HoleSize, F::Max),
            RuleField::AnnularRing => (RuleKind::AnnularRing, F::Min),
            RuleField::HoleToHole => (RuleKind::HoleToHole, F::Min),
            RuleField::EdgeClearance => (RuleKind::EdgeClearance, F::Value),
            RuleField::SilkToEdge => (RuleKind::SilkToEdge, F::Value),
            RuleField::SilkToMask => (RuleKind::SilkToMask, F::Value),
            RuleField::CourtyardClearance => (RuleKind::CourtyardClearance, F::Value),
            RuleField::DanglingTolerance => (RuleKind::Dangling, F::Tolerance),
            RuleField::MicroSegmentLength => (RuleKind::MicroSegment, F::MaxLength),
            RuleField::MaskTop => (RuleKind::MaskExpansion, F::Top),
            RuleField::MaskBottom => (RuleKind::MaskExpansion, F::Bottom),
            RuleField::MaskFromHoleEdge => (RuleKind::MaskExpansion, F::FromHoleEdge),
            RuleField::TentViasTop => (RuleKind::MaskExpansion, F::TentViasTop),
            RuleField::TentViasBottom => (RuleKind::MaskExpansion, F::TentViasBottom),
            RuleField::PasteExpansion => (RuleKind::PasteExpansion, F::Expansion),
            RuleField::PastePercent => (RuleKind::PasteExpansion, F::Percent),
            RuleField::Connection(t, v) => (RuleKind::ZoneConnection, F::connection_field(t, v)),
        }
    }

    /// Its name in problem fields, such as `track_width.preferred`.
    pub fn id(self) -> String {
        let (kind, field) = self.rule();
        format!("{}.{}", kind.id(), field.path())
    }

    /// Its length on `pcb`'s board-wide record.
    pub fn get(self, pcb: &PcbDocument) -> Option<Nm> {
        let (kind, field) = self.rule();
        pcb.board_rule(kind).values.length(field)
    }

    /// Its value in `values` as drafted text; empty when they leave it out.
    pub fn text_in(self, values: &RuleValues) -> String {
        let Some(value) = values.get(self.rule().1) else {
            return String::new();
        };
        match value {
            RuleValue::Length(n) => mm(n),
            RuleValue::Flag(b) => b.to_string(),
            RuleValue::Percent(p) => p.to_string(),
            RuleValue::Count(n) => n.to_string(),
            RuleValue::Angle(a) => mm(Nm(a.0 * 1000)),
            RuleValue::Style(s) => serde_json::to_value(s)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default(),
            RuleValue::Mode(_) | RuleValue::Area(_) => String::new(),
        }
    }

    /// Its value on `pcb`'s board-wide record, as text.
    pub fn board_text(self, pcb: &PcbDocument) -> String {
        self.text_in(&pcb.board_rule(self.rule().0).values)
    }

    /// Its value on `class`'s record, as text; empty with no record.
    pub fn class_text(self, pcb: &PcbDocument, class: Uuid) -> String {
        pcb.class_rule(self.rule().0, class)
            .map_or_else(String::new, |r| self.text_in(&r.values))
    }

    /// The length a Fab Preset seeds it with.
    pub fn seed(self, rules: &FabRules) -> Option<Nm> {
        self.seed_values(rules)?.length(self.rule().1)
    }

    /// The value a Fab Preset seeds it with, as text.
    pub fn seed_text(self, rules: &FabRules) -> Option<String> {
        let text = self.text_in(&self.seed_values(rules)?);
        (!text.is_empty()).then_some(text)
    }

    fn seed_values(self, rules: &FabRules) -> Option<RuleValues> {
        let kind = self.rule().0;
        reda_engine::pcb::seed_rules(rules)
            .into_iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| v)
    }

    /// Reads typed `text` as the param value `pcb.rules.set` takes; `Ok(None)` when it
    /// is empty.
    pub fn parse(self, text: &str) -> Result<Option<Value>, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let one_of = |options: &[&str], message: String| {
            options.contains(&text).then_some(()).ok_or(message)
        };
        Ok(Some(match self.kind() {
            ValueKind::Length => json!(optional(text)?.map_or(0, |n| n.0)),
            ValueKind::Signed => json!(signed(text)?.map_or(0, |n| n.0)),
            ValueKind::Flag => match text {
                "true" => json!(true),
                "false" => json!(false),
                _ => return Err(tr!("board-setup-not-a-flag")),
            },
            ValueKind::Percent => {
                let number = text.trim_end_matches('%').trim().parse::<i32>();
                match number {
                    Ok(p) if p > -100 => json!(p),
                    _ => return Err(tr!("board-setup-not-a-percent")),
                }
            }
            ValueKind::Style => {
                one_of(&STYLES, tr!("board-setup-not-a-style"))?;
                json!(text)
            }
            ValueKind::Spokes => {
                one_of(&SPOKES, tr!("board-setup-not-spokes"))?;
                json!(text.parse::<u8>().expect("a listed count"))
            }
            ValueKind::Angle => {
                one_of(&ANGLES, tr!("board-setup-not-an-angle"))?;
                json!(text.parse::<i64>().expect("a listed angle") * 1000)
            }
        }))
    }

    /// Why typed `text` is not a valid value, if it is not; `required` refuses an empty
    /// one.
    pub fn check(self, text: &str, required: bool) -> Result<(), String> {
        match self.parse(text)? {
            None if required => Err(match self.kind() {
                ValueKind::Length | ValueKind::Signed => tr!("board-setup-required"),
                _ => tr!("board-setup-required-value"),
            }),
            _ => Ok(()),
        }
    }

    /// Whether the board-wide record must hold it.
    pub fn required(self) -> bool {
        let (kind, field) = self.rule();
        kind.required().contains(&field)
    }

    /// The name of what it limits, such as "Track width", for the min, preferred and max
    /// values alike.
    fn base_name(self) -> String {
        match self {
            RuleField::Clearance => tr!("board-setup-clearance"),
            RuleField::TrackWidth | RuleField::TrackPreferred | RuleField::TrackMax => {
                tr!("board-setup-track-width")
            }
            RuleField::ViaDiameter
            | RuleField::ViaDiameterPreferred
            | RuleField::ViaDiameterMax => {
                tr!("board-setup-via-diameter")
            }
            RuleField::ViaHole | RuleField::ViaHolePreferred | RuleField::ViaHoleMax => {
                tr!("board-setup-via-hole")
            }
            RuleField::HoleMin | RuleField::HoleMax => tr!("board-setup-pad-hole"),
            RuleField::AnnularRing => tr!("board-setup-annular-ring"),
            RuleField::HoleToHole => tr!("board-setup-hole-to-hole"),
            RuleField::EdgeClearance => tr!("board-setup-edge-clearance"),
            RuleField::SilkToEdge => tr!("board-setup-silk-to-edge"),
            RuleField::SilkToMask => tr!("board-setup-silk-to-mask"),
            RuleField::CourtyardClearance => tr!("board-setup-courtyard-clearance"),
            RuleField::DanglingTolerance => tr!("board-setup-dangling-tolerance"),
            RuleField::MicroSegmentLength => tr!("board-setup-micro-segment-length"),
            RuleField::MaskTop => tr!("board-setup-mask-top"),
            RuleField::MaskBottom => tr!("board-setup-mask-bottom"),
            RuleField::MaskFromHoleEdge => tr!("board-setup-mask-from-hole-edge"),
            RuleField::TentViasTop => tr!("board-setup-tent-top"),
            RuleField::TentViasBottom => tr!("board-setup-tent-bottom"),
            RuleField::PasteExpansion => tr!("board-setup-paste-expansion"),
            RuleField::PastePercent => tr!("board-setup-paste-percent"),
            RuleField::Connection(t, v) => tr!(
                "board-setup-connection",
                target = target_name(t),
                value = connection_value_name(v)
            ),
        }
    }

    /// Whether it is one of the values outside the min/preferred/max families, which
    /// are named plainly.
    fn plain(self) -> bool {
        matches!(
            self.rule().0,
            RuleKind::CourtyardClearance
                | RuleKind::Dangling
                | RuleKind::MicroSegment
                | RuleKind::MaskExpansion
                | RuleKind::PasteExpansion
                | RuleKind::ZoneConnection
        )
    }

    /// Its name for the user: "Track width" for a minimum, "Preferred Track width".
    pub fn name(self) -> String {
        let base = self.base_name();
        if self.plain() {
            return base;
        }
        match self.rule().1 {
            f if f.is_preferred() => tr!("board-setup-preferred", rule = base),
            m::RuleField::Max | m::RuleField::DiameterMax | m::RuleField::HoleMax => {
                tr!("board-setup-maximum", rule = base)
            }
            _ => base,
        }
    }

    /// The label of its board-wide field, such as "Board-wide minimum Clearance (mm)".
    pub fn board_wide_label(self) -> String {
        let rule = self.base_name();
        if self.plain() {
            return match self.kind() {
                ValueKind::Length | ValueKind::Signed => tr!("board-setup-value", rule = rule),
                ValueKind::Percent => tr!("board-setup-value-percent", rule = rule),
                ValueKind::Angle => tr!("board-setup-value-degrees", rule = rule),
                _ => rule,
            };
        }
        match self.rule().1 {
            f if f.is_preferred() => tr!("board-setup-board-wide-preferred", rule = rule),
            m::RuleField::Max | m::RuleField::DiameterMax | m::RuleField::HoleMax => {
                tr!("board-setup-board-wide-maximum", rule = rule)
            }
            _ => tr!("board-setup-board-wide", rule = rule),
        }
    }

    /// The problem field of the board-wide value.
    pub fn problem_field(self) -> String {
        format!("rule/{}", self.id())
    }

    /// Whether `draft` holds a valid value other than the Project's: the param value
    /// (`null` to clear it).
    pub fn changed(self, value: &str, base: &str) -> Option<Value> {
        let now = self.parse(value).ok()?;
        let was = self.parse(base).ok().flatten();
        (now != was).then(|| now.unwrap_or(Value::Null))
    }
}

/// `values` as `pcb.rules.set` takes them: `{"min": 1}` or `{"diameter": {"min": 1}}`.
pub(super) fn values_param(values: &[(m::RuleField, Value)]) -> Value {
    let mut out = Map::new();
    for (field, value) in values {
        match field.path().split_once('.') {
            Some((outer, inner)) => {
                let entry = out
                    .entry(outer.to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Value::Object(map) = entry {
                    map.insert(inner.to_owned(), value.clone());
                }
            }
            None => {
                out.insert(field.path().to_owned(), value.clone());
            }
        }
    }
    Value::Object(out)
}

/// Changed values grouped by rule kind, in kind order.
pub(super) fn by_kind(
    changes: impl Iterator<Item = (RuleField, Value)>,
) -> BTreeMap<RuleKind, Vec<(m::RuleField, Value)>> {
    let mut out: BTreeMap<RuleKind, Vec<_>> = BTreeMap::new();
    for (rule, value) in changes {
        let (kind, field) = rule.rule();
        out.entry(kind).or_default().push((field, value));
    }
    out
}
