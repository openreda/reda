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
    registry.add(RulesValidate);
    registry.add(RulesApplicable);
    registry.add(RulesAffected);
    registry.add(RulesSaveAsPreset);
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

/// Refuses `rule` if it has a blocking problem, or (for a board-wide record) if judging
/// a class record over it brings that record a problem it did not have. A class record a
/// merge left broken does not stop other edits, so the user can fix it.
fn refuse_blocking(pcb: &PcbDocument, rule: &DesignRule) -> Result<(), CommandError> {
    let board = if rule.scope.is_board_wide() {
        rule
    } else {
        pcb.board_rule(rule.kind)
    };
    let old_board = pcb.board_rule(rule.kind);
    let classes = pcb
        .rules()
        .filter(|r| r.kind == rule.kind && !r.scope.is_board_wide() && r.uuid != rule.uuid)
        .filter(|_| rule.scope.is_board_wide())
        .filter(|r| blocking_problems(old_board, r).is_empty());
    for record in std::iter::once(rule).chain(classes) {
        if let Some(problem) = blocking_problems(board, record).into_iter().next() {
            let (field, why) = (problem.field, problem.message());
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
    /// Whether it is a limit the preset sets, rather than a preferred size or an output
    /// seed.
    pub limit: bool,
    /// Whether the row starts ticked: limits and output seeds do; a preferred size only
    /// if the board's would fall outside the range the ticked limits make.
    pub ticked: bool,
}

/// A Net Class record that applying the rows would leave with a blocking problem. Class
/// records never change; they are only flagged.
#[derive(Debug, PartialEq, Serialize, JsonSchema)]
pub struct ClassProblem {
    pub rule: Uuid,
    pub net_class: Uuid,
    pub kind: RuleKind,
    /// Which value, such as `min`.
    pub value: String,
    pub message: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuleChanges {
    /// Every value that differs, in kind order; empty when they all match. Applying
    /// returns the rows it applied.
    pub changes: Vec<RuleChange>,
    /// The Net Class records the ticked (or applied) rows would break.
    pub class_problems: Vec<ClassProblem>,
}

/// The board-wide values that `preset` seeds differently from the board, each with
/// whether its row starts ticked.
fn rule_changes(pcb: &PcbDocument, preset: &FabRules) -> Vec<RuleChange> {
    let limits = crate::rules::presets::preset_limits(preset);
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
                    limit: limits.iter().any(|l| l.0 == kind && l.1 == field),
                    ticked: !field.is_preferred(),
                });
            }
        }
    }
    // A preferred row is ticked when the board's value falls outside the new range.
    let ticked: Vec<(RuleKind, RuleField)> = out
        .iter()
        .filter(|c| c.ticked)
        .map(|c| (c.kind, c.field))
        .collect();
    let applied = reseeded(pcb, &out, &ticked);
    for change in out.iter_mut().filter(|c| c.field.is_preferred()) {
        let record = applied
            .iter()
            .find(|r| r.kind == change.kind)
            .unwrap_or_else(|| pcb.board_rule(change.kind));
        change.ticked = crate::rules::blocking_problems(record, record)
            .iter()
            .any(|p| p.field == change.field);
    }
    out
}

