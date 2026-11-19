//! Set the stored land-removal policy on a board or a mixed selection in one edit.
use std::collections::BTreeSet;

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{InDocument, Object, PadKind, PadRef, Scope, UnusedLayers, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::local_edits::{needs_pcb, place_edited};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(untagged)]
pub enum Target {
    /// A Via, Free Pad, or Footprint instance. An instance includes all its plated Pads.
    Object(Uuid),
    /// One Pad of one Footprint instance.
    Pad(PadRef),
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub document: Uuid,
    /// Apply to changed locked objects too. A matching mode needs no edit.
    #[serde(default)]
    pub include_locked: bool,
    /// Omit for the whole board. An empty selection changes nothing.
    pub objects: Option<BTreeSet<Target>>,
    /// Through-hole Pads support keep or remove. Their two solder lands always stay.
    pub pads: Option<UnusedLayers>,
    /// Vias also support remove_keep_ends to retain both ends of the span.
    pub vias: Option<UnusedLayers>,
}

#[derive(Serialize, JsonSchema)]
pub struct Counts {
    pub pads: usize,
    pub vias: usize,
}

pub(crate) struct Set;
impl Command for Set {
    const META: Meta = Meta {
        id: "pcb.unused_layers.set",
        title: "Set Unused Copper",
        description: "Choose which unconnected lands plated Pads and Vias retain on the board or selection. Returns the number of Pads and Vias changed.",
        category: "pcb",
        icon: "pad",
        kind: Kind::Edit,
    };
    type Params = Params;
    type Output = Counts;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: Params) -> Result<Counts, CommandError> {
        let project = cx.project()?;
        let pcb = project
            .pcb_document(p.document)
            .ok_or_else(|| CommandError::not_found(format!("no PCB Document {}", p.document)))?;
        if p.pads == Some(UnusedLayers::RemoveKeepEnds) {
            return Err(CommandError::invalid_params(
                "/pads",
                "Pads support keep or remove",
            ));
        }
        if p.pads.is_none() && p.vias.is_none() {
            return Err(CommandError::invalid_params(
                "/",
                "supply a mode for Pads, Vias, or both",
            ));
        }
        if let Some(objects) = &p.objects {
            for target in objects {
                let exists = match *target {
                    Target::Object(id) => project
                        .object(id)
                        .is_some_and(|o| o.scope() == Scope::Document(p.document)),
                    Target::Pad(reference) => pcb
                        .footprint(reference.footprint)
                        .and_then(|instance| pcb.footprint_copy(instance.footprint))
                        .is_some_and(|copy| {
                            copy.footprint
                                .pads
                                .iter()
                                .any(|pad| pad.uuid == reference.pad)
                        }),
                };
                if !exists {
                    return Err(CommandError::invalid_params(
                        "/objects",
                        format!("unknown board object {target:?}"),
                    ));
                }
            }
        }
        let includes = |target| {
            p.objects
                .as_ref()
                .is_none_or(|objects| objects.contains(&target))
        };
        let mut replacements = Vec::new();
        let mut footprints = Vec::new();
        let mut counts = Counts { pads: 0, vias: 0 };
        if let Some(mode) = p.pads {
            for free in pcb.free_pads().filter(|free| {
                free.kind == PadKind::PlatedThroughHole
                    && free.unused_layers != mode
                    && includes(Target::Object(free.uuid))
            }) {
                let mut free = free.clone();
                free.unused_layers = mode;
                replacements.push(Object::FreePad(InDocument::new(p.document, free)));
                counts.pads += 1;
            }
            for instance in pcb.footprints() {
                let Some(copy) = pcb.footprint_copy(instance.footprint) else {
                    continue;
                };
                let all = includes(Target::Object(instance.uuid));
                let mut edited = None;
                for (i, pad) in copy.footprint.pads.iter().enumerate() {
                    if pad.kind == PadKind::PlatedThroughHole
                        && pad.unused_layers != mode
                        && (all
                            || includes(Target::Pad(PadRef {
                                footprint: instance.uuid,
                                pad: pad.uuid,
                            })))
                    {
                        edited.get_or_insert_with(|| copy.footprint.clone()).pads[i]
                            .unused_layers = mode;
                        counts.pads += 1;
                    }
                }
                if let Some(footprint) = edited {
                    footprints.push((instance.uuid, footprint));
                }
            }
        }
        if let Some(mode) = p.vias {
            for via in pcb
                .vias()
                .filter(|via| via.unused_layers != mode && includes(Target::Object(via.uuid)))
            {
                let mut via = via.clone();
                via.unused_layers = mode;
                replacements.push(Object::Via(InDocument::new(p.document, via)));
                counts.vias += 1;
            }
        }
        let owners = replacements
            .iter()
            .map(Object::uuid)
            .chain(footprints.iter().map(|(id, _)| *id));
        reda_pcb::lock::refuse(pcb, owners, p.include_locked, "change unused copper on")?;
        for object in replacements {
            cx.tx()?.replace(object)?;
        }
        for (instance, footprint) in footprints {
            place_edited(cx, instance, footprint)?;
        }
        Ok(counts)
    }
}
