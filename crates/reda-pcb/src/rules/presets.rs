//! Comparing the rules with Fab Presets: the flat limits of a preset against the rule
//! records, for `rule_outside_preset` (DRC and an Output Job run) and validation notes.
//! Only limits are compared, so a preferred value never produces a finding.

use reda_model::geom::Nm;
use reda_model::{
    DesignRule, FabRules, Limit, PcbDocument, Project, RuleField, RuleKind, Severity, Violation,
};

/// The limits a preset sets on rule values: kind, value, which way the value may not go,
/// and the preset's limit.
pub fn preset_limits(rules: &FabRules) -> Vec<(RuleKind, RuleField, Limit, Nm)> {
    use RuleField::*;
    let mut limits = vec![
        (RuleKind::Clearance, Value, Limit::Min, rules.clearance),
        (RuleKind::TrackWidth, Min, Limit::Min, rules.track_width),
        (
            RuleKind::ViaSize,
            DiameterMin,
            Limit::Min,
            rules.via_diameter,
        ),
        (RuleKind::ViaSize, HoleMin, Limit::Min, rules.via_hole),
        (RuleKind::HoleSize, Min, Limit::Min, rules.hole_min),
        (RuleKind::HoleSize, Max, Limit::Max, rules.hole_max),
        (RuleKind::AnnularRing, Min, Limit::Min, rules.annular_ring),
        (RuleKind::HoleToHole, Min, Limit::Min, rules.hole_to_hole),
        (
            RuleKind::EdgeClearance,
            Value,
            Limit::Min,
            rules.edge_clearance,
        ),
    ];
    for (kind, value) in [
        (RuleKind::SilkWidth, rules.silk_width),
        (RuleKind::SilkTextHeight, rules.silk_text_height),
    ] {
        if let Some(min) = value {
            limits.push((kind, Min, Limit::Min, min));
        }
    }
    limits
}

/// A Fab Preset the rules are compared with: its name and its limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetRules {
    pub name: String,
    pub rules: FabRules,
}

/// The Fab Presets of the Output Jobs that plot `pcb`, each once, in job order.
pub fn job_presets(project: &Project, pcb: &PcbDocument) -> Vec<PresetRules> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for job in project.output_jobs().filter(|j| j.pcb == pcb.uuid) {
        if seen.contains(&job.fab_preset) {
            continue;
        }
        seen.push(job.fab_preset);
        if let Some(preset) = project.fab_preset(job.fab_preset) {
            out.push(PresetRules {
                name: preset.name.clone(),
                rules: preset.rules,
            });
        }
    }
    out
}

/// The strictest limit `presets` set on one rule value: the largest minimum or the smallest
/// maximum, with the preset that sets it (the first, on a tie). `None` when no preset limits
/// that value.
pub fn strictest_limit(
    presets: &[PresetRules],
    kind: RuleKind,
    field: RuleField,
) -> Option<(Limit, Nm, &str)> {
    strictest_limit_on(presets, kind, field, false)
}

pub fn strictest_limit_on(
    presets: &[PresetRules],
    kind: RuleKind,
    field: RuleField,
    inner: bool,
) -> Option<(Limit, Nm, &str)> {
    let mut best: Option<(Limit, Nm, &str)> = None;
    for preset in presets {
        for (k, f, limit, bound) in preset_limits(&preset.rules) {
            if k != kind || f != field {
                continue;
            }
            let bound = layer_bound(&preset.rules, k, f, bound, inner);
            let stricter = match best {
                None => true,
                Some((_, b, _)) => match limit {
                    Limit::Min => bound > b,
                    Limit::Max => bound < b,
                },
            };
            if stricter {
                best = Some((limit, bound, preset.name.as_str()));
            }
        }
    }
    best
}

fn layer_bound(rules: &FabRules, kind: RuleKind, field: RuleField, flat: Nm, inner: bool) -> Nm {
    if inner && let Some(limits) = rules.inner {
        limits
            .limits()
            .into_iter()
            .find(|(k, f, _)| *k == kind && *f == field)
            .and_then(|(_, _, v)| v)
            .unwrap_or(flat)
    } else {
        flat
    }
}

