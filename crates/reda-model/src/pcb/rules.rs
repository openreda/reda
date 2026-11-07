//! Design rules as scoped records (ADR-0012): one table of [`DesignRule`]s, each with a
//! [`RuleKind`], a [`RuleScope`] and the [`RuleValues`] it sets.
//!
//! The board-wide record of a kind holds every value the kind requires; a Net Class record
//! holds only the values it overrides. Which value applies to a Net is resolved in
//! `reda-pcb`, not here.

use reda_geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// What a rule record limits.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// Clearance between copper of different Nets: `value`, `ignore_pad_to_pad`.
    Clearance,
    /// Track width: `min`, `preferred`, `max?`.
    TrackWidth,
    /// Via size: `diameter` and `hole`, each `{min, preferred, max?}`.
    ViaSize,
    /// Pad, Free Pad and non-plated hole size: `min`, `max`.
    HoleSize,
    /// Annular ring: `min`.
    AnnularRing,
    /// Spacing between hole edges: `min`.
    HoleToHole,
    /// Copper to the Board Outline and Board Cutouts: `value`.
    EdgeClearance,
    /// Per-side solder-mask expansion, hole-edge reference and Via tenting; no DRC severity.
    MaskExpansion,
    /// Solder-paste expansion per edge or signed percent change in area; no DRC severity.
    PasteExpansion,
}

impl RuleKind {
    /// Every kind, in file order.
    pub const ALL: [RuleKind; 9] = [
        RuleKind::Clearance,
        RuleKind::TrackWidth,
        RuleKind::ViaSize,
        RuleKind::HoleSize,
        RuleKind::AnnularRing,
        RuleKind::HoleToHole,
        RuleKind::EdgeClearance,
        RuleKind::MaskExpansion,
        RuleKind::PasteExpansion,
    ];

    /// Its name, as the file and Commands spell it.
    pub fn id(self) -> &'static str {
        match self {
            RuleKind::Clearance => "clearance",
            RuleKind::TrackWidth => "track_width",
            RuleKind::ViaSize => "via_size",
            RuleKind::HoleSize => "hole_size",
            RuleKind::AnnularRing => "annular_ring",
            RuleKind::HoleToHole => "hole_to_hole",
            RuleKind::EdgeClearance => "edge_clearance",
            RuleKind::MaskExpansion => "mask_expansion",
            RuleKind::PasteExpansion => "paste_expansion",
        }
    }

    /// The values a record of this kind may hold.
    pub fn fields(self) -> &'static [RuleField] {
        use RuleField::*;
        match self {
            RuleKind::Clearance => &[Value, IgnorePadToPad],
            RuleKind::TrackWidth => &[Min, Preferred, Max],
            RuleKind::ViaSize => &[
                DiameterMin,
                DiameterPreferred,
                DiameterMax,
                HoleMin,
                HolePreferred,
                HoleMax,
            ],
            RuleKind::HoleSize => &[Min, Max],
            RuleKind::AnnularRing | RuleKind::HoleToHole => &[Min],
            RuleKind::EdgeClearance => &[Value],
            RuleKind::MaskExpansion => &[Top, Bottom, FromHoleEdge, TentViasTop, TentViasBottom],
            RuleKind::PasteExpansion => &[Expansion, Percent],
        }
    }

    /// The values the board-wide record of this kind must hold.
    pub fn required(self) -> &'static [RuleField] {
        use RuleField::*;
        match self {
            RuleKind::Clearance | RuleKind::EdgeClearance => &[Value],
            RuleKind::TrackWidth => &[Min, Preferred],
            RuleKind::ViaSize => &[DiameterMin, DiameterPreferred, HoleMin, HolePreferred],
            RuleKind::HoleSize => &[Min, Max],
            RuleKind::AnnularRing | RuleKind::HoleToHole => &[Min],
            RuleKind::MaskExpansion => &[Top, Bottom, FromHoleEdge, TentViasTop, TentViasBottom],
            RuleKind::PasteExpansion => &[],
        }
    }
}

