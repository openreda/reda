//! Mechanical Parts placed straight onto the PCB from a Library: `pcb.mechanical.place`,
//! `pcb.mechanical.set` and `pcb.mechanical.delete`.
//!
//! They carry no Component link, so Sync ignores them, and their Designator, fields, In
//! BOM, In CPL and Not Fitted are edited here on the PCB.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::Millideg;
use reda_model::{
    ComponentType, FootprintCopy, FootprintInstance, InDocument, Object, PadNet, Project, Side,
    Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::outline::PointParam;
use super::{needs_pcb, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(MechanicalPlace);
    registry.add(MechanicalSet);
    registry.add(MechanicalDelete);
}

/// The Designator prefix of a Mechanical Part with no Symbol and no prefix keyword.
const FALLBACK_PREFIX: &str = "MP";

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MechanicalPlaceParams {
    /// The Mechanical Part, by UUID, from any registered Library.
    pub part: Uuid,
    /// Where its Footprint origin goes.
    pub at: PointParam,
    /// Counter-clockwise; 0 when left out.
    #[serde(default)]
    pub rotation: Option<Angle>,
    /// The board side; top when left out.
    #[serde(default)]
    pub side: Option<Side>,
    /// One of the Part's alternate Footprints, by UUID; its first when left out.
    #[serde(default)]
    pub footprint: Option<Uuid>,
    /// The Designator; by default the next free one for the Part's prefix.
    #[serde(default)]
    pub designator: Option<String>,
    /// Fields to set over the Part's defaults, by name (`value`, `mpn`… or a free-form
    /// name); null or empty clears a field.
    #[serde(default)]
    pub fields: BTreeMap<String, Option<String>>,
}

/// What [`MechanicalPlace`] returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MechanicalPlaced {
    /// The new Footprint instance.
    pub footprint: Uuid,
    pub designator: String,
    /// The PCB's copy of the Footprint it was placed from.
    pub footprint_copy: Uuid,
}

pub struct MechanicalPlace;

