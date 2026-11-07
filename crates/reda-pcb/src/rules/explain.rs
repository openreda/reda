//! Explaining the rules: which records apply to an object and why
//! (`pcb.rules.applicable`), and which objects a record or Net Class affects
//! (`pcb.rules.affected`). Both read [`super::resolve`], so they agree with DRC.

use reda_model::{
    DesignRule, DrcCheck, PadRef, PcbDocument, RuleField, RuleKind, RuleValue, Severity, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;

use super::{resolve, resolve_pair};

/// The DRC check that reports a kind's limits; `None` for kinds that shape outputs or
/// fills.
pub fn check_of(kind: RuleKind) -> Option<DrcCheck> {
    match kind {
        RuleKind::Clearance => Some(DrcCheck::Clearance),
        RuleKind::TrackWidth => Some(DrcCheck::TrackWidth),
        RuleKind::ViaSize => Some(DrcCheck::ViaSize),
        RuleKind::HoleSize => Some(DrcCheck::HoleSize),
        RuleKind::AnnularRing => Some(DrcCheck::AnnularRing),
        RuleKind::HoleToHole => Some(DrcCheck::HoleToHole),
        RuleKind::EdgeClearance => Some(DrcCheck::EdgeClearance),
        RuleKind::SilkToEdge => Some(DrcCheck::SilkToEdge),
        RuleKind::SilkToMask => Some(DrcCheck::SilkToMask),
        RuleKind::CourtyardClearance => Some(DrcCheck::CourtyardOverlap),
        RuleKind::Dangling => Some(DrcCheck::Dangling),
        RuleKind::MicroSegment => Some(DrcCheck::MicroSegment),
        RuleKind::DeadCopper => Some(DrcCheck::DeadCopper),
        RuleKind::ZoneConnection | RuleKind::MaskExpansion | RuleKind::PasteExpansion => None,
    }
}

/// A value that applies, and the record it comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Winner {
    /// Which value, such as `diameter.min`.
    pub field: String,
    pub value: RuleValue,
    pub rule: Uuid,
    /// The record's Net Class; left out for the board-wide record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class: Option<Uuid>,
}

/// A record that sets values another record wins, or a disabled record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Loser {
    pub rule: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class: Option<Uuid>,
    /// The values it sets that lost; empty for a disabled record.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}

/// One kind's records as they apply to one object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ObjectRules {
    /// The winning values, in field order.
    pub values: Vec<Winner>,
    /// Enabled records that set a value another record wins.
    pub losers: Vec<Loser>,
    /// Disabled records on the object's Net Classes, and a disabled board-wide record.
    pub disabled: Vec<Loser>,
}

/// Which of two objects' values applies between them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Between {
    pub field: String,
    pub value: RuleValue,
    pub rule: Uuid,
    /// The object whose value applied: 0 or 1. Equal values name the first.
    pub object: usize,
}

/// One kind of rule as it applies to the objects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct KindRules {
    pub kind: RuleKind,
    /// The severity of the DRC check that reports it; left out for kinds that shape
    /// outputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
    /// One entry per object, in the order given.
    pub objects: Vec<ObjectRules>,
    /// For two objects and a kind that applies between them (`clearance`,
    /// `hole_to_hole`): the larger value, which applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub between: Option<Between>,
}

/// The rules for copper on each of the Nets `nets` (one or two), kind by kind.
pub fn applicable(pcb: &PcbDocument, nets: &[Option<Uuid>]) -> Vec<KindRules> {
    RuleKind::ALL
        .into_iter()
        .map(|kind| {
            let pair_field = match kind {
                RuleKind::Clearance => Some(RuleField::Value),
                RuleKind::HoleToHole => Some(RuleField::Min),
                _ => None,
            };
            let between = match (nets, pair_field) {
                ([a, b], Some(field)) => resolve_pair(pcb, *a, *b, kind, field).map(|s| {
                    let first = resolve(pcb, *a, kind, field);
                    Between {
                        field: field.path().to_owned(),
                        value: s.value,
                        rule: s.rule,
                        object: if first == Some(s) { 0 } else { 1 },
                    }
                }),
                _ => None,
            };
            KindRules {
                kind,
                severity: check_of(kind).map(|c| pcb.setup().severities.get(c)),
                objects: nets
                    .iter()
                    .map(|net| object_rules(pcb, *net, kind))
                    .collect(),
                between,
            }
        })
        .collect()
}

