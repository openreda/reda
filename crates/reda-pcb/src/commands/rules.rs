//! The board-wide design rules and the DRC severities.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::{DrcCheck, InDocument, Object, Rules, Severity};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{needs_pcb, optional_non_negative, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(RulesSet);
    registry.add(SeveritySet);
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
