//! Update from Library on the board: `pcb.footprint.update_preview` and
//! `pcb.footprint.update_from_library`.
//!
//! A Mechanical Part placed straight onto the PCB holds only a Footprint copy and its
//! instance's fields, with no link to its Part. So these compare that copy with the Library
//! Footprint of the same UUID by hash, and an update swaps the copy in place: position,
//! rotation, side, lock and Designator text placement stay, and fields aren't touched.
//! Instances with a Component update through the schematic and Sync.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Fingerprint, FootprintCopyData, Kind,
    Meta, Registry, Unavailable,
};
use reda_model::local_edit::LocalEdit;
use reda_model::{FootprintCopy, FootprintInstance, InDocument, Object, PadNet, PcbDocument, Uuid};
use reda_schematic::{LibraryStatus, PartChange, footprint_change, natural_cmp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::local_edits::{needs_pcb, pcb, prune_copies};
use crate::sync::lost_local_edits_of;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(FootprintUpdatePreview);
    registry.add(FootprintUpdateFromLibrary);
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintUpdateParams {
    /// The Footprint instances to update, by UUID: Mechanical Parts placed on the PCB, with
    /// no Component. Every such instance by default.
    pub footprints: Option<Vec<Uuid>>,
}

/// What an update changes on one placed Footprint.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FootprintUpdateRow {
    /// The Footprint instance.
    pub instance: Uuid,
    pub designator: String,
    /// The Library Footprint, by UUID, and its name in the board's copy.
    pub footprint: Uuid,
    pub name: String,
    /// `library_differs`, or `source_missing`, which cannot be updated.
    pub status: LibraryStatus,
    /// For Source Missing: what is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The Footprint's difference: the Pad numbers added or removed and the Pads moved or
    /// resized.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<PartChange>,
    /// The local Pad and graphics edits of this Locally Edited Footprint that the update
    /// removes, each with its Pad, property and values. Omitted when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lost_local_edits: Vec<LocalEdit>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FootprintUpdateResult {
    /// The instances in scope that are not Up to Date, by Designator in natural order.
    pub footprints: Vec<FootprintUpdateRow>,
    /// Pass it to `pcb.footprint.update_from_library` with the same `footprints`.
    pub preview_hash: String,
}

/// One instance's update: its row, and the Library copy it takes.
struct Planned {
    row: FootprintUpdateRow,
    copy: Option<FootprintCopyData>,
}

/// The board copy an instance's copy came from: its `edited_from` copy, or itself.
fn origin<'a>(pcb: &'a PcbDocument, copy: &'a FootprintCopy) -> &'a FootprintCopy {
    copy.edited_from
        .as_ref()
        .and_then(|from| pcb.footprint_copy(from.uuid))
        .unwrap_or(copy)
}

fn plan(cx: &Context<'_>, p: &FootprintUpdateParams) -> Result<Vec<Planned>, CommandError> {
    let pcb = pcb(cx)?;
    let mut instances: Vec<&FootprintInstance> = match &p.footprints {
        None => pcb.footprints().filter(|i| i.component.is_none()).collect(),
        Some(uuids) => {
            let mut chosen = Vec::with_capacity(uuids.len());
            for (i, uuid) in uuids.iter().enumerate() {
                let instance = pcb.footprint(*uuid).ok_or_else(|| {
                    CommandError::invalid_params(
                        &format!("/footprints/{i}"),
                        format!("the PCB has no Footprint instance {uuid}"),
                    )
                })?;
                if instance.component.is_some() {
                    return Err(CommandError::rejected(format!(
                        "{} has a Component: update it from the schematic \
                         (schematic.component.update_preview), then Sync",
                        instance.designator
                    )));
                }
                if !chosen.iter().any(|c: &&FootprintInstance| c.uuid == *uuid) {
                    chosen.push(instance);
                }
            }
            chosen
        }
    };
    instances.sort_by(|a, b| natural_cmp(&a.designator, &b.designator).then(a.uuid.cmp(&b.uuid)));
    let copies: Vec<&FootprintCopy> = (instances.iter())
        .map(|i| {
            pcb.footprint_copy(i.footprint)
                .ok_or_else(|| CommandError::internal("a Footprint instance's copy is gone"))
        })
        .collect::<Result<_, _>>()?;
    let wanted: Vec<Uuid> = copies.iter().map(|c| c.footprint.uuid).collect();
    let library = cx.library_footprints(&wanted);
    let mut out = vec![];
    for ((instance, copy), found) in instances.into_iter().zip(copies).zip(library) {
        let from = origin(pcb, copy);
        let mut row = FootprintUpdateRow {
            instance: instance.uuid,
            designator: instance.designator.clone(),
            footprint: copy.footprint.uuid,
            name: copy.footprint.name.clone(),
            status: LibraryStatus::LibraryDiffers,
            reason: None,
            changes: vec![],
            lost_local_edits: vec![],
        };
        match found {
            Err(e) => {
                row.status = LibraryStatus::SourceMissing;
                row.reason = Some(e.message);
                out.push(Planned { row, copy: None });
            }
            Ok(new) if new.hash == from.hash => {}
            Ok(new) => {
                if from.footprint != new.footprint {
                    row.changes
                        .push(footprint_change(&from.footprint, &new.footprint));
                }
                row.lost_local_edits = lost_local_edits_of(pcb, copy);
                out.push(Planned {
                    row,
                    copy: Some(new),
                });
            }
        }
    }
    Ok(out)
}

