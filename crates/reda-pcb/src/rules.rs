//! The one rule resolver: which design rule value applies to copper on a Net. Commands,
//! DRC, Zone fill and the Track Tool all ask here.
//!
//! - For one Net, value by value, the highest-ranked Net Class on the Net with an enabled
//!   record that sets the value wins; otherwise the board-wide record applies. Copper on
//!   no Net uses the board-wide record.
//! - A disabled board-wide limit is absent, so it is not checked, but its preferred values
//!   still supply routing defaults.
//! - Between two objects, each one's value is resolved and the larger applies.

use reda_model::geom::Nm;
use reda_model::{DesignRule, PcbDocument, RuleField, RuleKind, RuleValue, RuleValues, Uuid};

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

    /// The size a Tool or Command starts with: the preferred size, clipped into the range.
    pub fn default_size(&self) -> Nm {
        self.clip(self.preferred)
    }
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
            if values.get(field).is_none() {
                values
                    .set(field, board.values.get(field))
                    .expect("the same field takes the same type");
            }
        }
    }
    values
}

/// The blocking problems of `record`, judged on its effective values: a negative value,
/// `min` above `max`, or `preferred` outside the range, with `board` the board-wide
/// record of its kind. Each is `(field, message)`.
pub fn blocking_problems(board: &DesignRule, record: &DesignRule) -> Vec<(RuleField, String)> {
    use RuleField::*;
    let values = effective_values(board, record);
    let mut problems = Vec::new();
    for &field in record.kind.fields() {
        if let Some(n) = values.length(field)
            && n < Nm::ZERO
        {
            problems.push((field, format!("`{}` cannot be negative", field.path())));
        }
    }
    let triples: &[[RuleField; 3]] = match record.kind {
        RuleKind::TrackWidth | RuleKind::HoleSize => &[[Min, Preferred, Max]],
        RuleKind::ViaSize => &[
            [DiameterMin, DiameterPreferred, DiameterMax],
            [HoleMin, HolePreferred, HoleMax],
        ],
        _ => &[],
    };
    for &[min, preferred, max] in triples {
        let (lo, mid, hi) = (
            values.length(min),
            values.length(preferred),
            values.length(max),
        );
        if let (Some(lo), Some(hi)) = (lo, hi)
            && lo > hi
        {
            problems.push((min, format!("`{}` is above `{}`", min.path(), max.path())));
        }
        if let Some(mid) = mid
            && (lo.is_some_and(|lo| mid < lo) || hi.is_some_and(|hi| mid > hi))
        {
            problems.push((
                preferred,
                format!("`{}` is outside the range", preferred.path()),
            ));
        }
    }
    problems
}
