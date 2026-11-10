//! The one rule resolver: which design rule value applies to copper on a Net. Commands,
//! DRC, Zone fill and the Track Tool all ask here.
//!
//! - For one Net, value by value, the highest-ranked Net Class on the Net with an enabled
//!   record that sets the value wins; otherwise the board-wide record applies. Copper on
//!   no Net uses the board-wide record.
//! - A disabled board-wide limit is absent, so it is not checked, but its preferred values
//!   still supply routing defaults.
//! - Between two objects, each one's value is resolved and the larger applies.

pub mod explain;
pub mod presets;
pub mod transfer;
pub mod validation;

use reda_model::geom::{Millideg, Nm};
use reda_model::{
    ConnectionStyle, ConnectionTarget, ConnectionValue, DesignRule, Limit, PasteExpansion,
    PcbDocument, RuleField, RuleKind, RuleValue, RuleValues, Uuid, ZoneConnection,
};
use schemars::JsonSchema;
use serde::Serialize;

/// A resolved value and the rule record it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sourced<T> {
    pub value: T,
    pub rule: Uuid,
}

/// The Net Classes of `net`, highest rank first; none for no Net or an unknown one.
pub fn ranked_classes(pcb: &PcbDocument, net: Option<Uuid>) -> Vec<Uuid> {
    let Some(net) = net.and_then(|n| pcb.net(n)) else {
        return Vec::new();
    };
    let mut classes: Vec<(u32, Uuid)> = net
        .classes
        .iter()
        .filter_map(|c| pcb.net_class(*c).map(|class| (class.rank, class.uuid)))
        .collect();
    classes.sort();
    classes.into_iter().map(|(_, c)| c).collect()
}

/// The value of `kind`'s `field` for copper on `net`, and the record that sets it; `None`
/// where the value is absent (a disabled board-wide limit, or an optional value no record
/// sets).
pub fn resolve(
    pcb: &PcbDocument,
    net: Option<Uuid>,
    kind: RuleKind,
    field: RuleField,
) -> Option<Sourced<RuleValue>> {
    let from_class = ranked_classes(pcb, net).into_iter().find_map(|class| {
        let rule = pcb.class_rule(kind, class).filter(|r| r.enabled)?;
        rule.values.get(field).map(|value| Sourced {
            value,
            rule: rule.uuid,
        })
    });
    from_class.or_else(|| {
        let rule = pcb.board_rule(kind);
        (rule.enabled || field.is_preferred())
            .then(|| rule.values.get(field))
            .flatten()
            .map(|value| Sourced {
                value,
                rule: rule.uuid,
            })
    })
}

/// The length limit of `kind`'s `field` for copper on `net`; `None` where it is absent.
pub fn limit(pcb: &PcbDocument, net: Option<Uuid>, kind: RuleKind, field: RuleField) -> Option<Nm> {
    resolve(pcb, net, kind, field).and_then(|s| s.value.length())
}

/// The paste expansion for SMD copper on `net`. `expansion` and `percent` are one
/// exclusive pair, so they resolve together rather than value by value: the
/// highest-ranked Net Class with an enabled record that sets either wins, otherwise the
/// board-wide record; a disabled board-wide record means no expansion.
pub fn paste_expansion(pcb: &PcbDocument, net: Option<Uuid>) -> PasteExpansion {
    let sets = |r: &&DesignRule| r.values.expansion.is_some() || r.values.percent.is_some();
    let record = ranked_classes(pcb, net)
        .into_iter()
        .find_map(|c| {
            pcb.class_rule(RuleKind::PasteExpansion, c)
                .filter(|r| r.enabled)
                .filter(sets)
        })
        .or_else(|| Some(pcb.board_rule(RuleKind::PasteExpansion)).filter(|r| r.enabled));
    match record.map(|r| (r.values.expansion, r.values.percent)) {
        Some((_, Some(percent))) => PasteExpansion::Percent { percent },
        Some((expansion, None)) => PasteExpansion::Expansion {
            expansion: expansion.unwrap_or(Nm::ZERO),
        },
        None => PasteExpansion::Expansion {
            expansion: Nm::ZERO,
        },
    }
}