/// One value of a rule record, by where it sits in `values`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RuleField {
    Value,
    IgnorePadToPad,
    Min,
    Preferred,
    Max,
    DiameterMin,
    DiameterPreferred,
    DiameterMax,
    HoleMin,
    HolePreferred,
    HoleMax,
    Top,
    Bottom,
    FromHoleEdge,
    TentViasTop,
    TentViasBottom,
    Expansion,
    Percent,
}

impl RuleField {
    /// Its path in `values`, such as `diameter.min`.
    pub fn path(self) -> &'static str {
        match self {
            RuleField::Value => "value",
            RuleField::IgnorePadToPad => "ignore_pad_to_pad",
            RuleField::Min => "min",
            RuleField::Preferred => "preferred",
            RuleField::Max => "max",
            RuleField::DiameterMin => "diameter.min",
            RuleField::DiameterPreferred => "diameter.preferred",
            RuleField::DiameterMax => "diameter.max",
            RuleField::HoleMin => "hole.min",
            RuleField::HolePreferred => "hole.preferred",
            RuleField::HoleMax => "hole.max",
            RuleField::Top => "top",
            RuleField::Bottom => "bottom",
            RuleField::FromHoleEdge => "from_hole_edge",
            RuleField::TentViasTop => "tent_vias_top",
            RuleField::TentViasBottom => "tent_vias_bottom",
            RuleField::Expansion => "expansion",
            RuleField::Percent => "percent",
        }
    }

    /// Whether it is a preferred value, which supplies routing defaults rather than a
    /// limit.
    pub fn is_preferred(self) -> bool {
        matches!(
            self,
            RuleField::Preferred | RuleField::DiameterPreferred | RuleField::HolePreferred
        )
    }
}

/// One resolved value of a known rule field, serialized for previews. The field supplies
/// its unit; persisted input is deserialized through the typed [`RuleValues`] fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RuleValue {
    Length(Nm),
    Flag(bool),
    Percent(i32),
}

impl RuleValue {
    pub fn length(self) -> Option<Nm> {
        match self {
            RuleValue::Length(n) => Some(n),
            RuleValue::Flag(_) | RuleValue::Percent(_) => None,
        }
    }

    pub fn flag(self) -> Option<bool> {
        match self {
            RuleValue::Flag(b) => Some(b),
            RuleValue::Length(_) | RuleValue::Percent(_) => None,
        }
    }
}

/// A range of sizes: `min` and `max` are limits, `preferred` is what Tools start with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Nm>,
}

impl Bounds {
    fn is_empty(&self) -> bool {
        *self == Bounds::default()
    }
}

/// The values a rule record sets. Which ones a record may hold depends on its kind
/// ([`RuleKind::fields`]); every one left out is inherited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Nm>,
    /// Clearance only: Pads within one Footprint need no clearance from each other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_pad_to_pad: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Nm>,
    /// Via size only: the Via's copper diameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<Bounds>,
    /// Via size only: the Via's drilled hole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<Bounds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_hole_edge: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tent_vias_top: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tent_vias_bottom: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expansion: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Signed change in area in percent, strictly above -100.
    pub percent: Option<i32>,
}