/// A rule record value that allows less than a preset's limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutsidePreset<'a> {
    pub record: &'a DesignRule,
    pub field: RuleField,
    /// The record's value.
    pub value: Nm,
    /// The preset's limit, and which way the value breaks it.
    pub preset_limit: Nm,
    pub limit: Limit,
    pub preset: String,
}

/// The enabled rule records, board-wide first and then by class rank, each with the
/// first of its values that a preset's limit refuses: one finding per record and preset.
pub fn outside_presets<'a>(
    pcb: &'a PcbDocument,
    presets: &[PresetRules],
) -> Vec<OutsidePreset<'a>> {
    let mut records: Vec<&DesignRule> = pcb.rules().filter(|r| r.enabled).collect();
    records.sort_by_key(|r| {
        let rank = r
            .scope
            .net_class
            .and_then(|c| pcb.net_class(c))
            .map_or(0, |c| c.rank);
        (rank, r.kind)
    });
    let mut found = Vec::new();
    for record in records {
        for preset in presets {
            let breach = preset_limits(&preset.rules)
                .into_iter()
                .filter(|(kind, ..)| *kind == record.kind)
                .find_map(|(_, field, limit, bound)| {
                    let value = record.values.length(field)?;
                    pcb.copper_layers()
                        .filter(|l| {
                            record
                                .scope
                                .layers
                                .as_ref()
                                .is_none_or(|s| s.priority(l).is_some())
                        })
                        .find_map(|layer| {
                            // A more specific Layer rule can cover this owner's flat value
                            // everywhere the preset's inner minimum differs.
                            let specificity = |s: &reda_model::RuleScope| match &s.layers {
                                None => Some(0),
                                Some(layers) => layers.priority(layer),
                            };
                            let shadowed = pcb.rules().any(|r| {
                                r.enabled
                                    && r.kind == record.kind
                                    && r.scope.net_class == record.scope.net_class
                                    && r.scope.net_class_pair == record.scope.net_class_pair
                                    && specificity(&r.scope) > specificity(&record.scope)
                                    && r.values.get(field).is_some()
                            });
                            if shadowed {
                                return None;
                            }
                            let bound = layer_bound(
                                &preset.rules,
                                record.kind,
                                field,
                                bound,
                                layer.side.is_none(),
                            );
                            let outside = match limit {
                                Limit::Min => value < bound,
                                Limit::Max => value > bound,
                            };
                            outside.then_some((field, value, limit, bound))
                        })
                });
            if let Some((field, value, limit, preset_limit)) = breach {
                found.push(OutsidePreset {
                    record,
                    field,
                    value,
                    preset_limit,
                    limit,
                    preset: preset.name.clone(),
                });
            }
        }
    }
    found
}

/// One `rule_outside_preset` Violation per finding of [`outside_presets`], at `severity`:
/// the record's value measured against the preset's limit, and the arguments `rule` (the
/// kind), `value` (which of its values), `board`, `preset_limit`, `limit`, `preset` and,
/// for a class record, `net_class`. Its objects are the class, if any, then the record.
pub fn outside_preset_violations(
    pcb: &PcbDocument,
    presets: &[PresetRules],
    severity: Severity,
) -> Vec<Violation> {
    if severity == Severity::Off {
        return Vec::new();
    }
    outside_presets(pcb, presets)
        .into_iter()
        .map(|f| {
            let mut v = Violation::new(reda_model::DrcCheck::RuleOutsidePreset.id(), severity)
                .length(f.value, f.preset_limit, f.limit, f.record.uuid)
                .arg("rule", f.record.kind.id())
                .arg("value", f.field.path())
                .arg("board", f.value)
                .arg("preset_limit", f.preset_limit)
                .arg(
                    "limit",
                    match f.limit {
                        Limit::Min => "min",
                        Limit::Max => "max",
                    },
                )
                .arg("preset", f.preset.as_str());
            if let Some(class) = f.record.scope.net_class.and_then(|c| pcb.net_class(c)) {
                v = v
                    .arg("net_class", class.name.as_str())
                    .objects([class.uuid]);
            }
            v.objects([f.record.uuid])
        })
        .collect()
}
