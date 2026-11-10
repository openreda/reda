//! Locally Edited Footprints (ADR-0013): `pcb.pad.edit` changes one Pad of one placed
//! Footprint, and `pcb.footprint.graphic.edit` and `.delete` its graphics once
//! `pcb.footprint.unlock_graphics` has unlocked them, by giving the instance its own
//! part-table entry, keyed by the new content hash and recording `edited_from`;
//! `pcb.footprint.revert_local_edits` puts instances back on that copy. The PCB's table keeps a copy while an entry's `edited_from` names
//! it ([`prune_copies`]).

use std::collections::BTreeSet;

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, HoleParam, Kind, Length, Meta,
    NO_PROJECT, RatioParam, Registry, Unavailable, ZoneConnectionParam,
};
use reda_library::edit::{GraphicPatch, PadPatch, PointParam, ShapeParam, present};
use reda_model::{
    Corners, EditedFrom, Footprint, FootprintCopy, FootprintInstance, FootprintLayer, InDocument,
    Object, PadKind, PadShape, PcbDocument, Side, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::library::ContentHash;
use crate::sync::NO_PCB;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(PcbPadEdit);
    registry.add(RevertLocalEdits);
    registry.add(UnlockGraphics);
    registry.add(GraphicEdit);
    registry.add(GraphicDelete);
}

pub(crate) fn needs_pcb(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    let reason = match cx.project {
        None => NO_PROJECT,
        Some(project) if project.pcb().is_none() => NO_PCB,
        Some(_) => return Ok(()),
    };
    Err(Unavailable {
        reason: reason.into(),
    })
}

pub(crate) fn pcb<'a>(cx: &'a Context<'_>) -> Result<&'a PcbDocument, CommandError> {
    cx.project()?
        .pcb()
        .ok_or_else(|| CommandError::unavailable(NO_PCB))
}

/// The content hash of a Footprint as a Document copies it: over its canonical file bytes,
/// as [`crate::library::copy_part`] hashes a Library Footprint.
pub(crate) fn footprint_hash(footprint: &Footprint) -> Result<String, CommandError> {
    let bytes = crate::format::library::write_footprint(footprint)
        .map_err(|e| CommandError::internal(e.to_string()))?;
    let mut hasher = ContentHash::new();
    hasher.write(&bytes);
    Ok(hasher.finish())
}

/// The copy a PCB copy came from: its `edited_from` copy, or itself when it isn't edited.
fn origin<'a>(
    pcb: &'a PcbDocument,
    copy: &'a FootprintCopy,
) -> Result<&'a FootprintCopy, CommandError> {
    match &copy.edited_from {
        None => Ok(copy),
        Some(from) => pcb.footprint_copy(from.uuid).ok_or_else(|| {
            CommandError::internal(format!(
                "Footprint copy {} is edited from {}, which the PCB does not hold",
                copy.uuid, from.uuid
            ))
        }),
    }
}

/// Puts Footprint instance `instance` on a copy holding `edited`, a local edit of the copy
/// it is on: the copy it came from when `edited` matches that again, an entry with the
/// same edits when another instance has one, or else a new entry. Unused copies go.
pub(crate) fn place_edited(
    cx: &mut Context<'_>,
    instance: Uuid,
    edited: Footprint,
) -> Result<(), CommandError> {
    let pcb = pcb(cx)?;
    let doc = pcb.uuid;
    let mut placed = pcb
        .footprint(instance)
        .ok_or_else(|| {
            CommandError::not_found(format!("the PCB has no Footprint instance {instance}"))
        })?
        .clone();
    let current = pcb
        .footprint_copy(placed.footprint)
        .ok_or_else(|| CommandError::internal("a Footprint instance's copy is gone"))?;
    let origin = origin(pcb, current)?;
    let from = EditedFrom {
        uuid: origin.uuid,
        hash: origin.hash.clone(),
    };
    let hash = footprint_hash(&edited)?;
    let same = pcb
        .footprint_copies()
        .find(|c| c.footprint.uuid == edited.uuid && c.hash == hash)
        .map(|c| (c.uuid, c.edited_from.as_ref().map(|e| e.uuid)));
    let target = match same {
        Some((uuid, _)) if uuid == from.uuid => uuid,
        Some((uuid, Some(edited_from))) if edited_from == from.uuid => uuid,
        // Another version's copy (or an edit of one) already has this content. The table
        // is keyed by Footprint UUID + hash, so this edit can't have an entry of its own,
        // and sharing that one would drop the `edited_from` Sync and revert rely on.
        Some(_) => {
            return Err(CommandError::conflict(
                "the edit makes this Footprint the same as another copy of it on the board; \
                 Sync that version instead",
            ));
        }
        None => {
            let copy = FootprintCopy {
                uuid: cx.new_id(),
                hash,
                edited_from: Some(from),
                footprint: edited,
            };
            let uuid = copy.uuid;
            cx.tx()?
                .insert(Object::FootprintCopy(InDocument::new(doc, copy)))?;
            uuid
        }
    };
    if placed.footprint != target {
        placed.footprint = target;
        cx.tx()?
            .replace(Object::FootprintInstance(InDocument::new(doc, placed)))?;
    }
    prune_copies(cx)
}