impl RuleValues {
    /// The value at `field`, if this record sets it.
    pub fn get(&self, field: RuleField) -> Option<RuleValue> {
        let length = |n: Option<Nm>| n.map(RuleValue::Length);
        let diameter = self.diameter.unwrap_or_default();
        let hole = self.hole.unwrap_or_default();
        match field {
            RuleField::Value => length(self.value),
            RuleField::IgnorePadToPad => self.ignore_pad_to_pad.map(RuleValue::Flag),
            RuleField::Min => length(self.min),
            RuleField::Preferred => length(self.preferred),
            RuleField::Max => length(self.max),
            RuleField::DiameterMin => length(diameter.min),
            RuleField::DiameterPreferred => length(diameter.preferred),
            RuleField::DiameterMax => length(diameter.max),
            RuleField::HoleMin => length(hole.min),
            RuleField::HolePreferred => length(hole.preferred),
            RuleField::HoleMax => length(hole.max),
            RuleField::Top => length(self.top),
            RuleField::Bottom => length(self.bottom),
            RuleField::FromHoleEdge => self.from_hole_edge.map(RuleValue::Flag),
            RuleField::TentViasTop => self.tent_vias_top.map(RuleValue::Flag),
            RuleField::TentViasBottom => self.tent_vias_bottom.map(RuleValue::Flag),
            RuleField::Expansion => length(self.expansion),
            RuleField::Percent => self.percent.map(RuleValue::Percent),
        }
    }

    /// The length at `field`, if this record sets one there.
    pub fn length(&self, field: RuleField) -> Option<Nm> {
        self.get(field).and_then(RuleValue::length)
    }

    /// Sets or (with `None`) clears the value at `field`. A value of the wrong type for
    /// the field is refused.
    pub fn set(&mut self, field: RuleField, value: Option<RuleValue>) -> Result<(), String> {
        let wrong = || format!("`{}` takes a {}", field.path(), type_name(field));
        if matches!(
            field,
            RuleField::IgnorePadToPad
                | RuleField::FromHoleEdge
                | RuleField::TentViasTop
                | RuleField::TentViasBottom
        ) {
            let flag = match value {
                None => None,
                Some(RuleValue::Flag(b)) => Some(b),
                Some(_) => return Err(wrong()),
            };
            match field {
                RuleField::IgnorePadToPad => self.ignore_pad_to_pad = flag,
                RuleField::FromHoleEdge => self.from_hole_edge = flag,
                RuleField::TentViasTop => self.tent_vias_top = flag,
                RuleField::TentViasBottom => self.tent_vias_bottom = flag,
                _ => unreachable!(),
            }
            return Ok(());
        }
        if field == RuleField::Percent {
            self.percent = match value {
                None => None,
                Some(RuleValue::Percent(p)) => Some(p),
                Some(_) => return Err(wrong()),
            };
            return Ok(());
        }
        let n = match value {
            None => None,
            Some(RuleValue::Length(n)) => Some(n),
            Some(_) => return Err(wrong()),
        };
        let in_bounds = |bounds: &mut Option<Bounds>, pick: fn(&mut Bounds) -> &mut Option<Nm>| {
            let mut b = bounds.unwrap_or_default();
            *pick(&mut b) = n;
            *bounds = (!b.is_empty()).then_some(b);
        };
        match field {
            RuleField::Value => self.value = n,
            RuleField::Min => self.min = n,
            RuleField::Preferred => self.preferred = n,
            RuleField::Max => self.max = n,
            RuleField::DiameterMin => in_bounds(&mut self.diameter, |b| &mut b.min),
            RuleField::DiameterPreferred => in_bounds(&mut self.diameter, |b| &mut b.preferred),
            RuleField::DiameterMax => in_bounds(&mut self.diameter, |b| &mut b.max),
            RuleField::HoleMin => in_bounds(&mut self.hole, |b| &mut b.min),
            RuleField::HolePreferred => in_bounds(&mut self.hole, |b| &mut b.preferred),
            RuleField::HoleMax => in_bounds(&mut self.hole, |b| &mut b.max),
            RuleField::Top => self.top = n,
            RuleField::Bottom => self.bottom = n,
            RuleField::Expansion => self.expansion = n,
            _ => unreachable!("handled above"),
        }
        Ok(())
    }

    /// The fields this record sets.
    pub fn set_fields(&self) -> Vec<RuleField> {
        ALL_FIELDS
            .iter()
            .copied()
            .filter(|f| self.get(*f).is_some())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        *self == RuleValues::default()
    }
}

