//! Setting design rule records and applying a Fab Preset's rules. The DRC severities are
//! set with `drc.set_severity` ([`super::drc`]).

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{
    DesignRule, FabRules, InDocument, Object, PcbDocument, Project, RuleField, RuleKind, RuleScope,
    RuleValue, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, nullable, pcb};
use crate::board::seed_rules;
use crate::rules::blocking_problems;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(RulesSet);
    registry.add(FabPresetDiff);
    registry.add(FabPresetApply);
}

/// A range's values to change; `null` clears one.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundsParam {
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub min: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub preferred: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub max: Option<Option<Length>>,
}

/// The values to change; those left out keep theirs, and `null` clears one. Which a kind
/// holds: `clearance` `value` and `ignore_pad_to_pad`; `track_width` `min`, `preferred`
/// and `max`; `via_size` `diameter` and `hole`, each `{min, preferred, max}`;
/// `hole_size` `min` and `max`; `annular_ring` and `hole_to_hole` `min`;
/// `edge_clearance` `value`; `mask_expansion` top/bottom lengths and hole-edge/Via-tenting
/// flags; `paste_expansion` either `expansion` or signed area `percent`.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValuesParam {
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub value: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<bool>")]
    pub ignore_pad_to_pad: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub min: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub preferred: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub max: Option<Option<Length>>,
    #[serde(default)]
    pub diameter: Option<BoundsParam>,
    #[serde(default)]
    pub hole: Option<BoundsParam>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub top: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub bottom: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<bool>")]
    pub from_hole_edge: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<bool>")]
    pub tent_vias_top: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<bool>")]
    pub tent_vias_bottom: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub expansion: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<i32>")]
    pub percent: Option<Option<i32>>,
}

impl ValuesParam {
    /// Every value given, by field, `None` for a `null`.
    fn changes(&self) -> Vec<(RuleField, Option<RuleValue>)> {
        let mut out = Vec::new();
        let mut put = |field, v: Option<Option<Length>>| {
            if let Some(v) = v {
                out.push((field, v.map(|l| RuleValue::Length(l.nm()))));
            }
        };
        put(RuleField::Value, self.value);
        put(RuleField::Min, self.min);
        put(RuleField::Preferred, self.preferred);
        put(RuleField::Max, self.max);
        let d = self.diameter.clone().unwrap_or_default();
        put(RuleField::DiameterMin, d.min);
        put(RuleField::DiameterPreferred, d.preferred);
        put(RuleField::DiameterMax, d.max);
        let h = self.hole.clone().unwrap_or_default();
        put(RuleField::HoleMin, h.min);
        put(RuleField::HolePreferred, h.preferred);
        put(RuleField::HoleMax, h.max);
        put(RuleField::Top, self.top);
        put(RuleField::Bottom, self.bottom);
        put(RuleField::Expansion, self.expansion);
        if let Some(percent) = self.percent {
            out.push((RuleField::Percent, percent.map(RuleValue::Percent)));
        }
        if let Some(flag) = self.from_hole_edge {
            out.push((RuleField::FromHoleEdge, flag.map(RuleValue::Flag)));
        }
        if let Some(flag) = self.tent_vias_top {
            out.push((RuleField::TentViasTop, flag.map(RuleValue::Flag)));
        }
        if let Some(flag) = self.tent_vias_bottom {
            out.push((RuleField::TentViasBottom, flag.map(RuleValue::Flag)));
        }
        if let Some(flag) = self.ignore_pad_to_pad {
            out.push((RuleField::IgnorePadToPad, flag.map(RuleValue::Flag)));
        }
        out
    }
}

pub struct RulesSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulesSetParams {
    pub kind: RuleKind,
    /// Left out, the board-wide record; `{net_class}` names a Net Class's.
    #[serde(default)]
    pub scope: RuleScope,
    #[serde(default)]
    pub values: ValuesParam,
    /// Whether the record counts.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// A note for the user; `null` clears it.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<String>")]
    pub comment: Option<Option<String>>,
}

/// One rule record as a Command result: the record, or `null` once deleted.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RuleResult {
    pub rule: Option<DesignRule>,
}

/// What [`set_record`] changes.
pub(super) struct RecordEdit<'a> {
    pub kind: RuleKind,
    pub scope: RuleScope,
    pub values: &'a ValuesParam,
    pub enabled: Option<bool>,
    pub comment: Option<Option<String>>,
}

