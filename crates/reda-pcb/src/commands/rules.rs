//! Setting design rule records and applying a Fab Preset's rules. The DRC severities are
//! set with `drc.set_severity` ([`super::drc`]).

use reda_command::{
    Angle, AreaValue, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta,
    Registry, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{
    ConnectionTarget, ConnectionValue, DesignRule, FabRules, InDocument, Object, PcbDocument,
    Project, RuleField, RuleKind, RuleScope, RuleValue, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, nullable, pcb};
use crate::board::seed_rules;

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

/// Depth-to-hole maximum in thousandths (8000 is 8:1); null clears the maximum.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AspectRatioParam {
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u32>")]
    pub max: Option<Option<u32>>,
}

/// Either the Via size range or plated hole clearance values, according to rule kind.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HoleParam {
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub min: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub preferred: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub max: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub via: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub pad: Option<Option<Length>>,
}

/// A Zone connection's values to change; `null` clears one.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectionParam {
    /// `relief`, `solid` or `none`.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<reda_model::ConnectionStyle>")]
    pub style: Option<Option<reda_model::ConnectionStyle>>,
    /// 2 or 4.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u8>")]
    pub spokes: Option<Option<u8>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub spoke_width: Option<Option<Length>>,
    /// The clear ring around a relieved or unconnected object.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub gap: Option<Option<Length>>,
    /// 0, 45 or 90 degrees in the object's own frame.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Angle>")]
    pub angle: Option<Option<Angle>>,
}

/// The shape properties edited independently on one Teardrop target. Null clears a scoped override.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TeardropParam {
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<bool>")]
    pub enabled: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u32>")]
    pub length: Option<Option<u32>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub max_length: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u32>")]
    pub width: Option<Option<u32>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub max_width: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<bool>")]
    pub curved: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u32>")]
    pub skip_ratio: Option<Option<u32>>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ViaParam {
    Connection(ConnectionParam),
    Teardrop(TeardropParam),
}