fn type_name(field: RuleField) -> &'static str {
    match field {
        RuleField::IgnorePadToPad
        | RuleField::FromHoleEdge
        | RuleField::TentViasTop
        | RuleField::TentViasBottom => "boolean",
        RuleField::Percent => "percentage",
        _ => "length",
    }
}

const ALL_FIELDS: [RuleField; 18] = [
    RuleField::Value,
    RuleField::IgnorePadToPad,
    RuleField::Min,
    RuleField::Preferred,
    RuleField::Max,
    RuleField::DiameterMin,
    RuleField::DiameterPreferred,
    RuleField::DiameterMax,
    RuleField::HoleMin,
    RuleField::HolePreferred,
    RuleField::HoleMax,
    RuleField::Top,
    RuleField::Bottom,
    RuleField::FromHoleEdge,
    RuleField::TentViasTop,
    RuleField::TentViasBottom,
    RuleField::Expansion,
    RuleField::Percent,
];

/// Where a rule record applies: filters that must all match. Empty is board-wide; v0.1
/// accepts only empty or `{net_class}`.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct RuleScope {
    /// The Net Class whose Nets it applies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_class: Option<Uuid>,
}

impl RuleScope {
    /// The board-wide scope.
    pub const BOARD: RuleScope = RuleScope { net_class: None };

    pub fn class(class: Uuid) -> RuleScope {
        RuleScope {
            net_class: Some(class),
        }
    }

    pub fn is_board_wide(&self) -> bool {
        self.net_class.is_none()
    }
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

/// One design rule record: the values it sets for one kind, within one scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesignRule {
    pub uuid: Uuid,
    pub kind: RuleKind,
    pub scope: RuleScope,
    pub values: RuleValues,
    /// Whether the record counts; left out when it does. A disabled board-wide limit is
    /// absent, but its preferred values still supply routing defaults.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// A note for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl DesignRule {
    /// What is structurally wrong with this record, if anything: values its kind does not
    /// hold, or a board-wide record missing a value its kind requires.
    pub fn problem(&self) -> Option<String> {
        let kind = self.kind.id();
        if self.kind == RuleKind::PasteExpansion {
            if self.values.percent.is_some_and(|p| p <= -100) {
                return Some("paste percent must be above -100".into());
            }
            if self.values.expansion.is_some() && self.values.percent.is_some() {
                return Some("paste takes expansion or percent, not both".into());
            }
            if self.scope.is_board_wide()
                && self.values.expansion.is_none()
                && self.values.percent.is_none()
            {
                return Some("board-wide paste needs expansion or percent".into());
            }
        }
        if let Some(field) = self
            .values
            .set_fields()
            .into_iter()
            .find(|f| !self.kind.fields().contains(f))
        {
            return Some(format!("a `{kind}` rule has no value `{}`", field.path()));
        }
        if !self.scope.is_board_wide() && self.values.is_empty() {
            return Some(format!("a Net Class `{kind}` rule sets no value"));
        }
        if self.scope.is_board_wide()
            && let Some(field) = self
                .kind
                .required()
                .iter()
                .find(|f| self.values.get(**f).is_none())
        {
            return Some(format!(
                "the board-wide `{kind}` rule needs a value `{}`",
                field.path()
            ));
        }
        None
    }
}

/// A Net Class as a [`RuleSet`] names it: by name only, since its Nets differ per board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortableClass {
    pub name: String,
}

/// A rule record as a [`RuleSet`] holds it: its Net Class by name, and no UUID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortableRule {
    pub kind: RuleKind,
    /// The Net Class it applies to, by name; left out for a board-wide record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_class: Option<String>,
    pub values: RuleValues,
    /// Whether the record counts; left out when it does.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// A board's rules made portable between boards (a `.redarules` file): the rule records,
/// the Net Class names they need, in rank order, and the DRC severities. It holds no Net
/// membership, because Net names differ per board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleSet {
    pub net_classes: Vec<PortableClass>,
    pub rules: Vec<PortableRule>,
    pub severities: crate::DrcSeverities,
}