/// Removes the PCB's Footprint copies that no instance uses and no used entry's
/// `edited_from` names.
pub(crate) fn prune_copies(cx: &mut Context<'_>) -> Result<(), CommandError> {
    let pcb = pcb(cx)?;
    let used: BTreeSet<Uuid> = pcb.footprints().map(|i| i.footprint).collect();
    let named: BTreeSet<Uuid> = pcb
        .footprint_copies()
        .filter(|c| used.contains(&c.uuid))
        .filter_map(|c| Some(c.edited_from.as_ref()?.uuid))
        .collect();
    let unused: Vec<Uuid> = pcb
        .footprint_copies()
        .map(|c| c.uuid)
        .filter(|u| !used.contains(u) && !named.contains(u))
        .collect();
    for uuid in unused {
        cx.tx()?.remove(uuid)?;
    }
    Ok(())
}

pub struct PcbPadEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbPadEditParams {
    /// The placed Footprint instance.
    pub instance: Uuid,
    /// The Pad, by its UUID in the Footprint.
    pub pad: Uuid,
    pub kind: Option<PadKind>,
    pub shape: Option<PadShape>,
    /// In the Footprint's own frame.
    pub at: Option<PointParam>,
    /// In the Footprint's own frame.
    pub rotation: Option<Angle>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    /// `null` returns a rounded rectangle to the default ratio.
    #[serde(default, deserialize_with = "present")]
    pub corner_ratio: Option<Option<RatioParam>>,
    /// `null` removes it.
    #[serde(default, deserialize_with = "present")]
    pub chamfer_ratio: Option<Option<RatioParam>>,
    /// `null` returns to all four corners.
    #[serde(default, deserialize_with = "present")]
    pub corners: Option<Option<Corners>>,
    /// `null` puts the copper back on the anchor.
    #[serde(default, deserialize_with = "present")]
    pub offset: Option<Option<PointParam>>,
    /// `null` removes the hole.
    #[serde(default, deserialize_with = "present")]
    pub hole: Option<Option<HoleParam>>,
    /// `null` returns an SMD Pad to the Footprint's top.
    #[serde(default, deserialize_with = "present")]
    pub side: Option<Option<Side>>,
    /// `null` inherits the board's rule.
    #[serde(default, deserialize_with = "present")]
    pub mask: Option<Option<reda_command::MaskParam>>,
    /// `null` inherits the board's rule.
    #[serde(default, deserialize_with = "present")]
    pub paste: Option<Option<reda_command::PasteParam>>,
    /// Replaces the whole Zone connection; `null` inherits every value.
    #[serde(default, deserialize_with = "present")]
    pub zone_connection: Option<Option<ZoneConnectionParam>>,
}

impl Command for PcbPadEdit {
    const META: Meta = Meta {
        id: "pcb.pad.edit",
        title: "Edit Placed Pad",
        description: "Changes one Pad of one placed Footprint: any of its kind, shape, \
                      position, rotation, size, corner and chamfer ratios and corners, \
                      offset, hole, side, mask and paste overrides and Zone connection, as \
                      `library.footprint.pad.edit` does, but never its number. Pads can't be \
                      added or removed on the board. The instance gets a Locally Edited \
                      Footprint: its own part-table entry, keyed by the new content hash, \
                      with `edited_from` naming the copy it came from. Instances with \
                      identical edits share one entry, and an edit that undoes every change \
                      returns the instance to that copy. Sync keeps local edits.",
        category: "pcb",
        icon: "pad",
        kind: Kind::Edit,
    };
    type Params = PcbPadEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: PcbPadEditParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let instance = pcb.footprint(p.instance).ok_or_else(|| {
            CommandError::not_found(format!("the PCB has no Footprint instance {}", p.instance))
        })?;
        let mut footprint = pcb
            .footprint_copy(instance.footprint)
            .ok_or_else(|| CommandError::internal("a Footprint instance's copy is gone"))?
            .footprint
            .clone();
        let index = footprint
            .pads
            .iter()
            .position(|pad| pad.uuid == p.pad)
            .ok_or_else(|| {
                CommandError::not_found(format!(
                    "Footprint {} has no Pad {}",
                    instance.designator, p.pad
                ))
            })?;
        let patch = PadPatch {
            kind: p.kind,
            shape: p.shape,
            at: p.at,
            rotation: p.rotation,
            width: p.width,
            height: p.height,
            corner_ratio: p.corner_ratio,
            chamfer_ratio: p.chamfer_ratio,
            corners: p.corners,
            offset: p.offset,
            hole: p.hole,
            side: p.side,
            mask: p.mask,
            paste: p.paste,
            zone_connection: p.zone_connection,
        };
        footprint.pads[index] = patch.apply(footprint.pads[index].clone())?;
        place_edited(cx, p.instance, footprint)
    }
}

