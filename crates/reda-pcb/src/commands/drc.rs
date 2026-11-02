//! The DRC Commands: running it, telling whether a run is stale, the severities and the
//! Waivers. They mirror ERC's (`erc.*`); the Waivers live in the PCB Document.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{DrcCheck, InDocument, Object, Severity, Uuid, Violation, Waiver, apply_waivers};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, pcb};
use crate::drc::{check, design_fingerprint};
use crate::zone;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(DrcRun);
    registry.add(DrcStale);
    registry.add(DrcSetSeverity);
    registry.add(DrcWaive);
    registry.add(DrcUnwaive);
}

/// The board's Violations now, ignoring Waivers, and whether the Zones had to be refilled
/// for them.
fn current(cx: &Context<'_>) -> Result<(Vec<Violation>, bool), CommandError> {
    let pcb = pcb(cx)?;
    let stored = cx.project()?.zone_fills(pcb.uuid);
    let refilled = zone::status(pcb, stored).needs_refill;
    let fills = zone::current(pcb, stored);
    Ok((check(pcb, &fills, cx.fonts()), refilled))
}

pub struct DrcRun;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcRunParams {
    /// Also list the Violations that Waivers waive, each naming its Waiver.
    #[serde(default)]
    pub include_waived: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcRunResult {
    /// The Violations, in the order of the DRC table.
    pub violations: Vec<Violation>,
    /// How many Violations Waivers waived.
    pub waived: usize,
    /// Waivers that match no Violation any more, because their objects no longer break
    /// the check.
    pub unmatched_waivers: Vec<Uuid>,
    /// Whether some Zone was unfilled or stale, so the run checked a fresh fill. The fill
    /// is not stored: run `pcb.zone.fill` to keep it.
    pub refilled: bool,
    /// A fingerprint of the board the run checked. Pass it to `drc.stale` to learn
    /// whether the board was edited since.
    pub design: String,
}

impl Command for DrcRun {
    const META: Meta = Meta {
        id: "drc.run",
        title: "Run DRC",
        description: "Checks the PCB with the DRC checks that are not Off, after refilling \
                      stale Zones, and returns the Violations: each with its check id, \
                      severity, message arguments and the UUIDs of the objects involved \
                      (a Pad as its Footprint then the Pad). Waived Violations are left out \
                      unless `include_waived`.",
        category: "pcb",
        icon: "drc",
        kind: Kind::Query,
    };
    type Params = DrcRunParams;
    type Output = DrcRunResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcRunParams) -> Result<DrcRunResult, CommandError> {
        let (violations, refilled) = current(cx)?;
        let pcb = pcb(cx)?;
        let waived = apply_waivers(violations, pcb.waivers(), p.include_waived);
        Ok(DrcRunResult {
            violations: waived.violations,
            waived: waived.waived,
            unmatched_waivers: waived.unmatched,
            refilled,
            design: design_fingerprint(pcb),
        })
    }
}

pub struct DrcStale;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcStaleParams {
    /// The `design` fingerprint a `drc.run` returned.
    pub design: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcStaleResult {
    /// Whether the board was edited since that run, so its results are stale.
    pub stale: bool,
}

impl Command for DrcStale {
    const META: Meta = Meta {
        id: "drc.stale",
        title: "Are DRC Results Stale",
        description: "Tells whether the PCB, its rules, its DRC severities or its Waivers \
                      changed since the `drc.run` that returned `design`.",
        category: "pcb",
        icon: "drc",
        kind: Kind::Query,
    };
    type Params = DrcStaleParams;
    type Output = DrcStaleResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcStaleParams) -> Result<DrcStaleResult, CommandError> {
        Ok(DrcStaleResult {
            stale: design_fingerprint(pcb(cx)?) != p.design,
        })
    }
}

pub struct DrcSetSeverity;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcSetSeverityParams {
    /// The DRC check.
    pub check: DrcCheck,
    pub severity: Severity,
}