/// A size range: limits that may be absent, and the preferred size Tools start with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeRange {
    pub min: Option<Nm>,
    pub preferred: Nm,
    pub max: Option<Nm>,
}

impl SizeRange {
    /// `size` brought inside the limits. Where the limits cross (two classes that combine
    /// into an invalid range), the minimum wins.
    pub fn clip(&self, size: Nm) -> Nm {
        let size = self.max.map_or(size, |max| size.min(max));
        self.min.map_or(size, |min| size.max(min))
    }

    /// Which limit `size` breaks, and that limit's value: the minimum before the maximum.
    pub fn breach(&self, size: Nm) -> Option<(Limit, Nm)> {
        breach(self.min, self.max, size)
    }

    /// The size a Tool or Command starts with: the preferred size, clipped into the range.
    pub fn default_size(&self) -> Nm {
        self.clip(self.preferred)
    }
}

/// Which of the limits `min` and `max` the size breaks, and that limit's value: below the
/// minimum before above the maximum.
pub fn breach(min: Option<Nm>, max: Option<Nm>, size: Nm) -> Option<(Limit, Nm)> {
    if let Some(min) = min.filter(|min| size < *min) {
        return Some((Limit::Min, min));
    }
    max.filter(|max| size > *max).map(|max| (Limit::Max, max))
}

fn range(
    pcb: &PcbDocument,
    net: Option<Uuid>,
    kind: RuleKind,
    [min, preferred, max]: [RuleField; 3],
) -> SizeRange {
    SizeRange {
        min: limit(pcb, net, kind, min),
        preferred: resolve(pcb, net, kind, preferred)
            .and_then(|s| s.value.length())
            .expect("the board-wide record holds every preferred value"),
        max: limit(pcb, net, kind, max),
    }
}

/// Every rule value that applies to copper on one Net.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetRules {
    /// Clearance to copper of other Nets.
    pub clearance: Option<Nm>,
    /// Whether Pads within one Footprint need no clearance from each other.
    pub ignore_pad_to_pad: bool,
    pub track_width: SizeRange,
    pub via_diameter: SizeRange,
    pub via_hole: SizeRange,
    /// Pad, Free Pad and non-plated hole limits.
    pub hole_min: Option<Nm>,
    pub hole_max: Option<Nm>,
    pub annular_ring: Option<Nm>,
    pub hole_to_hole: Option<Nm>,
    pub edge_clearance: Option<Nm>,
}

/// The rules for copper on `net` (or on no Net, which takes the board-wide records).
pub fn net_rules(pcb: &PcbDocument, net: Option<Uuid>) -> NetRules {
    use RuleField::*;
    NetRules {
        clearance: limit(pcb, net, RuleKind::Clearance, Value),
        ignore_pad_to_pad: resolve(pcb, net, RuleKind::Clearance, IgnorePadToPad)
            .and_then(|s| s.value.flag())
            .unwrap_or(false),
        track_width: range(pcb, net, RuleKind::TrackWidth, [Min, Preferred, Max]),
        via_diameter: range(
            pcb,
            net,
            RuleKind::ViaSize,
            [DiameterMin, DiameterPreferred, DiameterMax],
        ),
        via_hole: range(
            pcb,
            net,
            RuleKind::ViaSize,
            [HoleMin, HolePreferred, HoleMax],
        ),
        hole_min: limit(pcb, net, RuleKind::HoleSize, Min),
        hole_max: limit(pcb, net, RuleKind::HoleSize, Max),
        annular_ring: limit(pcb, net, RuleKind::AnnularRing, Min),
        hole_to_hole: limit(pcb, net, RuleKind::HoleToHole, Min),
        edge_clearance: limit(pcb, net, RuleKind::EdgeClearance, Value),
    }
}