pub struct RevertLocalEdits;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevertLocalEditsParams {
    /// The placed Footprint instances. Those without local edits are left alone.
    pub instances: Vec<Uuid>,
}

#[derive(Serialize, JsonSchema)]
pub struct RevertLocalEditsResult {
    /// The instances that had local edits and are back on their `edited_from` copy.
    pub reverted: Vec<Uuid>,
}

impl Command for RevertLocalEdits {
    const META: Meta = Meta {
        id: "pcb.footprint.revert_local_edits",
        title: "Revert Local Edits",
        description: "Returns Locally Edited Footprints to the copy their `edited_from` \
                      names, not to the Library: the board then matches the schematic's \
                      copy again. Instances without local edits are left alone. Entries no \
                      instance uses any more leave the part table.",
        category: "pcb",
        icon: "undo",
        kind: Kind::Edit,
    };
    type Params = RevertLocalEditsParams;
    type Output = RevertLocalEditsResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: RevertLocalEditsParams,
    ) -> Result<RevertLocalEditsResult, CommandError> {
        let pcb = pcb(cx)?;
        let doc = pcb.uuid;
        let mut reverted = Vec::new();
        for (i, uuid) in p.instances.iter().enumerate() {
            let instance = pcb.footprint(*uuid).ok_or_else(|| {
                CommandError::invalid_params(
                    &format!("/instances/{i}"),
                    format!("the PCB has no Footprint instance {uuid}"),
                )
            })?;
            let from = pcb
                .footprint_copy(instance.footprint)
                .and_then(|c| c.edited_from.as_ref());
            if let Some(from) = from
                && !reverted.iter().any(|r: &FootprintInstance| r.uuid == *uuid)
            {
                let mut back = instance.clone();
                back.footprint = from.uuid;
                reverted.push(back);
            }
        }
        let uuids = reverted.iter().map(|r| r.uuid).collect();
        for instance in reverted {
            cx.tx()?
                .replace(Object::FootprintInstance(InDocument::new(doc, instance)))?;
        }
        prune_copies(cx)?;
        Ok(RevertLocalEditsResult { reverted: uuids })
    }
}

pub struct UnlockGraphics;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnlockGraphicsParams {
    /// The placed Footprint instances.
    pub instances: Vec<Uuid>,
    /// True lets their graphics be edited on the board; false locks them again.
    pub unlocked: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct UnlockGraphicsResult {
    /// The instances whose setting changed, in the order given.
    pub instances: Vec<Uuid>,
}

impl Command for UnlockGraphics {
    const META: Meta = Meta {
        id: "pcb.footprint.unlock_graphics",
        title: "Unlock Footprint Graphics",
        description: "Unlocks the graphics of placed Footprints, so `pcb.footprint.graphic.edit` \
                      and `pcb.footprint.graphic.delete` can change them, or locks them again. \
                      Unlocking changes nothing else: locking again keeps the edits made, \
                      which `pcb.footprint.revert_local_edits` removes.",
        category: "pcb",
        icon: "lock",
        kind: Kind::Edit,
    };
    type Params = UnlockGraphicsParams;
    type Output = UnlockGraphicsResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: UnlockGraphicsParams,
    ) -> Result<UnlockGraphicsResult, CommandError> {
        let pcb = pcb(cx)?;
        let doc = pcb.uuid;
        let mut changed: Vec<FootprintInstance> = Vec::new();
        for (i, uuid) in p.instances.iter().enumerate() {
            let instance = pcb.footprint(*uuid).ok_or_else(|| {
                CommandError::invalid_params(
                    &format!("/instances/{i}"),
                    format!("the PCB has no Footprint instance {uuid}"),
                )
            })?;
            if instance.graphics_unlocked != p.unlocked && !changed.iter().any(|c| c.uuid == *uuid)
            {
                let mut instance = instance.clone();
                instance.graphics_unlocked = p.unlocked;
                changed.push(instance);
            }
        }
        let instances = changed.iter().map(|c| c.uuid).collect();
        for instance in changed {
            cx.tx()?
                .replace(Object::FootprintInstance(InDocument::new(doc, instance)))?;
        }
        Ok(UnlockGraphicsResult { instances })
    }
}