impl Command for DrcSetSeverity {
    const META: Meta = Meta {
        id: "drc.set_severity",
        title: "Set DRC Severity",
        description: "Sets how one DRC check reports its Violations: `error`, `warning` or \
                      `off`. Stored in the PCB Document, next to its rules.",
        category: "pcb",
        icon: "severity",
        kind: Kind::Edit,
    };
    type Params = DrcSetSeverityParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcSetSeverityParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut setup = pcb.setup().clone();
        setup.severities.set(p.check, p.severity);
        cx.tx()?
            .replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        Ok(())
    }
}

pub struct DrcWaive;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcWaiveParams {
    /// The check id of the Violation, such as `clearance`.
    pub check: DrcCheck,
    /// Exactly the objects of the Violation, in any order.
    pub objects: Vec<Uuid>,
    /// Why it is waived.
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcWaiveResult {
    /// The new Waiver.
    pub waiver: Uuid,
    /// The PCB Document that holds it.
    pub document: Uuid,
}

impl Command for DrcWaive {
    const META: Meta = Meta {
        id: "drc.waive",
        title: "Waive DRC Violation",
        description: "Waives one current DRC Violation, named by its check and exactly its \
                      objects, with an optional comment. The Waiver is stored in the PCB \
                      Document and stops matching once those objects no longer break the \
                      check.",
        category: "pcb",
        icon: "waive",
        kind: Kind::Edit,
    };
    type Params = DrcWaiveParams;
    type Output = DrcWaiveResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DrcWaiveParams) -> Result<DrcWaiveResult, CommandError> {
        let probe = Violation::new(p.check.id(), Severity::Error).objects(p.objects.clone());
        let candidate = Waiver::of(Uuid::from_u128(0), &probe, "");
        let (violations, _) = current(cx)?;
        let Some(violation) = violations.into_iter().find(|v| candidate.matches(v)) else {
            return Err(CommandError::rejected(format!(
                "no current {} Violation has exactly these objects",
                p.check.id()
            )));
        };
        let pcb = pcb(cx)?;
        if let Some(existing) = pcb.waivers().find(|w| w.matches(&violation)) {
            return Err(CommandError::rejected(format!(
                "Waiver {} already waives this Violation",
                existing.uuid
            )));
        }
        let document = pcb.uuid;
        let waiver = Waiver::of(cx.new_id(), &violation, p.comment);
        let uuid = waiver.uuid;
        cx.tx()?
            .insert(Object::Waiver(InDocument::new(document, waiver)))?;
        Ok(DrcWaiveResult {
            waiver: uuid,
            document,
        })
    }
}

pub struct DrcUnwaive;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrcUnwaiveParams {
    /// The Waivers to delete.
    pub waivers: Vec<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DrcUnwaiveResult {
    /// The deleted Waivers.
    pub waivers: Vec<Uuid>,
}

impl Command for DrcUnwaive {
    const META: Meta = Meta {
        id: "drc.unwaive",
        title: "Delete DRC Waivers",
        description: "Deletes Waivers of DRC Violations, so the Violations they waived are \
                      reported again.",
        category: "pcb",
        icon: "waive",
        kind: Kind::Edit,
    };
    type Params = DrcUnwaiveParams;
    type Output = DrcUnwaiveResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: DrcUnwaiveParams,
    ) -> Result<DrcUnwaiveResult, CommandError> {
        let pcb = pcb(cx)?;
        for (i, &uuid) in p.waivers.iter().enumerate() {
            if pcb.waiver(uuid).is_none() {
                return Err(CommandError::invalid_params(
                    &format!("/waivers/{i}"),
                    format!("{uuid} is not a Waiver on the PCB"),
                ));
            }
        }
        let tx = cx.tx()?;
        for &uuid in &p.waivers {
            tx.remove(uuid)?;
        }
        Ok(DrcUnwaiveResult { waivers: p.waivers })
    }
}