/// The values to change; those left out keep theirs, and `null` clears one. Which a kind
/// holds: `clearance` `value`, `hole {via, pad}`, `ignore_pad_to_pad` and (board-wide only)
/// `zero_keepout_clearance`; `track_width` `min`, `preferred`
/// and `max`; `via_size` `diameter` and `hole`, each `{min, preferred, max}`;
/// `hole_size` `min` and `max`; `annular_ring` and `hole_to_hole` `min`;
/// `edge_clearance`, `silk_to_edge` and `courtyard_clearance` `value`; `silk_to_mask`
/// `value` and `mode` (`exposed_copper` or `mask_opening`); `dangling` `tolerance`;
/// `micro_segment` `max_length`; `dead_copper` `min_area`; `zone_connection`
/// `through_hole`, `smd` and `via`, each `{style, spokes, spoke_width, gap, angle}`; `mask_expansion` top/bottom lengths and hole-edge/Via-tenting
/// flags; `paste_expansion` either `expansion` or signed area `percent`.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValuesParam {
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Vec<Uuid>>")]
    pub allowed: Option<Option<Vec<Uuid>>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub value: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<bool>")]
    pub ignore_pad_to_pad: Option<Option<bool>>,
    /// `clearance`, board-wide only: Keepout edges are exact, with no clearance.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<bool>")]
    pub zero_keepout_clearance: Option<Option<bool>>,
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
    pub hole: Option<HoleParam>,
    #[serde(default)]
    pub aspect_ratio: Option<AspectRatioParam>,
    /// Maximum number of micro Vias meeting end to end in one Via Stack.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u8>")]
    pub max_stack: Option<Option<u8>>,
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
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<reda_model::SilkMaskMode>")]
    pub mode: Option<Option<reda_model::SilkMaskMode>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub tolerance: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub max_length: Option<Option<Length>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<AreaValue>")]
    pub min_area: Option<Option<AreaValue>>,
    #[serde(default)]
    pub through_hole: Option<ConnectionParam>,
    #[serde(default)]
    pub smd: Option<ConnectionParam>,
    #[serde(default)]
    pub via: Option<ViaParam>,
    #[serde(default)]
    pub through_hole_pad: Option<TeardropParam>,
    #[serde(default)]
    pub smd_pad: Option<TeardropParam>,
    #[serde(default)]
    pub width_change: Option<TeardropParam>,
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
        put(RuleField::HoleVia, h.via);
        put(RuleField::HolePad, h.pad);
        put(RuleField::HolePreferred, h.preferred);
        put(RuleField::HoleMax, h.max);
        put(RuleField::Top, self.top);
        put(RuleField::Bottom, self.bottom);
        put(RuleField::Expansion, self.expansion);
        put(RuleField::Tolerance, self.tolerance);
        put(RuleField::MaxLength, self.max_length);
        if let Some(max) = self.aspect_ratio.as_ref().and_then(|a| a.max) {
            out.push((RuleField::AspectRatioMax, max.map(RuleValue::MilliRatio)));
        }
        if let Some(max) = self.max_stack {
            out.push((RuleField::MaxStack, max.map(RuleValue::Count)));
        }
        if let Some(mode) = self.mode {
            out.push((RuleField::Mode, mode.map(RuleValue::Mode)));
        }
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
        if let Some(flag) = self.zero_keepout_clearance {
            out.push((RuleField::ZeroKeepoutClearance, flag.map(RuleValue::Flag)));
        }
        if let Some(area) = self.min_area {
            out.push((RuleField::MinArea, area.map(|a| RuleValue::Area(a.0))));
        }
        let (via_zone, via_tear) = match &self.via {
            Some(ViaParam::Connection(z)) => (Some(z), None),
            Some(ViaParam::Teardrop(t)) => (None, Some(t)),
            None => (None, None),
        };
        for (target, param) in [
            (ConnectionTarget::ThroughHole, self.through_hole.as_ref()),
            (ConnectionTarget::Smd, self.smd.as_ref()),
            (ConnectionTarget::Via, via_zone),
        ] {
            let Some(c) = param else { continue };
            let field = |v| RuleField::connection_field(target, v);
            if let Some(v) = c.style {
                out.push((field(ConnectionValue::Style), v.map(RuleValue::Style)));
            }
            if let Some(v) = c.spokes {
                out.push((field(ConnectionValue::Spokes), v.map(RuleValue::Count)));
            }
            if let Some(v) = c.spoke_width {
                out.push((
                    field(ConnectionValue::SpokeWidth),
                    v.map(|l| RuleValue::Length(l.nm())),
                ));
            }
            if let Some(v) = c.gap {
                out.push((
                    field(ConnectionValue::Gap),
                    v.map(|l| RuleValue::Length(l.nm())),
                ));
            }
            if let Some(v) = c.angle {
                out.push((
                    field(ConnectionValue::Angle),
                    v.map(|a| RuleValue::Angle(a.millideg())),
                ));
            }
        }
        use reda_model::{TeardropField, TeardropKind};
        for (kind, param) in [
            (TeardropKind::ThroughHolePad, self.through_hole_pad.as_ref()),
            (TeardropKind::SmdPad, self.smd_pad.as_ref()),
            (TeardropKind::Via, via_tear),
            (TeardropKind::WidthChange, self.width_change.as_ref()),
        ] {
            let Some(t) = param else {
                continue;
            };
            let field = |f| RuleField::Teardrop(kind, f);
            if let Some(v) = t.enabled {
                out.push((field(TeardropField::Enabled), v.map(RuleValue::Flag)));
            }
            if let Some(v) = t.length {
                out.push((
                    field(TeardropField::Length),
                    v.map(RuleValue::UnsignedPercent),
                ));
            }
            if let Some(v) = t.max_length {
                out.push((
                    field(TeardropField::MaxLength),
                    v.map(|l| RuleValue::Length(l.nm())),
                ));
            }
            if let Some(v) = t.width {
                out.push((
                    field(TeardropField::Width),
                    v.map(RuleValue::UnsignedPercent),
                ));
            }
            if let Some(v) = t.max_width {
                out.push((
                    field(TeardropField::MaxWidth),
                    v.map(|l| RuleValue::Length(l.nm())),
                ));
            }
            if let Some(v) = t.curved {
                out.push((field(TeardropField::Curved), v.map(RuleValue::Flag)));
            }
            if let Some(v) = t.skip_ratio {
                out.push((
                    field(TeardropField::SkipRatio),
                    v.map(RuleValue::UnsignedPercent),
                ));
            }
        }
        out
    }
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeParam {
    #[serde(default)]
    net_class: Option<Uuid>,
    #[serde(default)]
    layers: Option<reda_model::RuleLayers>,
    /// Two class names. Default matches every Net.
    #[serde(default)]
    net_class_pair: Option<[String; 2]>,
}