impl Command for MechanicalPlace {
    const META: Meta = Meta {
        id: "pcb.mechanical.place",
        title: "Place Mechanical Part",
        description: "Places a Mechanical Part (a mounting hole, fiducial and the like) from a \
                      Library straight onto the board, with no Component behind it; Sync \
                      leaves it alone. The Designator prefix is the Symbol's, else the Part's \
                      first keyword when it is in capitals, else `MP`. Standard Parts reach \
                      the board only through Sync.",
        category: "pcb",
        icon: "footprint",
        kind: Kind::Edit,
    };
    type Params = MechanicalPlaceParams;
    type Output = MechanicalPlaced;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: MechanicalPlaceParams,
    ) -> Result<MechanicalPlaced, CommandError> {
        let document = pcb(cx)?.uuid;
        let copy = cx.library_part(p.part)?;
        if copy.part.component_type != ComponentType::Mechanical {
            return Err(CommandError::rejected(format!(
                "{} is a {} Part: it needs a Symbol and a Footprint, and reaches the board \
                 through Sync from the schematic",
                copy.part.name,
                copy.part.component_type.name()
            )));
        }
        let chosen = match p.footprint {
            None => copy.footprints.first().ok_or_else(|| {
                CommandError::rejected(format!("{} has no Footprint", copy.part.name))
            })?,
            Some(uuid) => copy
                .footprints
                .iter()
                .find(|f| f.footprint.uuid == uuid)
                .ok_or_else(|| {
                    CommandError::invalid_params(
                        "/footprint",
                        format!("{uuid} is not one of {}'s Footprints", copy.part.name),
                    )
                })?,
        };
        if p.fields.keys().any(|k| k.trim().is_empty()) {
            return Err(CommandError::invalid_params(
                "/fields",
                "a field name is empty",
            ));
        }
        CommandError::check_field_texts("/fields", &p.fields)?;
        let mut fields = copy.part.fields.clone();
        for (name, value) in &p.fields {
            fields.set(name, value.as_deref().unwrap_or(""));
        }
        let project = cx.project()?;
        let designator = match p.designator {
            Some(d) => {
                let d = check_designator("/designator", &d)?;
                if designator_taken(project, &d, &[]) {
                    return Err(CommandError::conflict(format!("{d} is already in use")));
                }
                d
            }
            None => {
                let prefix = copy
                    .symbol
                    .as_ref()
                    .map(|s| s.designator_prefix.clone())
                    .filter(|p| !p.is_empty())
                    .or_else(|| keyword_prefix(&copy.part.fields.keywords))
                    .unwrap_or_else(|| FALLBACK_PREFIX.into());
                next_free_designator(project, &prefix)
            }
        };
        let existing = project
            .pcb()
            .and_then(|pcb| pcb.footprint_copy_for(chosen.footprint.uuid, &chosen.hash))
            .map(|c| c.uuid);
        let footprint_copy = match existing {
            Some(uuid) => uuid,
            None => {
                let uuid = cx.new_id();
                let copy = FootprintCopy {
                    uuid,
                    hash: chosen.hash.clone(),
                    edited_from: None,
                    footprint: chosen.footprint.clone(),
                };
                cx.tx()?
                    .insert(Object::FootprintCopy(InDocument::new(document, copy)))?;
                uuid
            }
        };
        let mut pads: Vec<PadNet> = chosen
            .footprint
            .pads
            .iter()
            .map(|pad| PadNet {
                pad: pad.uuid,
                net: None,
            })
            .collect();
        pads.sort_by_key(|p| p.pad);
        let uuid = cx.new_id();
        let instance = FootprintInstance {
            uuid,
            component: None,
            footprint: footprint_copy,
            at: p.at.point(),
            rotation: p
                .rotation
                .map_or(Millideg::ZERO, |r| r.millideg().normalized()),
            side: p.side.unwrap_or(Side::Top),
            locked: false,
            graphics_unlocked: false,
            designator: designator.clone(),
            fields,
            component_type: copy.part.component_type,
            in_bom: copy.part.in_bom,
            in_cpl: copy.part.in_cpl,
            not_fitted: false,
            text_overrides: Vec::new(),
            pads,
        };
        cx.tx()?.insert(Object::FootprintInstance(InDocument::new(
            document, instance,
        )))?;
        Ok(MechanicalPlaced {
            footprint: uuid,
            designator,
            footprint_copy,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MechanicalSetParams {
    /// The Mechanical Parts' Footprint instances, by UUID.
    pub footprints: Vec<Uuid>,
    /// A new Designator; only with a single Footprint.
    #[serde(default)]
    pub designator: Option<String>,
    /// A new value.
    #[serde(default)]
    pub value: Option<String>,
    /// Fields to set by name (`mpn`, `description`… or a free-form name); null or empty
    /// clears a field.
    #[serde(default)]
    pub fields: BTreeMap<String, Option<String>>,
    /// Whether they go into the BOM.
    #[serde(default)]
    pub in_bom: Option<bool>,
    /// Whether they go into the CPL.
    #[serde(default)]
    pub in_cpl: Option<bool>,
    #[serde(default)]
    pub not_fitted: Option<bool>,
}

pub struct MechanicalSet;

impl Command for MechanicalSet {
    const META: Meta = Meta {
        id: "pcb.mechanical.set",
        title: "Edit Mechanical Part",
        description: "Edits the Designator, value, fields, In BOM, In CPL or Not Fitted of \
                      Mechanical Parts placed on the PCB. Footprints synced from the schematic are edited \
                      with `pcb.footprint.set`, which writes through to the schematic.",
        category: "pcb",
        icon: "properties",
        kind: Kind::Edit,
    };
    type Params = MechanicalSetParams;
    type Output = super::placement::Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: MechanicalSetParams,
    ) -> Result<super::placement::Placed, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut instances = mechanical(cx.project()?, &p.footprints)?;
        if p.fields.keys().any(|k| k.trim().is_empty()) {
            return Err(CommandError::invalid_params(
                "/fields",
                "a field name is empty",
            ));
        }
        if let Some(value) = &p.value {
            CommandError::check_text("/value", value)?;
        }
        CommandError::check_field_texts("/fields", &p.fields)?;
        let designator = match &p.designator {
            None => None,
            Some(_) if instances.len() != 1 => {
                return Err(CommandError::invalid_params(
                    "/designator",
                    "a Designator can be set on one Footprint at a time",
                ));
            }
            Some(d) => {
                let d = check_designator("/designator", d)?;
                if designator_taken(cx.project()?, &d, &[instances[0].uuid]) {
                    return Err(CommandError::conflict(format!("{d} is already in use")));
                }
                Some(d)
            }
        };
        for f in &mut instances {
            if let Some(d) = &designator {
                f.designator.clone_from(d);
            }
            if let Some(v) = &p.value {
                f.fields.value.clone_from(v);
            }
            for (name, value) in &p.fields {
                f.fields.set(name, value.as_deref().unwrap_or(""));
            }
            if let Some(in_bom) = p.in_bom {
                f.in_bom = in_bom;
            }
            if let Some(in_cpl) = p.in_cpl {
                f.in_cpl = in_cpl;
            }
            if let Some(nf) = p.not_fitted {
                f.not_fitted = nf;
            }
        }
        let objects = instances.iter().map(|f| f.uuid).collect();
        let tx = cx.tx()?;
        for f in instances {
            tx.replace(Object::FootprintInstance(InDocument::new(document, f)))?;
        }
        Ok(super::placement::Placed { objects })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MechanicalDeleteParams {
    /// The Mechanical Parts' Footprint instances, by UUID.
    pub footprints: Vec<Uuid>,
    /// Deletes locked objects too; without it a locked one refuses the whole Command.
    #[serde(default)]
    pub include_locked: bool,
}

pub struct MechanicalDelete;

impl Command for MechanicalDelete {
    const META: Meta = Meta {
        id: "pcb.mechanical.delete",
        title: "Delete Mechanical Part",
        description: "Removes Mechanical Parts placed on the PCB, and any Footprint copy no \
                      Footprint uses any more. Footprints synced from the schematic go away \
                      by deleting their Component and syncing.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = MechanicalDeleteParams;
    type Output = super::placement::Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: MechanicalDeleteParams,
    ) -> Result<super::placement::Placed, CommandError> {
        let instances = mechanical(cx.project()?, &p.footprints)?;
        let gone: BTreeSet<Uuid> = instances.iter().map(|f| f.uuid).collect();
        let pcb = pcb(cx)?;
        crate::lock::refuse(pcb, gone.iter().copied(), p.include_locked, "delete")?;
        let still_used: BTreeSet<Uuid> = pcb
            .footprints()
            .filter(|f| !gone.contains(&f.uuid))
            .map(|f| f.footprint)
            .collect();
        let copies: BTreeSet<Uuid> = instances
            .iter()
            .map(|f| f.footprint)
            .filter(|c| !still_used.contains(c))
            .collect();
        let tx = cx.tx()?;
        for uuid in gone.iter().chain(&copies) {
            tx.remove(*uuid)?;
        }
        Ok(super::placement::Placed {
            objects: instances.iter().map(|f| f.uuid).collect(),
        })
    }
}

/// The Footprint instances named by `uuids`, each a Mechanical Part placed on the PCB, or
/// `invalid_params` at `/footprints/i`.
fn mechanical(project: &Project, uuids: &[Uuid]) -> Result<Vec<FootprintInstance>, CommandError> {
    let pcb = project
        .pcb()
        .ok_or_else(|| CommandError::unavailable("no PCB"))?;
    if uuids.is_empty() {
        return Err(CommandError::invalid_params(
            "/footprints",
            "nothing is selected",
        ));
    }
    let mut out: Vec<FootprintInstance> = Vec::new();
    for (i, &uuid) in uuids.iter().enumerate() {
        let pointer = format!("/footprints/{i}");
        match pcb.footprint(uuid) {
            None => {
                return Err(CommandError::invalid_params(
                    &pointer,
                    format!("{uuid} is not a Footprint on the board"),
                ));
            }
            Some(f) if f.component.is_some() => {
                return Err(CommandError::invalid_params(
                    &pointer,
                    format!(
                        "{} comes from the schematic; edit it with pcb.footprint.set",
                        f.designator
                    ),
                ));
            }
            Some(f) => {
                if !out.iter().any(|o| o.uuid == uuid) {
                    out.push(f.clone());
                }
            }
        }
    }
    Ok(out)
}

/// A Designator param, trimmed and not empty.
fn check_designator(pointer: &str, designator: &str) -> Result<String, CommandError> {
    let d = designator.trim();
    if d.is_empty() {
        return Err(CommandError::invalid_params(
            pointer,
            "the Designator is empty",
        ));
    }
    Ok(d.to_owned())
}

/// Every Designator in use: the schematic's Units and the PCB's Footprints, leaving out
/// the Footprints in `except`.
fn designators_in_use(project: &Project, except: &[Uuid]) -> BTreeSet<String> {
    let units = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units())
        .map(|u| u.designator.clone());
    let footprints = project
        .pcb()
        .into_iter()
        .flat_map(|pcb| pcb.footprints())
        .filter(|f| !except.contains(&f.uuid))
        .map(|f| f.designator.clone());
    units.chain(footprints).collect()
}

fn designator_taken(project: &Project, designator: &str, except: &[Uuid]) -> bool {
    designators_in_use(project, except).contains(designator)
}

/// The smallest `<prefix><n>` that neither the schematic nor the PCB uses.
fn next_free_designator(project: &Project, prefix: &str) -> String {
    let used: BTreeSet<u64> = designators_in_use(project, &[])
        .iter()
        .filter_map(|d| d.strip_prefix(prefix))
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|n| n.parse().ok())
        .collect();
    let n = (1..).find(|n| !used.contains(n)).expect("a free number");
    format!("{prefix}{n}")
}

/// A Mechanical Part's prefix from its keywords: the first word, when it is all capital
/// letters (`H mounting hole` → `H`, `FID fiducial` → `FID`).
fn keyword_prefix(keywords: &str) -> Option<String> {
    let first = keywords.split_whitespace().next()?;
    first
        .bytes()
        .all(|b| b.is_ascii_uppercase())
        .then(|| first.to_owned())
}