fn preview_hash(plans: &[Planned]) -> String {
    let mut f = Fingerprint::new();
    for plan in plans {
        f.add(&plan.row).add(&plan.copy.as_ref().map(|c| &c.hash));
    }
    f.finish()
}

pub struct FootprintUpdatePreview;

impl Command for FootprintUpdatePreview {
    const META: Meta = Meta {
        id: "pcb.footprint.update_preview",
        title: "Preview Update from Library",
        description: "Shows what PCB ▸ Update from Library would change, without changing \
                      anything: each Mechanical Part placed straight onto the board (no \
                      Component) whose Footprint copy differs by hash from the Library \
                      Footprint with its UUID, with the Pads added, removed, moved or \
                      resized and the local Pad edits the update removes. A Footprint no \
                      registered Library holds is `source_missing`. Instances with a \
                      Component update through `schematic.component.update_preview` and \
                      Sync. Apply it with `pcb.footprint.update_from_library`.",
        category: "pcb",
        icon: "library_update",
        kind: Kind::Query,
    };
    type Params = FootprintUpdateParams;
    type Output = FootprintUpdateResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FootprintUpdateParams,
    ) -> Result<FootprintUpdateResult, CommandError> {
        let plans = plan(cx, &p)?;
        Ok(FootprintUpdateResult {
            preview_hash: preview_hash(&plans),
            footprints: plans.into_iter().map(|p| p.row).collect(),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintUpdateFromLibraryParams {
    /// The `preview_hash` of the `pcb.footprint.update_preview` this applies.
    pub preview_hash: String,
    /// As for the preview.
    pub footprints: Option<Vec<Uuid>>,
    /// Instances of the preview left as they are.
    #[serde(default)]
    pub excluded: Vec<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FootprintUpdateApplied {
    /// The instances updated, by Designator in natural order.
    pub updated: Vec<Uuid>,
    /// The instances of the preview left as they were: excluded or Source Missing.
    pub not_updated: Vec<Uuid>,
}

pub struct FootprintUpdateFromLibrary;

impl Command for FootprintUpdateFromLibrary {
    const META: Meta = Meta {
        id: "pcb.footprint.update_from_library",
        title: "Update from Library",
        description: "Applies the update `pcb.footprint.update_preview` showed, given its \
                      `preview_hash` and the same `footprints`; fails with `conflict` when \
                      the result no longer matches. Each Footprint takes the Library's copy \
                      in place, keeping position, rotation, side, lock, Designator text \
                      placement and Pad Nets; its fields aren't touched, and local Pad edits \
                      go. `excluded` instances and Source Missing ones stay as they are.",
        category: "pcb",
        icon: "library_update",
        kind: Kind::Edit,
    };
    type Params = FootprintUpdateFromLibraryParams;
    type Output = FootprintUpdateApplied;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FootprintUpdateFromLibraryParams,
    ) -> Result<FootprintUpdateApplied, CommandError> {
        let plans = plan(
            cx,
            &FootprintUpdateParams {
                footprints: p.footprints.clone(),
            },
        )?;
        if preview_hash(&plans) != p.preview_hash {
            return Err(CommandError::conflict(
                "the update no longer matches the preview; run pcb.footprint.update_preview \
                 again",
            ));
        }
        for (i, uuid) in p.excluded.iter().enumerate() {
            if !plans.iter().any(|plan| plan.row.instance == *uuid) {
                return Err(CommandError::invalid_params(
                    &format!("/excluded/{i}"),
                    format!("Footprint instance {uuid} is not in the preview"),
                ));
            }
        }
        let doc = pcb(cx)?.uuid;
        let (mut updated, mut not_updated) = (vec![], vec![]);
        for plan in plans {
            let Some(new) = plan
                .copy
                .filter(|_| !p.excluded.contains(&plan.row.instance))
            else {
                not_updated.push(plan.row.instance);
                continue;
            };
            let board = pcb(cx)?;
            let mut instance = board
                .footprint(plan.row.instance)
                .expect("a planned instance")
                .clone();
            let old = board
                .footprint_copy(instance.footprint)
                .expect("a planned copy")
                .footprint
                .clone();
            let existing = board
                .footprint_copy_for(new.footprint.uuid, &new.hash)
                .map(|c| c.uuid);
            let copy = match existing {
                Some(uuid) => uuid,
                None => {
                    let uuid = cx.new_id();
                    let copy = FootprintCopy {
                        uuid,
                        hash: new.hash,
                        edited_from: None,
                        footprint: new.footprint.clone(),
                    };
                    cx.tx()?
                        .insert(Object::FootprintCopy(InDocument::new(doc, copy)))?;
                    uuid
                }
            };
            reda_pcb::placed::remap_text_overrides(&mut instance, &old, &new.footprint);
            let mut pads: Vec<PadNet> = (new.footprint.pads.iter())
                .map(|pad| PadNet {
                    pad: pad.uuid,
                    net: instance.pad_net(pad.uuid),
                })
                .collect();
            pads.sort_by_key(|p| p.pad);
            instance.footprint = copy;
            instance.pads = pads;
            cx.tx()?
                .replace(Object::FootprintInstance(InDocument::new(doc, instance)))?;
            updated.push(plan.row.instance);
        }
        prune_copies(cx)?;
        Ok(FootprintUpdateApplied {
            updated,
            not_updated,
        })
    }
}