/// `rule` (the record of the edit's kind and scope, or a new empty one) once the edit is
/// made; `None` for a class record left with no values. Refuses values the kind does not
/// hold, negative values, clearing a value the board-wide record requires, and a blocking
/// result.
pub(super) fn set_record(
    pcb: &PcbDocument,
    mut rule: DesignRule,
    edit: RecordEdit<'_>,
) -> Result<Option<DesignRule>, CommandError> {
    let RecordEdit {
        kind,
        scope,
        values,
        enabled,
        comment,
    } = edit;
    for (field, value) in values.changes() {
        let pointer = format!("/values/{}", field.path().replace('.', "/"));
        if !kind.fields().contains(&field) {
            return Err(CommandError::invalid_params(
                &pointer,
                format!("a `{}` rule has no value `{}`", kind.id(), field.path()),
            ));
        }
        if !matches!(kind, RuleKind::MaskExpansion | RuleKind::PasteExpansion)
            && value
                .and_then(RuleValue::length)
                .is_some_and(|n| n < Nm::ZERO)
        {
            return Err(CommandError::invalid_params(
                &pointer,
                format!("`{}` cannot be negative", field.path()),
            ));
        }
        if value.is_none() && scope.is_board_wide() && kind.required().contains(&field) {
            return Err(CommandError::invalid_params(
                &pointer,
                format!(
                    "the board-wide `{}` rule needs a value `{}`",
                    kind.id(),
                    field.path()
                ),
            ));
        }
        rule.values
            .set(field, value)
            .map_err(|why| CommandError::invalid_params(&pointer, why))?;
    }
    if let Some(enabled) = enabled {
        rule.enabled = enabled;
    }
    if let Some(comment) = comment {
        rule.comment = comment.filter(|c| !c.trim().is_empty());
    }
    if !scope.is_board_wide() && rule.values.is_empty() {
        return Ok(None);
    }
    if let Some(problem) = rule.problem() {
        return Err(CommandError::invalid_params("/values", problem));
    }
    refuse_blocking(pcb, &rule)?;
    Ok(Some(rule))
}

/// Refuses `rule` if it, or (for a board-wide record) a class record judged over it, has a
/// blocking problem.
fn refuse_blocking(pcb: &PcbDocument, rule: &DesignRule) -> Result<(), CommandError> {
    let board = if rule.scope.is_board_wide() {
        rule
    } else {
        pcb.board_rule(rule.kind)
    };
    let classes = pcb
        .rules()
        .filter(|r| r.kind == rule.kind && !r.scope.is_board_wide() && r.uuid != rule.uuid)
        .filter(|_| rule.scope.is_board_wide());
    for record in std::iter::once(rule).chain(classes) {
        if let Some((field, why)) = blocking_problems(board, record).into_iter().next() {
            let whose = match record.scope.net_class.and_then(|c| pcb.net_class(c)) {
                Some(class) => format!("Net Class {:?}", class.name),
                None => "the board".to_owned(),
            };
            return Err(CommandError::invalid_params(
                &format!("/values/{}", field.path().replace('.', "/")),
                format!("{why} for {whose}"),
            ));
        }
    }
    Ok(())
}

impl Command for RulesSet {
    const META: Meta = Meta {
        id: "pcb.rules.set",
        title: "Set Design Rule",
        description: "Sets values of one design rule record: the board-wide record of \
                      `kind`, or with `scope: {net_class}` that Net Class's, created when \
                      missing. Values left out keep theirs, and `null` clears one; a Net \
                      Class record with nothing left is deleted. Nothing is filled in, and a \
                      negative limit, `min` above `max` or `preferred` outside the range is \
                      refused. Returns the record, or `null` once deleted.",
        category: "pcb",
        icon: "rules",
        kind: Kind::Edit,
    };
    type Params = RulesSetParams;
    type Output = RuleResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: RulesSetParams) -> Result<RuleResult, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        if let Some(class) = p.scope.net_class
            && pcb.net_class(class).is_none()
        {
            return Err(CommandError::not_found(format!(
                "no Net Class has UUID {class} (`scope.net_class`)"
            )));
        }
        let existing = pcb.scoped_rule(p.kind, p.scope).cloned();
        let found = existing.is_some();
        let base = match existing {
            Some(rule) => rule,
            None => DesignRule {
                uuid: cx.new_id(),
                kind: p.kind,
                scope: p.scope,
                values: Default::default(),
                enabled: true,
                comment: None,
            },
        };
        let uuid = base.uuid;
        let edit = RecordEdit {
            kind: p.kind,
            scope: p.scope,
            values: &p.values,
            enabled: p.enabled,
            comment: p.comment,
        };
        let rule = set_record(super::pcb(cx)?, base, edit)?;
        let tx = cx.tx()?;
        match (&rule, found) {
            (Some(rule), true) => {
                tx.replace(Object::DesignRule(InDocument::new(document, rule.clone())))?;
            }
            (Some(rule), false) => {
                tx.insert(Object::DesignRule(InDocument::new(document, rule.clone())))?;
            }
            (None, true) => {
                tx.remove(uuid)?;
            }
            (None, false) => {}
        }
        Ok(RuleResult { rule })
    }
}

