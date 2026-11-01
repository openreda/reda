//! The board-wide design rules and the DRC severities.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{DrcCheck, InDocument, Object, Project, Rules, Severity, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, optional_non_negative, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(RulesSet);
    registry.add(SeveritySet);
    registry.add(FabPresetDiff);
    registry.add(FabPresetApply);
}

pub struct RulesSet;

/// The rules to change; the others keep their values.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulesSetParams {
    /// Minimum clearance between copper of different Nets.
    #[serde(default)]
    pub clearance: Option<Length>,
    /// Minimum Track width.
    #[serde(default)]
    pub track_width: Option<Length>,
    /// Minimum Via diameter.
    #[serde(default)]
    pub via_diameter: Option<Length>,
    /// Minimum Via hole.
    #[serde(default)]
    pub via_hole: Option<Length>,
    /// Minimum annular ring.
    #[serde(default)]
    pub annular_ring: Option<Length>,
    /// Minimum spacing between hole edges.
    #[serde(default)]
    pub hole_to_hole: Option<Length>,
    /// Minimum clearance from copper to the board edge.
    #[serde(default)]
    pub edge_clearance: Option<Length>,
}

impl Command for RulesSet {
    const META: Meta = Meta {
        id: "pcb.rules.set",
        title: "Set Design Rules",
        description: "Sets board-wide design rules: any of `clearance`, `track_width`, \
                      `via_diameter`, `via_hole`, `annular_ring`, `hole_to_hole` and \
                      `edge_clearance`. Rules left out keep their values. Returns the rules.",
        category: "pcb",
        icon: "rules",
        kind: Kind::Edit,
    };
    type Params = RulesSetParams;
    type Output = Rules;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: RulesSetParams) -> Result<Rules, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut setup = pcb.setup().clone();
        let rules = &mut setup.rules;
        let fields = [
            ("/clearance", p.clearance, &mut rules.clearance),
            ("/track_width", p.track_width, &mut rules.track_width),
            ("/via_diameter", p.via_diameter, &mut rules.via_diameter),
            ("/via_hole", p.via_hole, &mut rules.via_hole),
            ("/annular_ring", p.annular_ring, &mut rules.annular_ring),
            ("/hole_to_hole", p.hole_to_hole, &mut rules.hole_to_hole),
            (
                "/edge_clearance",
                p.edge_clearance,
                &mut rules.edge_clearance,
            ),
        ];
        for (pointer, value, rule) in fields {
            if let Some(value) = optional_non_negative(pointer, value)? {
                *rule = value;
            }
        }
        let rules = setup.rules;
        cx.tx()?
            .replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        Ok(rules)
    }
}

pub struct SeveritySet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeveritySetParams {
    /// The DRC check.
    pub check: DrcCheck,
    pub severity: Severity,
}

impl Command for SeveritySet {
    const META: Meta = Meta {
        id: "pcb.drc.set_severity",
        title: "Set DRC Severity",
        description: "Sets how one DRC check reports its Violations: `error`, `warning` or \
                      `off`.",
        category: "pcb",
        icon: "severity",
        kind: Kind::Edit,
    };
    type Params = SeveritySetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: SeveritySetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut setup = pcb.setup().clone();
        setup.severities.set(p.check, p.severity);
        cx.tx()?
            .replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        Ok(())
    }
}

/// Names one of the Project's Fab Preset copies.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetRef {
    /// The Project copy's UUID.
    pub preset: Uuid,
}

/// One rule the preset would change.
#[derive(Debug, PartialEq, Serialize, JsonSchema)]
pub struct RuleChange {
    /// The rule, as `pcb.rules.set` names it.
    pub rule: &'static str,
    /// The board's value now.
    pub board: Nm,
    /// The preset's value.
    pub preset: Nm,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuleChanges {
    /// Every rule that differs, in `pcb.rules.set` order; empty when they all match.
    pub changes: Vec<RuleChange>,
}

/// The rules where `board` and `preset` differ.
fn rule_changes(board: &Rules, preset: &Rules) -> Vec<RuleChange> {
    let pairs = [
        ("clearance", board.clearance, preset.clearance),
        ("track_width", board.track_width, preset.track_width),
        ("via_diameter", board.via_diameter, preset.via_diameter),
        ("via_hole", board.via_hole, preset.via_hole),
        ("annular_ring", board.annular_ring, preset.annular_ring),
        ("hole_to_hole", board.hole_to_hole, preset.hole_to_hole),
        (
            "edge_clearance",
            board.edge_clearance,
            preset.edge_clearance,
        ),
    ];
    pairs
        .into_iter()
        .filter(|(_, b, p)| b != p)
        .map(|(rule, board, preset)| RuleChange {
            rule,
            board,
            preset,
        })
        .collect()
}

fn preset_rules(project: &Project, uuid: Uuid) -> Result<Rules, CommandError> {
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
        description: "Lists the board-wide design rules that applying one of the Project's \
                      Fab Preset copies would change, with the board's and the preset's \
                      values. `pcb.rules.apply_fab_preset` applies them.",
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
        let board = pcb(cx)?.setup().rules;
        Ok(RuleChanges {
            changes: rule_changes(&board, &preset),
        })
    }
}

pub struct FabPresetApply;

impl Command for FabPresetApply {
    const META: Meta = Meta {
        id: "pcb.rules.apply_fab_preset",
        title: "Apply Fab Preset Rules",
        description: "Re-seeds the board-wide design rules from one of the Project's Fab \
                      Preset copies, after `pcb.rules.fab_preset_diff` has shown what \
                      changes. Net Class overrides and DRC severities are kept. Returns the \
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
        let mut setup = pcb.setup().clone();
        let changes = rule_changes(&setup.rules, &preset);
        if !changes.is_empty() {
            setup.rules = preset;
            cx.tx()?
                .replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        }
        Ok(RuleChanges { changes })
    }
}