/// The records of `kind` that can apply to copper on `net`: its classes' records, highest
/// rank first, then the board-wide record.
fn candidates(pcb: &PcbDocument, net: Option<Uuid>, kind: RuleKind) -> Vec<&DesignRule> {
    super::ranked_classes(pcb, net)
        .into_iter()
        .filter_map(|c| pcb.class_rule(kind, c))
        .chain([pcb.board_rule(kind)])
        .collect()
}

fn object_rules(pcb: &PcbDocument, net: Option<Uuid>, kind: RuleKind) -> ObjectRules {
    let mut values = Vec::new();
    let mut losers: Vec<Loser> = Vec::new();
    let records = candidates(pcb, net, kind);
    for &field in kind.fields() {
        let winner = resolve(pcb, net, kind, field);
        if let Some(w) = winner {
            values.push(Winner {
                field: field.path().to_owned(),
                value: w.value,
                rule: w.rule,
                net_class: pcb.rule(w.rule).and_then(|r| r.scope.net_class),
            });
        }
        for record in records.iter().filter(|r| r.enabled) {
            if record.values.get(field).is_none() || winner.is_some_and(|w| w.rule == record.uuid) {
                continue;
            }
            match losers.iter_mut().find(|l| l.rule == record.uuid) {
                Some(l) => l.fields.push(field.path().to_owned()),
                None => losers.push(Loser {
                    rule: record.uuid,
                    net_class: record.scope.net_class,
                    fields: vec![field.path().to_owned()],
                }),
            }
        }
    }
    let disabled = records
        .iter()
        .filter(|r| !r.enabled)
        .map(|r| Loser {
            rule: r.uuid,
            net_class: r.scope.net_class,
            fields: Vec::new(),
        })
        .collect();
    ObjectRules {
        values,
        losers,
        disabled,
    }
}

/// What a record or Net Class affects.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Affected {
    /// The Nets it supplies a value to, in Net order.
    pub nets: Vec<Uuid>,
    /// The Tracks, Vias, Free Pads and Zones on them, in that order; for a board-wide
    /// record also those on no Net.
    pub objects: Vec<Uuid>,
    /// The Pads of Footprint instances on them.
    pub pads: Vec<PadRef>,
}

/// The copper whose resolved values of some field come from one of `records`: objects
/// another record overrides in every value are left out.
pub fn affected(pcb: &PcbDocument, records: &[&DesignRule]) -> Affected {
    let supplies = |net: Option<Uuid>| {
        records.iter().any(|record| {
            record.kind.fields().iter().any(|&field| {
                resolve(pcb, net, record.kind, field).is_some_and(|s| s.rule == record.uuid)
            })
        })
    };
    let nets: Vec<Uuid> = pcb
        .nets()
        .map(|n| n.uuid)
        .filter(|n| supplies(Some(*n)))
        .collect();
    let no_net = supplies(None);
    let hit = |net: Option<Uuid>| match net {
        Some(n) => nets.contains(&n),
        None => no_net,
    };
    let mut objects = Vec::new();
    objects.extend(pcb.tracks().filter(|t| hit(t.net)).map(|t| t.uuid));
    objects.extend(pcb.vias().filter(|v| hit(v.net)).map(|v| v.uuid));
    objects.extend(pcb.free_pads().filter(|p| hit(p.net)).map(|p| p.uuid));
    objects.extend(pcb.zones().filter(|z| hit(z.net)).map(|z| z.uuid));
    let mut pads = Vec::new();
    for instance in pcb.footprints() {
        for pad in crate::placed::placed_pads(pcb, instance) {
            if hit(pad.net) {
                pads.push(PadRef {
                    footprint: instance.uuid,
                    pad: pad.pad,
                });
            }
        }
    }
    Affected {
        nets,
        objects,
        pads,
    }
}