/// A Zone connection with every value resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connection {
    pub style: ConnectionStyle,
    /// 2 or 4.
    pub spokes: u8,
    pub spoke_width: Nm,
    /// The clear ring around a relieved or unconnected object.
    pub gap: Nm,
    /// 0, 45 or 90 degrees in the object's frame.
    pub angle: Millideg,
}

/// How a fill of `net` joins a `target` object, value by value: the object's own value
/// (`own`, a Pad's `zone_connection`), then the highest-ranked Net Class record that sets
/// it, then the board-wide record. The record only shapes the fill, so a disabled
/// board-wide record still supplies its values.
pub fn zone_connection(
    pcb: &PcbDocument,
    net: Option<Uuid>,
    target: ConnectionTarget,
    own: Option<&ZoneConnection>,
) -> Connection {
    let value = |which: ConnectionValue| {
        let field = RuleField::connection_field(target, which);
        resolve(pcb, net, RuleKind::ZoneConnection, field)
            .map(|s| s.value)
            .or_else(|| pcb.board_rule(RuleKind::ZoneConnection).values.get(field))
    };
    let seed = match target {
        ConnectionTarget::Via => crate::VIA_CONNECTION_SEED,
        _ => crate::PAD_CONNECTION_SEED,
    };
    let own = own.copied().unwrap_or_default();
    Connection {
        style: own
            .style
            .or_else(|| match value(ConnectionValue::Style) {
                Some(RuleValue::Style(s)) => Some(s),
                _ => None,
            })
            .or(seed.style)
            .expect("seeded"),
        spokes: own
            .spokes
            .or_else(|| match value(ConnectionValue::Spokes) {
                Some(RuleValue::Count(n)) => Some(n),
                _ => None,
            })
            .or(seed.spokes)
            .expect("seeded"),
        spoke_width: own
            .spoke_width
            .or_else(|| value(ConnectionValue::SpokeWidth).and_then(RuleValue::length))
            .or(seed.spoke_width)
            .expect("seeded"),
        gap: own
            .gap
            .or_else(|| value(ConnectionValue::Gap).and_then(RuleValue::length))
            .or(seed.gap)
            .expect("seeded"),
        angle: own
            .angle
            .or_else(|| match value(ConnectionValue::Angle) {
                Some(RuleValue::Angle(a)) => Some(a),
                _ => None,
            })
            .or(seed.angle)
            .expect("seeded"),
    }
}

/// The larger of two values that may be absent.
fn larger(a: Option<Nm>, b: Option<Nm>) -> Option<Nm> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// The clearance required between copper on Nets `a` and `b`: the larger of their
/// values; `None` where neither has one.
pub fn clearance_between(pcb: &PcbDocument, a: Option<Uuid>, b: Option<Uuid>) -> Option<Nm> {
    larger(net_rules(pcb, a).clearance, net_rules(pcb, b).clearance)
}

/// The hole-to-hole spacing required between holes on Nets `a` and `b`: the larger.
pub fn hole_to_hole_between(pcb: &PcbDocument, a: Option<Uuid>, b: Option<Uuid>) -> Option<Nm> {
    larger(
        net_rules(pcb, a).hole_to_hole,
        net_rules(pcb, b).hole_to_hole,
    )
}

/// The largest value any enabled record sets for `kind`'s `field`, 0 for none: how far a
/// search for neighbours must reach.
pub fn largest(pcb: &PcbDocument, kind: RuleKind, field: RuleField) -> Nm {
    pcb.rules()
        .filter(|r| r.kind == kind && r.enabled)
        .filter_map(|r| r.values.length(field))
        .max()
        .unwrap_or(Nm::ZERO)
}

/// The values of `record` as they apply: its own, and for a Net Class record those of
/// `board` (the board-wide record of its kind) where it sets none.
pub fn effective_values(board: &DesignRule, record: &DesignRule) -> RuleValues {
    let mut values = record.values;
    if !record.scope.is_board_wide() {
        for &field in record.kind.fields() {
            if record.kind == RuleKind::PasteExpansion && !record.values.is_empty() {
                continue;
            }
            if values.get(field).is_none() {
                values
                    .set(field, board.values.get(field))
                    .expect("the same field takes the same type");
            }
        }
    }
    values
}

