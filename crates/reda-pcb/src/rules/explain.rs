//! Explaining the rules: which records apply to an object and why
//! (`pcb.rules.applicable`), and which objects a record or Net Class affects
//! (`pcb.rules.affected`). Both read [`super::resolve`], so they agree with DRC.

use reda_model::{
    DesignRule, DrcCheck, PadRef, PcbDocument, RuleField, RuleKind, RuleValue, Severity, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;

use super::{resolve_on, resolve_pair_on};

/// The DRC check that reports a kind's limits; `None` for kinds that shape outputs or
/// fills.
pub fn check_of(kind: RuleKind) -> Option<DrcCheck> {
    match kind {
        RuleKind::Clearance => Some(DrcCheck::Clearance),
        RuleKind::TrackWidth => Some(DrcCheck::TrackWidth),
        RuleKind::ViaSize => Some(DrcCheck::ViaSize),
        RuleKind::ViaSpan => Some(DrcCheck::ViaSpan),
        RuleKind::MicroviaSize => Some(DrcCheck::MicroviaSize),
        RuleKind::Teardrop => None,
        RuleKind::SilkWidth => Some(DrcCheck::SilkWidth),
        RuleKind::SilkTextHeight => Some(DrcCheck::SilkTextHeight),

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<reda_model::RuleLayers>,
}

/// A record that sets values another record wins, or a disabled record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Loser {
    pub rule: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<reda_model::RuleLayers>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class_pair: Option<[Uuid; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<reda_model::RuleLayers>,
}

/// One kind of rule as it applies to the objects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct KindRules {
    pub kind: RuleKind,
    /// Present when this kind resolves differently across the selected copper Layers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Uuid>,
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
    applicable_on(pcb, nets, &vec![None; nets.len()], true)
}

/// Selected copper and the Layers where it is measured.
pub struct Measured {
    pub net: Option<Uuid>,
    pub layers: Vec<Uuid>,
}

impl Measured {
    pub fn object(pcb: &PcbDocument, id: Uuid) -> Option<Self> {
        if let Some(t) = pcb.track(id) {
            return Some(Self {
                net: t.net,
                layers: vec![t.layer],
            });
        }
        if let Some(z) = pcb.zone(id) {
            return Some(Self {
                net: z.net,
                layers: vec![z.layer],
            });
        }
        if let Some(v) = pcb.via(id) {
            return Some(Self {
                net: v.net,
                layers: pcb.span_layers(v.span),
            });
        }
        if let Some(p) = pcb.free_pad(id) {
            return Some(Self::pad(pcb, &crate::placed::placed_free_pad(p)));
        }
        pcb.net(id).map(|n| Self {
            net: Some(n.uuid),
            layers: pcb.copper_layers().map(|l| l.uuid).collect(),
        })
    }

    pub fn pad(pcb: &PcbDocument, p: &crate::placed::PlacedPad) -> Self {
        Self {
            net: p.net,
            layers: pcb
                .copper_layers()
                .filter(|l| p.side.is_none() || p.side == l.side)
                .map(|l| l.uuid)
                .collect(),
        }
    }

    pub fn placed(pcb: &PcbDocument, pad: PadRef) -> Option<Self> {
        let instance = pcb.footprint(pad.footprint)?;
        crate::placed::placed_pads(pcb, instance)
            .iter()
            .find(|p| p.pad == pad.pad)
            .map(|p| Self::pad(pcb, p))
    }
}

/// Groups identical explanations across shared copper Layers. Objects with no common
/// Layer retain their own winners and have no between-objects measurement.
pub fn applicable_objects(pcb: &PcbDocument, objects: &[Measured]) -> Vec<KindRules> {
    let nets: Vec<_> = objects.iter().map(|o| o.net).collect();
    let common: Vec<_> = pcb
        .copper_layers()
        .map(|l| l.uuid)
        .filter(|id| objects.iter().all(|o| o.layers.contains(id)))
        .collect();
    if common.is_empty() {
        let layers: Vec<_> = objects.iter().map(|o| o.layers.first().copied()).collect();
        return applicable_on(pcb, &nets, &layers, false);
    }
    let mut groups: Vec<(KindRules, Vec<Uuid>)> = Vec::new();
    for &layer in &common {
        for result in applicable_on(pcb, &nets, &vec![Some(layer); nets.len()], true) {
            if let Some((_, layers)) = groups.iter_mut().find(|(r, _)| r == &result) {
                layers.push(layer);
            } else {
                groups.push((result, vec![layer]));
            }
        }
    }
    groups.sort_by_key(|(r, _)| r.kind);
    groups
        .into_iter()
        .map(|(mut r, layers)| {
            if layers.len() != common.len() {
                r.layers = layers;
            }
            r
        })
        .collect()
}

fn applicable_on(
    pcb: &PcbDocument,
    nets: &[Option<Uuid>],
    layers: &[Option<Uuid>],
    shared: bool,
) -> Vec<KindRules> {
    RuleKind::ALL
        .into_iter()
        .map(|kind| {
            let pair_field = match kind {
                RuleKind::Clearance => Some(RuleField::Value),
                RuleKind::HoleToHole => Some(RuleField::Min),
                _ => None,
            };
            let between = match (nets, pair_field) {
                ([a, b], Some(field)) if shared => {
                    resolve_pair_on(pcb, *a, *b, layers[0], kind, field).map(|s| {
                        let first = resolve_on(pcb, *a, layers[0], kind, field);
                        let pair = pcb.rule(s.rule).and_then(|r| r.scope.net_class_pair);
                        Between {
                            field: field.path().to_owned(),
                            value: s.value,
                            rule: s.rule,
                            object: pair.is_none().then_some(usize::from(first != Some(s))),
                            net_class_pair: pair,
                            layers: pcb.rule(s.rule).and_then(|r| r.scope.layers.clone()),
                        }
                    })
                }
                _ => None,
            };
            KindRules {
                kind,
                layers: Vec::new(),
                severity: check_of(kind).map(|c| pcb.setup().severities.get(c)),
                objects: nets
                    .iter()
                    .zip(layers)
                    .map(|(net, layer)| object_rules(pcb, *net, *layer, kind))
                    .collect(),
                between,
            }
        })
        .collect()
}

fn object_rules(
    pcb: &PcbDocument,
    net: Option<Uuid>,
    layer: Option<Uuid>,
    kind: RuleKind,
) -> ObjectRules {
    let mut values = Vec::new();
    let mut losers: Vec<Loser> = Vec::new();
    let records = super::candidates(pcb, &super::ranked_classes(pcb, net), layer, kind);
    for &field in kind.fields() {
        let winner = resolve_on(pcb, net, layer, kind, field);
        if let Some(w) = winner {
            values.push(Winner {
                field: field.path().to_owned(),
                value: w.value,
                rule: w.rule,
                net_class: pcb.rule(w.rule).and_then(|r| r.scope.net_class),
                layers: pcb.rule(w.rule).and_then(|r| r.scope.layers.clone()),
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
                    layers: record.scope.layers.clone(),
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
            layers: r.scope.layers.clone(),
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
    let mut cache = std::collections::BTreeMap::new();
    let mut supplies = |net: Option<Uuid>, layer: Uuid| {
        *cache.entry((net, layer)).or_insert_with(|| {
            records.iter().any(|record| {
                record.kind.fields().iter().any(|&field| {
                    if record.scope.net_class_pair.is_some() {
                        pcb.nets()
                            .filter(|other| Some(other.uuid) != net)
                            .any(|other| {
                                resolve_pair_on(
                                    pcb,
                                    net,
                                    Some(other.uuid),
                                    Some(layer),
                                    record.kind,
                                    field,
                                )
                                .is_some_and(|s| s.rule == record.uuid)
                            })
                    } else {
                        resolve_on(pcb, net, Some(layer), record.kind, field)
                            .is_some_and(|s| s.rule == record.uuid)
                    }
                })
            })
        })
    };
    let nets = pcb
        .nets()
        .filter(|n| pcb.copper_layers().any(|l| supplies(Some(n.uuid), l.uuid)))
        .map(|n| n.uuid)
        .collect();
    let mut hit = |m: Measured| m.layers.into_iter().any(|l| supplies(m.net, l));
    let objects = pcb
        .tracks()
        .map(|t| t.uuid)
        .chain(pcb.vias().map(|v| v.uuid))
        .chain(pcb.free_pads().map(|p| p.uuid))
        .chain(pcb.zones().map(|z| z.uuid))
        .filter(|id| Measured::object(pcb, *id).is_some_and(&mut hit))
        .collect();
    let mut pads = Vec::new();
    for instance in pcb.footprints() {
        for pad in crate::placed::placed_pads(pcb, instance) {
            if hit(Measured::pad(pcb, &pad)) {
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