/// The board-wide records once `rows` of `changes` are applied: only those that change.
fn reseeded(
    pcb: &PcbDocument,
    changes: &[RuleChange],
    rows: &[(RuleKind, RuleField)],
) -> Vec<DesignRule> {
    let mut changed: Vec<DesignRule> = Vec::new();
    for change in changes.iter().filter(|c| rows.contains(&(c.kind, c.field))) {
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
    changed
}

/// The class records that judging over `boards` (new board-wide records) breaks.
fn class_problems(pcb: &PcbDocument, boards: &[DesignRule]) -> Vec<ClassProblem> {
    let mut out = Vec::new();
    for board in boards {
        let old = pcb.board_rule(board.kind);
        for record in pcb
            .rules()
            .filter(|r| r.kind == board.kind && !r.scope.is_board_wide())
        {
            if !blocking_problems(old, record).is_empty() {
                continue;
            }
            if let Some(p) = blocking_problems(board, record).into_iter().next() {
                out.push(ClassProblem {
                    rule: record.uuid,
                    net_class: record.scope.net_class.expect("a class record"),
                    kind: record.kind,
                    value: p.field.path().to_owned(),
                    message: p.message(),
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
                      preset's values, one row each. Limit rows start `ticked`; a preferred \
                      row only if the board's value would fall outside the new range. \
                      `class_problems` lists the Net Class records the ticked rows would \
                      break; class records never change. `pcb.rules.apply_fab_preset` \
                      applies the chosen rows.",
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
        let pcb = pcb(cx)?;
        let changes = rule_changes(pcb, &preset);
        let ticked: Vec<_> = changes
            .iter()
            .filter(|c| c.ticked)
            .map(|c| (c.kind, c.field))
            .collect();
        let class_problems = class_problems(pcb, &reseeded(pcb, &changes, &ticked));
        Ok(RuleChanges {
            changes,
            class_problems,
        })
    }
}

/// One row of the preview to apply.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RowRef {
    pub kind: RuleKind,
    pub field: RuleField,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyPresetParams {
    /// The Project copy's UUID.
    pub preset: Uuid,
    /// The rows of `pcb.rules.fab_preset_diff` to apply; left out, the rows it ticks.
    #[serde(default)]
    pub rows: Option<Vec<RowRef>>,
}

pub struct FabPresetApply;

impl Command for FabPresetApply {
    const META: Meta = Meta {
        id: "pcb.rules.apply_fab_preset",
        title: "Apply Fab Preset Rules",
        description: "Re-seeds board-wide design rule values from one of the Project's Fab \
                      Preset copies: the chosen `rows` of `pcb.rules.fab_preset_diff`, or \
                      the rows it ticks. A row that no longer differs is skipped, and a \
                      blocking result for the board is refused. Net Class records and DRC \
                      severities are kept; class records the result breaks are returned in \
                      `class_problems`. Returns the rows applied.",
        category: "pcb",
        icon: "rules",
        kind: Kind::Edit,
    };
    type Params = ApplyPresetParams;
    type Output = RuleChanges;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ApplyPresetParams) -> Result<RuleChanges, CommandError> {
        let preset = preset_rules(cx.project()?, p.preset)?;
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let all = rule_changes(pcb, &preset);
        let rows: Vec<(RuleKind, RuleField)> = match &p.rows {
            Some(rows) => rows.iter().map(|r| (r.kind, r.field)).collect(),
            None => all
                .iter()
                .filter(|c| c.ticked)
                .map(|c| (c.kind, c.field))
                .collect(),
        };
        let changed = reseeded(pcb, &all, &rows);
        for rule in &changed {
            if let Some(problem) = blocking_problems(rule, rule).into_iter().next() {
                return Err(CommandError::invalid_params(
                    "/rows",
                    format!(
                        "{} for the board's `{}` rule; choose its other rows too",
                        problem.message(),
                        rule.kind.id()
                    ),
                ));
            }
        }
        let class_problems = class_problems(pcb, &changed);
        let changes = all
            .into_iter()
            .filter(|c| rows.contains(&(c.kind, c.field)))
            .collect();
        let tx = cx.tx()?;
        for rule in changed {
            tx.replace(Object::DesignRule(InDocument::new(document, rule)))?;
        }
        Ok(RuleChanges {
            changes,
            class_problems,
        })
    }
}

pub struct RulesValidate;

impl Command for RulesValidate {
    const META: Meta = Meta {
        id: "pcb.rules.validate",
        title: "Validate Design Rules",
        description: "Checks every design rule record. `problems` are blocking: a negative \
                      value, `min` above `max` or `preferred` outside the range, judged on a \
                      record's effective values; `pcb.rules.set` refuses them, but a file \
                      can hold them after a merge, and DRC reports them as `rule_invalid`. \
                      `notes` never block: a value outside the limit of a Fab Preset an \
                      Output Job of this PCB uses, a class weaker than the board, a class \
                      with no Nets, an override equal to what it inherits, a disabled \
                      board-wide mask or paste record, and a Net whose classes combine into \
                      an invalid range.",
        category: "pcb",
        icon: "rules",
        kind: Kind::Query,
    };
    type Params = reda_command::NoParams;
    type Output = crate::rules::validation::Validation;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        _: reda_command::NoParams,
    ) -> Result<crate::rules::validation::Validation, CommandError> {
        let pcb = pcb(cx)?;
        let presets = crate::rules::presets::job_presets(cx.project()?, pcb);
        Ok(crate::rules::validation::validate(pcb, &presets))
    }
}

/// An object to explain: a Track, Via, Free Pad or Zone (or a Net) by UUID, or a Pad of a
/// Footprint instance as `{footprint, pad}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RuleObject {
    Object(Uuid),
    Pad(reda_model::PadRef),
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplicableParams {
    /// One or two objects.
    pub objects: Vec<RuleObject>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApplicableResult {
    /// Each object's Net, in the order given; `null` for none.
    pub nets: Vec<Option<Uuid>>,
    /// Every rule kind, in file order.
    pub kinds: Vec<crate::rules::explain::KindRules>,
}

/// The Net of a copper object, or `pointer`'s error.
fn net_of(
    pcb: &PcbDocument,
    object: RuleObject,
    pointer: &str,
) -> Result<Option<Uuid>, CommandError> {
    match object {
        RuleObject::Pad(pad) => {
            let instance = pcb.footprint(pad.footprint).ok_or_else(|| {
                CommandError::not_found(format!(
                    "no Footprint has UUID {} ({pointer})",
                    pad.footprint
                ))
            })?;
            crate::placed::placed_pads(pcb, instance)
                .into_iter()
                .find(|p| p.pad == pad.pad)
                .map(|p| p.net)
                .ok_or_else(|| {
                    CommandError::not_found(format!(
                        "the Footprint has no Pad {} ({pointer})",
                        pad.pad
                    ))
                })
        }
        RuleObject::Object(uuid) => {
            if pcb.net(uuid).is_some() {
                Ok(Some(uuid))
            } else if let Some(t) = pcb.track(uuid) {
                Ok(t.net)
            } else if let Some(v) = pcb.via(uuid) {
                Ok(v.net)
            } else if let Some(p) = pcb.free_pad(uuid) {
                Ok(p.net)
            } else if let Some(z) = pcb.zone(uuid) {
                Ok(z.net)
            } else {
                Err(CommandError::invalid_params(
                    pointer,
                    format!("{uuid} is not a Track, Via, Free Pad, Zone, Pad or Net of the PCB"),
                ))
            }
        }
    }
}

pub struct RulesApplicable;

impl Command for RulesApplicable {
    const META: Meta = Meta {
        id: "pcb.rules.applicable",
        title: "Applicable Rules",
        description: "Explains the design rules for one or two copper objects (Tracks, Vias, \
                      Free Pads, Zones, Pads as `{footprint, pad}`, or Nets). Per kind: the \
                      winning values and the record each comes from, the records that lost, \
                      the disabled ones, and the DRC check's severity; for two objects, \
                      each one's `clearance` and `hole_to_hole` and which applied \
                      (`between`).",
        category: "pcb",
        icon: "rules",
        kind: Kind::Query,
    };
    type Params = ApplicableParams;
    type Output = ApplicableResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ApplicableParams,
    ) -> Result<ApplicableResult, CommandError> {
        if !(1..=2).contains(&p.objects.len()) {
            return Err(CommandError::invalid_params(
                "/objects",
                "give one or two objects",
            ));
        }
        let pcb = pcb(cx)?;
        let nets = p
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| net_of(pcb, *o, &format!("/objects/{i}")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ApplicableResult {
            kinds: crate::rules::explain::applicable(pcb, &nets),
            nets,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AffectedParams {
    /// A rule record.
    #[serde(default)]
    pub rule: Option<Uuid>,
    /// A Net Class: what its records affect.
    #[serde(default)]
    pub net_class: Option<Uuid>,
}

pub struct RulesAffected;

impl Command for RulesAffected {
    const META: Meta = Meta {
        id: "pcb.rules.affected",
        title: "Objects a Rule Affects",
        description: "Lists the copper a rule record (`rule`) or a Net Class's records \
                      (`net_class`) affect: the Nets they supply a value to, their Tracks, \
                      Vias, Free Pads and Zones (`objects`) and Pads (`pads`, as \
                      `selection.set` takes them). Objects whose every value another record \
                      overrides are left out. A board-wide record also affects copper on \
                      no Net.",
        category: "pcb",
        icon: "rules",
        kind: Kind::Query,
    };
    type Params = AffectedParams;
    type Output = crate::rules::explain::Affected;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: AffectedParams,
    ) -> Result<crate::rules::explain::Affected, CommandError> {
        let pcb = pcb(cx)?;
        let records: Vec<&DesignRule> = match (p.rule, p.net_class) {
            (Some(rule), None) => vec![pcb.rule(rule).ok_or_else(|| {
                CommandError::not_found(format!("no rule record has UUID {rule} (`rule`)"))
            })?],
            (None, Some(class)) => {
                if pcb.net_class(class).is_none() {
                    return Err(CommandError::not_found(format!(
                        "no Net Class has UUID {class} (`net_class`)"
                    )));
                }
                pcb.rules()
                    .filter(|r| r.scope.net_class == Some(class))
                    .collect()
            }
            _ => {
                return Err(CommandError::invalid_params(
                    "",
                    "give exactly one of `rule` and `net_class`",
                ));
            }
        };
        Ok(crate::rules::explain::affected(pcb, &records))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveAsPresetParams {
    /// The new Fab Preset's name.
    pub name: String,
    /// The Project copy whose other settings (Gerber, drill, BOM, CPL, Stackup) it takes;
    /// left out, the preset of the first Output Job for this PCB, else the Project's first
    /// copy.
    #[serde(default)]
    pub from: Option<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SavedPreset {
    /// The new Project copy's UUID.
    pub uuid: Uuid,
}

pub struct RulesSaveAsPreset;

impl Command for RulesSaveAsPreset {
    const META: Meta = Meta {
        id: "pcb.rules.save_as_preset",
        title: "Save Rules as Fab Preset",
        description: "Adds a Fab Preset copy to the Project whose rules are the board-wide \
                      design rule values: limits, preferred sizes and mask and paste seeds. \
                      Net Class records are left out, because presets have no scope, and so \
                      are Track and Via maximums. Its other settings come from `from`. \
                      `fab_preset.save_user` then keeps it for new Projects.",
        category: "pcb",
        icon: "save",
        kind: Kind::Edit,
    };
    type Params = SaveAsPresetParams;
    type Output = SavedPreset;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SaveAsPresetParams,
    ) -> Result<SavedPreset, CommandError> {
        let name = p.name.trim();
        if name.is_empty() {
            return Err(CommandError::invalid_params("/name", "the name is empty"));
        }
        let project = cx.project()?;
        let pcb = pcb(cx)?;
        let from = match p.from {
            Some(uuid) => uuid,
            None => project
                .output_jobs()
                .find(|j| j.pcb == pcb.uuid)
                .map(|j| j.fab_preset)
                .or_else(|| project.fab_presets().next().map(|p| p.uuid))
                .ok_or_else(|| {
                    CommandError::invalid_params(
                        "/from",
                        "the Project has no Fab Preset to start from",
                    )
                })?,
        };
        let mut preset = project
            .fab_preset(from)
            .ok_or_else(|| {
                CommandError::not_found(format!("the Project has no Fab Preset {from}"))
            })?
            .preset();
        preset.name = name.to_owned();
        preset.rules = crate::board::board_fab_rules(pcb);
        let uuid = cx.new_id();
        cx.tx()?
            .insert(Object::FabPreset(reda_model::ProjectFabPreset::new(
                uuid, preset,
            )))?;
        Ok(SavedPreset { uuid })
    }
}