impl ScopeParam {
    fn resolve(self, pcb: &PcbDocument) -> Result<RuleScope, CommandError> {
        let class = |name: &str| {
            if name.eq_ignore_ascii_case(crate::DEFAULT_NET_CLASS) {
                return Ok(Uuid::NIL);
            }
            pcb.net_class_named(name).map(|c| c.uuid).ok_or_else(|| {
                CommandError::invalid_params(
                    "/scope/net_class_pair",
                    format!("unknown Net Class {name:?}"),
                )
            })
        };
        let pair = match self.net_class_pair {
            Some([a, b]) => {
                let mut pair = [class(&a)?, class(&b)?];
                pair.sort();
                Some(pair)
            }
            None => None,
        };
        let mut layers = self.layers;
        if let Some(reda_model::RuleLayers::Named(ids)) = &mut layers {
            ids.sort();
        }
        Ok(RuleScope {
            net_class: self.net_class,
            net_class_pair: pair,
            layers,
        })
    }
}

pub struct RulesSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulesSetParams {
    pub kind: RuleKind,
    /// Filter the record by Net Class UUID or class-name pair, and by copper Layers.
    #[serde(default)]
    pub scope: ScopeParam,
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
    if let Some(allowed) = &values.allowed {
        if kind != RuleKind::ViaSpan || scope.net_class.is_none() {
            return Err(CommandError::invalid_params(
                "/values/allowed",
                "only a Net Class Via Span record takes an allowed list",
            ));
        }
        if let Some(list) = allowed {
            let mut unique = std::collections::BTreeSet::new();
            for (i, id) in list.iter().enumerate() {
                if !pcb.stackup().via_types.iter().any(|v| v.uuid == *id) || !unique.insert(id) {
                    return Err(CommandError::invalid_params(
                        &format!("/values/allowed/{i}"),
                        "expected a distinct Via type on this board",
                    ));
                }
            }
        }
        rule.values.allowed.clone_from(allowed);
    }
    for (field, value) in values.changes() {
        let pointer = format!("/values/{}", field.path().replace('.', "/"));
        if matches!(kind, RuleKind::ViaSize | RuleKind::MicroviaSize)
            && scope.is_board_wide()
            && matches!(
                field,
                RuleField::DiameterPreferred | RuleField::HolePreferred
            )
        {
            return Err(CommandError::invalid_params(
                &pointer,
                "preferred Via sizes belong to a Via type or Net Class",
            ));
        }
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
        if field == RuleField::ZeroKeepoutClearance && !scope.is_board_wide() {
            return Err(CommandError::invalid_params(
                &pointer,
                "`zero_keepout_clearance` is set on the board-wide record only",
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
        refuse_blocking(pcb, &rule, true)?;
        return Ok(None);
    }
    if let Some(problem) = rule.problem() {
        return Err(CommandError::invalid_params("/values", problem));
    }
    refuse_blocking(pcb, &rule, false)?;
    Ok(Some(rule))
}

/// Refuses `rule` if it has a blocking problem, or (for a board-wide record) if judging
/// a class record over it brings that record a problem it did not have. A class record a
/// merge left broken does not stop other edits, so the user can fix it.
fn refuse_blocking(pcb: &PcbDocument, rule: &DesignRule, remove: bool) -> Result<(), CommandError> {
    let mut records: Vec<_> = pcb.rules().filter(|r| r.kind == rule.kind).collect();
    let before = crate::rules::validation::problems_for(pcb, &records);
    records.retain(|r| r.uuid != rule.uuid);
    if !remove {
        records.push(rule);
    }
    for problem in crate::rules::validation::problems_for(pcb, &records) {
        if problem.rule != rule.uuid && before.contains(&problem) {
            continue;
        }
        let whose = problem
            .net_class
            .and_then(|id| pcb.net_class(id))
            .map_or_else(
                || "the board".to_owned(),
                |class| format!("Net Class {:?}", class.name),
            );
        return Err(CommandError::invalid_params(
            &format!("/values/{}", problem.value.replace('.', "/")),
            format!("{} for {whose}", problem.message),
        ));
    }
    Ok(())
}

impl Command for RulesSet {
    const META: Meta = Meta {
        id: "pcb.rules.set",
        title: "Set Design Rule",
        description: "Edits a rule record identified by kind and scope. The scope may name a \
                      Net Class by UUID, or an unordered clearance pair by class names, and \
                      copper Layers by group or UUID list. Omitted fields retain their values. \
                      A null clears an override and an empty scoped record is deleted. \
                      Invalid effective ranges are refused. Returns the record or null.",
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
        let scope = p.scope.resolve(pcb)?;
        if let Some(class) = scope.net_class
            && pcb.net_class(class).is_none()
        {
            return Err(CommandError::not_found(format!(
                "no Net Class has UUID {class} (`scope.net_class`)"
            )));
        }
        let existing = pcb.scoped_rule(p.kind, &scope).cloned();
        if let Some(why) = pcb.rule_layer_problem(p.kind, &scope, existing.as_ref().map(|r| r.uuid))
        {
            return Err(CommandError::invalid_params("/scope/layers", why));
        }
        let found = existing.is_some();
        let base = match existing {
            Some(rule) => rule,
            None => DesignRule {
                uuid: cx.new_id(),
                kind: p.kind,
                scope: scope.clone(),
                values: Default::default(),
                enabled: true,
                comment: None,
            },
        };
        let uuid = base.uuid;
        let edit = RecordEdit {
            kind: p.kind,
            scope,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<reda_model::LayerGroup>,
    pub kind: RuleKind,
    pub field: RuleField,
    /// The board's value now.
    pub board: Option<RuleValue>,
    /// The preset's value.
    pub preset: Option<RuleValue>,
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
    for (kind, seeds) in seed_rules(preset)
        .into_iter()
        .chain(crate::silk_rules(preset))
    {
        let board = &pcb.board_rule(kind).values;
        for field in seeds.set_fields() {
            if field == RuleField::AspectRatioMax {
                continue;
            }
            let Some(to) = seeds.get(field) else {
                continue;
            };
            let now = board.get(field);
            if now != Some(to) {
                out.push(RuleChange {
                    kind,
                    field,
                    board: now,
                    preset: Some(to),
                    layers: None,
                    limit: limits.iter().any(|l| l.0 == kind && l.1 == field),
                    ticked: !field.is_preferred(),
                });
            }
        }
    }
    // Optional drilling limits also appear when applying a preset would clear them.
    let laser = crate::board::microvia_seeds(preset);
    let mechanical = reda_model::RuleValues {
        aspect_ratio: preset.via_aspect_ratio,
        ..Default::default()
    };
    for (kind, seeds, fields) in [
        (
            RuleKind::ViaSize,
            mechanical,
            &[RuleField::AspectRatioMax][..],
        ),
        (
            RuleKind::MicroviaSize,
            laser,
            &[
                RuleField::DiameterMin,
                RuleField::DiameterMax,
                RuleField::HoleMin,
                RuleField::HoleMax,
                RuleField::AspectRatioMax,
                RuleField::MaxStack,
            ][..],
        ),
    ] {
        for &field in fields {
            let board = pcb.board_rule(kind).values.get(field);
            let to = seeds.get(field);
            if board != to {
                out.push(RuleChange {
                    kind,
                    field,
                    board,
                    preset: to,
                    layers: None,
                    limit: true,
                    ticked: true,
                });
            }
        }
    }
    for (kind, field, to) in preset.inner.unwrap_or_default().limits() {
        let board = pcb
            .scoped_rule(kind, &RuleScope::INNER)
            .and_then(|r| r.values.get(field));
        let to = to.map(RuleValue::Length);
        if board != to {
            out.push(RuleChange {
                kind,
                field,
                layers: Some(reda_model::LayerGroup::Inner),
                board,
                preset: to,
                limit: true,
                ticked: true,
            });
        }
    }
    // A preferred row is ticked when the board's value falls outside the new range.
    let ticked: Vec<(RuleKind, RuleField, Option<reda_model::LayerGroup>)> = out
        .iter()
        .filter(|c| c.ticked)
        .map(|c| (c.kind, c.field, c.layers))
        .collect();
    let applied = reseeded(pcb, &out, &ticked);
    let blockers = preset_problems(pcb, &applied);
    for change in out.iter_mut().filter(|c| c.field.is_preferred()) {
        change.ticked = blockers.iter().any(|p| {
            p.kind == change.kind
                && p.net_class.is_none()
                && p.value == change.field.path()
                && p.problem == crate::rules::ProblemKind::PreferredOutside
                && applied
                    .iter()
                    .find(|r| r.uuid == p.rule)
                    .or_else(|| pcb.rule(p.rule))
                    .is_some_and(|r| {
                        r.scope.is_board_wide() || r.values.get(change.field).is_none()
                    })
        });
    }
    out
}

/// The board-wide records once `rows` of `changes` are applied: only those that change.
fn reseeded(
    pcb: &PcbDocument,
    changes: &[RuleChange],
    rows: &[(RuleKind, RuleField, Option<reda_model::LayerGroup>)],
) -> Vec<DesignRule> {
    let mut changed: Vec<DesignRule> = Vec::new();
    for change in changes
        .iter()
        .filter(|c| rows.contains(&(c.kind, c.field, c.layers)))
    {
        let scope = RuleScope {
            layers: change.layers.map(reda_model::RuleLayers::Group),
            ..RuleScope::BOARD
        };
        let rule = match changed
            .iter()
            .position(|r| r.kind == change.kind && r.scope == scope)
        {
            Some(i) => &mut changed[i],
            None => {
                changed.push(
                    pcb.scoped_rule(change.kind, &scope)
                        .cloned()
                        .unwrap_or_else(|| DesignRule {
                            uuid: pcb.uuid.child(&format!("inner.{}", change.kind.id())),
                            kind: change.kind,
                            scope,
                            values: Default::default(),
                            enabled: true,
                            comment: None,
                        }),
                );
                changed.last_mut().expect("just pushed")
            }
        };
        if change.kind == RuleKind::PasteExpansion {
            rule.values.expansion = None;
            rule.values.percent = None;
        }
        rule.values
            .set(change.field, change.preset)
            .expect("a seeded field holds the correct type");
    }
    changed
}

/// The class records that judging over `boards` (new board-wide records) breaks.
fn class_problems(pcb: &PcbDocument, boards: &[DesignRule]) -> Vec<ClassProblem> {
    let mut seen = std::collections::BTreeSet::new();
    preset_problems(pcb, boards)
        .into_iter()
        .filter(|p| seen.insert(p.rule))
        .filter_map(|p| {
            Some(ClassProblem {
                rule: p.rule,
                net_class: p.net_class?,
                kind: p.kind,
                value: p.value,
                message: p.message,
            })
        })
        .collect()
}

fn preset_problems(
    pcb: &PcbDocument,
    boards: &[DesignRule],
) -> Vec<crate::rules::validation::Problem> {
    let old: Vec<_> = pcb.rules().collect();
    let before = crate::rules::validation::problems_for(pcb, &old);
    let records: Vec<_> = old
        .into_iter()
        .filter(|r| !boards.iter().any(|b| b.uuid == r.uuid))
        .chain(
            boards
                .iter()
                .filter(|r| r.scope.is_board_wide() || !r.values.is_empty()),
        )
        .collect();
    crate::rules::validation::problems_for(pcb, &records)
        .into_iter()
        .filter(|p| !before.contains(p))
        .collect()
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
            .map(|c| (c.kind, c.field, c.layers))
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
    #[serde(default)]
    pub layers: Option<reda_model::LayerGroup>,
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
        let rows: Vec<(RuleKind, RuleField, Option<reda_model::LayerGroup>)> = match &p.rows {
            Some(rows) => rows.iter().map(|r| (r.kind, r.field, r.layers)).collect(),
            None => all
                .iter()
                .filter(|c| c.ticked)
                .map(|c| (c.kind, c.field, c.layers))
                .collect(),
        };
        let changed = reseeded(pcb, &all, &rows);
        if let Some(problem) = preset_problems(pcb, &changed)
            .into_iter()
            .find(|p| p.net_class.is_none())
        {
            return Err(CommandError::invalid_params(
                "/rows",
                format!(
                    "{} for the board's {} rule",
                    problem.message,
                    problem.kind.id()
                ),
            ));
        }
        let existing: std::collections::BTreeSet<_> = changed
            .iter()
            .filter(|r| pcb.rule(r.uuid).is_some())
            .map(|r| r.uuid)
            .collect();
        let class_problems = class_problems(pcb, &changed);
        let changes = all
            .into_iter()
            .filter(|c| rows.contains(&(c.kind, c.field, c.layers)))
            .collect();
        let tx = cx.tx()?;
        for rule in changed {
            let id = rule.uuid;
            let empty = !rule.scope.is_board_wide() && rule.values.is_empty();
            let object = Object::DesignRule(InDocument::new(document, rule));
            if empty {
                tx.remove(id)?;
            } else if existing.contains(&id) {
                tx.replace(object)?;
            } else {
                tx.insert(object)?;
            }
        }
        Ok(RuleChanges {
            changes,
            class_problems,
        })
    }
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateParams {
    #[serde(default)]
    kind: Option<RuleKind>,
    #[serde(default)]
    scope: Option<ScopeParam>,
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
    type Params = ValidateParams;
    type Output = crate::rules::validation::Validation;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ValidateParams,
    ) -> Result<crate::rules::validation::Validation, CommandError> {
        let pcb = pcb(cx)?;
        let presets = crate::rules::presets::job_presets(cx.project()?, pcb);
        let mut result = crate::rules::validation::validate(pcb, &presets);
        if p.kind.is_some() || p.scope.is_some() {
            let scope = p.scope.map(|s| s.resolve(pcb)).transpose()?;
            let matches = |id| {
                pcb.rule(id).is_some_and(|r| {
                    p.kind.is_none_or(|k| k == r.kind)
                        && scope.as_ref().is_none_or(|s| s == &r.scope)
                })
            };
            result.problems.retain(|p| matches(p.rule));
            result.notes.retain(|n| n.rule.is_some_and(matches));
        }
        Ok(result)
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

pub struct RulesApplicable;

impl Command for RulesApplicable {
    const META: Meta = Meta {
        id: "pcb.rules.applicable",
        title: "Applicable Rules",
        description: "Explains each field on one or two Tracks, Vias, Free Pads, Zones, \
                      placed Pads or Nets. Distinct Layer results list their copper Layer UUIDs. \
                      Each kind names its winners, overridden and disabled records, and severity. \
                      For two objects, between names the winning clearance pair or object.",
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
        use crate::rules::explain::{Measured, applicable_objects};
        let objects = p
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| {
                match o {
                    RuleObject::Object(id) => Measured::object(pcb, *id),
                    RuleObject::Pad(pad) => Measured::placed(pcb, *pad),
                }
                .ok_or_else(|| {
                    CommandError::invalid_params(
                        &format!("/objects/{i}"),
                        "unknown copper object or Net",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ApplicableResult {
            kinds: applicable_objects(pcb, &objects),
            nets: objects.iter().map(|o| o.net).collect(),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AffectedParams {
    #[serde(default)]
    pub kind: Option<RuleKind>,
    #[serde(default)]
    pub scope: Option<ScopeParam>,
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
        description: "Lists the Nets, objects and placed Pads a rule supplies values to on \
                      their copper Layers. Select a record by rule UUID or by kind and scope, \
                      or select all records of a Net Class by net_class UUID.",
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
        let records: Vec<&DesignRule> = match (p.rule, p.net_class, p.kind, p.scope) {
            (None, None, Some(kind), scope) => {
                let scope = scope.unwrap_or_default().resolve(pcb)?;
                pcb.scoped_rule(kind, &scope).into_iter().collect()
            }
            (Some(rule), None, None, None) => vec![pcb.rule(rule).ok_or_else(|| {
                CommandError::not_found(format!("no rule record has UUID {rule} (`rule`)"))
            })?],
            (None, Some(class), None, None) => {
                if pcb.net_class(class).is_none() {
                    return Err(CommandError::not_found(format!(
                        "no Net Class has UUID {class} (`net_class`)"
                    )));
                }
                pcb.rules()
                    .filter(|r| {
                        r.scope.net_class == Some(class)
                            || r.scope.net_class_pair.is_some_and(|p| p.contains(&class))
                    })
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
                      The Inner limits are included. Other Layer and class records are omitted, as \
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