/// What makes a rule record's values unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    /// A negative length.
    Negative,
    /// `min` above `max`.
    MinAboveMax,
    /// `preferred` outside the range.
    PreferredOutside,
}

/// One blocking problem of a rule record: which value, and what is wrong with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockingProblem {
    pub field: RuleField,
    pub problem: ProblemKind,
}

impl BlockingProblem {
    /// The problem in words, such as "`min` is above `max`".
    pub fn message(&self) -> String {
        let path = self.field.path();
        match self.problem {
            ProblemKind::Negative => format!("`{path}` cannot be negative"),
            ProblemKind::MinAboveMax => {
                let max = path.replace("min", "max");
                format!("`{path}` is above `{max}`")
            }
            ProblemKind::PreferredOutside => format!("`{path}` is outside the range"),
        }
    }
}

/// The `[min, preferred, max]` triples of a kind; `hole_size` has no preferred value.
pub(crate) fn triples(kind: RuleKind) -> &'static [[Option<RuleField>; 3]] {
    use RuleField::*;
    match kind {
        RuleKind::TrackWidth => &[[Some(Min), Some(Preferred), Some(Max)]],
        RuleKind::HoleSize => &[[Some(Min), None, Some(Max)]],
        RuleKind::ViaSize => &[
            [
                Some(DiameterMin),
                Some(DiameterPreferred),
                Some(DiameterMax),
            ],
            [Some(HoleMin), Some(HolePreferred), Some(HoleMax)],
        ],
        _ => &[],
    }
}

/// The problems of the range `lo..=hi` holding `mid`, each named by its field.
pub(crate) fn range_problems(
    [min, preferred, max]: [Option<RuleField>; 3],
    get: impl Fn(RuleField) -> Option<Nm>,
) -> Vec<BlockingProblem> {
    let mut problems = Vec::new();
    let (lo, hi) = (min.and_then(&get), max.and_then(&get));
    if let (Some(field), Some(lo), Some(hi)) = (min, lo, hi)
        && lo > hi
    {
        problems.push(BlockingProblem {
            field,
            problem: ProblemKind::MinAboveMax,
        });
    }
    if let Some(field) = preferred
        && let Some(mid) = get(field)
        && (lo.is_some_and(|lo| mid < lo) || hi.is_some_and(|hi| mid > hi))
    {
        problems.push(BlockingProblem {
            field,
            problem: ProblemKind::PreferredOutside,
        });
    }
    problems
}

/// The blocking problems of `record`, judged on its effective values: a negative value,
/// `min` above `max`, or `preferred` outside the range, with `board` the board-wide
/// record of its kind. Negative values come first, then each range's.
pub fn blocking_problems(board: &DesignRule, record: &DesignRule) -> Vec<BlockingProblem> {
    let values = effective_values(board, record);
    let mut problems = Vec::new();
    for &field in record.kind.fields() {
        if !matches!(
            record.kind,
            RuleKind::MaskExpansion | RuleKind::PasteExpansion
        ) && let Some(n) = values.length(field)
            && n < Nm::ZERO
        {
            problems.push(BlockingProblem {
                field,
                problem: ProblemKind::Negative,
            });
        }
    }
    for &triple in triples(record.kind) {
        problems.extend(range_problems(triple, |f| values.length(f)));
    }
    problems
}

/// The larger limit between two Nets. Equal values name the first Net's record.
pub fn resolve_pair(
    pcb: &PcbDocument,
    a: Option<Uuid>,
    b: Option<Uuid>,
    kind: RuleKind,
    field: RuleField,
) -> Option<Sourced<RuleValue>> {
    match (resolve(pcb, a, kind, field), resolve(pcb, b, kind, field)) {
        (Some(a), Some(b)) => Some(if a.value.length() >= b.value.length() {
            a
        } else {
            b
        }),
        (a, b) => a.or(b),
    }
}