/// The Footprint instance `uuid` holds, cloned for a local edit of its graphics: refused
/// unless its graphics are unlocked or the caller passes `include_locked`.
fn unlocked_footprint(
    cx: &Context<'_>,
    uuid: Uuid,
    include_locked: bool,
) -> Result<Footprint, CommandError> {
    let pcb = pcb(cx)?;
    let instance = pcb.footprint(uuid).ok_or_else(|| {
        CommandError::not_found(format!("the PCB has no Footprint instance {uuid}"))
    })?;
    if !instance.graphics_unlocked && !include_locked {
        let mut err = CommandError::rejected(format!(
            "{}'s graphics are locked; unlock them with pcb.footprint.unlock_graphics, or \
             pass include_locked",
            instance.designator
        ));
        err.details = Some(serde_json::json!({ "locked": [uuid] }));
        return Err(err);
    }
    Ok(pcb
        .footprint_copy(instance.footprint)
        .ok_or_else(|| CommandError::internal("a Footprint instance's copy is gone"))?
        .footprint
        .clone())
}

fn no_graphic(footprint: &Footprint, graphic: Uuid) -> CommandError {
    CommandError::not_found(format!(
        "Footprint {} has no graphic {graphic}",
        footprint.name
    ))
}

pub struct GraphicEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicEditParams {
    /// The placed Footprint instance.
    pub instance: Uuid,
    /// The graphic, by its UUID in the Footprint.
    pub graphic: Uuid,
    pub layer: Option<FootprintLayer>,
    /// Replaces the whole shape, in the Footprint's own frame: moving a graphic is a new
    /// shape.
    pub shape: Option<ShapeParam>,
    pub width: Option<Length>,
    pub fill: Option<bool>,
    /// Edits a Footprint whose graphics aren't unlocked too, as the GUI does with Protect
    /// Locked Primitives In Component off.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for GraphicEdit {
    const META: Meta = Meta {
        id: "pcb.footprint.graphic.edit",
        title: "Edit Placed Footprint Graphic",
        description: "Changes one graphic of one placed Footprint whose graphics are \
                      unlocked: its Layer, shape (which also moves it), stroke width or fill, \
                      as `library.footprint.graphic.edit` does. The instance gets a Locally \
                      Edited Footprint, as `pcb.pad.edit` makes, with the same compare, revert \
                      and Sync rules.",
        category: "pcb",
        icon: "footprint_edit",
        kind: Kind::Edit,
    };
    type Params = GraphicEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: GraphicEditParams) -> Result<(), CommandError> {
        let mut footprint = unlocked_footprint(cx, p.instance, p.include_locked)?;
        let index = footprint
            .graphics
            .iter()
            .position(|g| g.uuid == p.graphic)
            .ok_or_else(|| no_graphic(&footprint, p.graphic))?;
        let patch = GraphicPatch {
            layer: p.layer,
            shape: p.shape,
            width: p.width,
            fill: p.fill,
        };
        footprint.graphics[index] = patch.apply(footprint.graphics[index].clone())?;
        place_edited(cx, p.instance, footprint)
    }
}

pub struct GraphicDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicDeleteParams {
    /// The placed Footprint instance.
    pub instance: Uuid,
    /// The graphics, by their UUIDs in the Footprint.
    pub graphics: Vec<Uuid>,
    /// Deletes from a Footprint whose graphics aren't unlocked too, as the GUI does with
    /// Protect Locked Primitives In Component off.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for GraphicDelete {
    const META: Meta = Meta {
        id: "pcb.footprint.graphic.delete",
        title: "Delete Placed Footprint Graphics",
        description: "Deletes graphics of one placed Footprint whose graphics are unlocked, \
                      in one step. The instance gets a Locally Edited Footprint, as \
                      `pcb.pad.edit` makes, with the same compare, revert and Sync rules.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = GraphicDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: GraphicDeleteParams) -> Result<(), CommandError> {
        let mut footprint = unlocked_footprint(cx, p.instance, p.include_locked)?;
        for graphic in &p.graphics {
            let index = footprint
                .graphics
                .iter()
                .position(|g| g.uuid == *graphic)
                .ok_or_else(|| no_graphic(&footprint, *graphic))?;
            footprint.graphics.remove(index);
        }
        place_edited(cx, p.instance, footprint)
    }
}