/// Names one of the Project's Fab Preset copies.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetRef {
    /// The Project copy's UUID.
    pub preset: Uuid,
}

/// One board-wide value the preset would change.
#[derive(Debug, PartialEq, Serialize, JsonSchema)]
pub struct RuleChange {
    pub kind: RuleKind,
    pub field: RuleField,
    /// The board's value now.
    pub board: Option<RuleValue>,
    /// The preset's value.
    pub preset: RuleValue,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuleChanges {
    /// Every value that differs, in kind order; empty when they all match.
    pub changes: Vec<RuleChange>,
}

/// The board-wide values that `preset` seeds differently from the board.
fn rule_changes(pcb: &PcbDocument, preset: &FabRules) -> Vec<RuleChange> {
    let mut out = Vec::new();
    for (kind, seeds) in seed_rules(preset) {
        let board = &pcb.board_rule(kind).values;
        for field in seeds.set_fields() {
            let Some(to) = seeds.get(field) else {
                continue;
            };
            let now = board.get(field);
            if now != Some(to) {
                out.push(RuleChange {
                    kind,
                    field,
                    board: now,
                    preset: to,
                });
            }
        }
    }
    out
}

fn preset_rules(project: &Project, uuid: Uuid) -> Result<FabRules, CommandError> {
    project
        .fab_preset(uuid)
        .map(|p| p.rules)
        .ok_or_else(|| CommandError::not_found(format!("the Project has no Fab Preset {uuid}")))
}

pub struct FabPresetDiff;

impl Command for FabPresetDiff {
    const META: Meta = Meta {
        id: "pcb.rules.fab_preset_diff",
        title: "Preview Fab Preset Rules",
        description: "Lists the board-wide design rule values that applying one of the \
                      Project's Fab Preset copies would change, with the board's and the \
                      preset's values. `pcb.rules.apply_fab_preset` applies them.",
        category: "pcb",
        icon: "rules",
        kind: Kind::Query,
    };
    type Params = PresetRef;
    type Output = RuleChanges;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: PresetRef) -> Result<RuleChanges, CommandError> {
        let preset = preset_rules(cx.project()?, p.preset)?;
        Ok(RuleChanges {
            changes: rule_changes(pcb(cx)?, &preset),
        })
    }
}

pub struct FabPresetApply;

impl Command for FabPresetApply {
    const META: Meta = Meta {
        id: "pcb.rules.apply_fab_preset",
        title: "Apply Fab Preset Rules",
        description: "Re-seeds the board-wide design rule records from one of the Project's \
                      Fab Preset copies, after `pcb.rules.fab_preset_diff` has shown what \
                      changes. Net Class records and DRC severities are kept. Returns the \
                      changes.",
        category: "pcb",
        icon: "rules",
        kind: Kind::Edit,
    };
    type Params = PresetRef;
    type Output = RuleChanges;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: PresetRef) -> Result<RuleChanges, CommandError> {
        let preset = preset_rules(cx.project()?, p.preset)?;
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let changes = rule_changes(pcb, &preset);
        let mut changed: Vec<DesignRule> = Vec::new();
        for change in &changes {
            let rule = match changed.iter().position(|r| r.kind == change.kind) {
                Some(i) => &mut changed[i],
                None => {
                    changed.push(pcb.board_rule(change.kind).clone());
                    changed.last_mut().expect("just pushed")
                }
            };
            if change.kind == RuleKind::PasteExpansion {
                rule.values.expansion = None;
                rule.values.percent = None;
            }
            rule.values
                .set(change.field, Some(change.preset))
                .expect("a seeded field holds the correct type");
        }
        let tx = cx.tx()?;
        for rule in changed {
            tx.replace(Object::DesignRule(InDocument::new(document, rule)))?;
        }
        Ok(RuleChanges { changes })
    }
}
