//! Shelving: `pcb.zone.shelve` and `pcb.zone.restore`. A shelved Zone stays in the file
//! but is hidden from the canvas and skipped by fill, DRC, connectivity and outputs.

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{InDocument, Object, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::{needs_pcb, pcb};
use super::existing_zone;

pub struct ZoneShelve;
pub struct ZoneRestore;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShelveParams {
    /// The Zones' UUIDs; left out, every Zone.
    #[serde(default)]
    pub zones: Option<Vec<Uuid>>,
}

/// What shelving or restoring returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Shelved {
    /// The Zones whose state changed, sorted by UUID; those already in it are left out.
    pub changed: Vec<Uuid>,
}

impl Command for ZoneShelve {
    const META: Meta = Meta {
        id: "pcb.zone.shelve",
        title: "Shelve Zones",
        description: "Shelves Zones (every Zone if `zones` is left out): each stays in the \
                      file but is hidden from the canvas and skipped by fill, DRC, \
                      connectivity and outputs, and DRC's `shelved_zone` Warning flags it. \
                      `pcb.zone.restore` brings it back.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = ShelveParams;
    type Output = Shelved;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ShelveParams) -> Result<Shelved, CommandError> {
        set_shelved(cx, p.zones, true)
    }
}

impl Command for ZoneRestore {
    const META: Meta = Meta {
        id: "pcb.zone.restore",
        title: "Restore Zones",
        description: "Restores shelved Zones (every Zone if `zones` is left out): they show \
                      on the canvas again and take part in fill, DRC, connectivity and \
                      outputs; they refill like any stale Zone.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = ShelveParams;
    type Output = Shelved;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ShelveParams) -> Result<Shelved, CommandError> {
        set_shelved(cx, p.zones, false)
    }
}

fn set_shelved(
    cx: &mut Context<'_>,
    zones: Option<Vec<Uuid>>,
    shelved: bool,
) -> Result<Shelved, CommandError> {
    let pcb = pcb(cx)?;
    let document = pcb.uuid;
    let chosen: Vec<_> = match zones {
        Some(zones) => zones
            .iter()
            .enumerate()
            .map(|(i, z)| existing_zone(pcb, *z).map_err(|e| at(e, i)))
            .collect::<Result<_, _>>()?,
        None => pcb.zones().collect(),
    };
    let mut changed: Vec<_> = chosen
        .into_iter()
        .filter(|z| z.shelved != shelved)
        .cloned()
        .collect();
    changed.sort_by_key(|z| z.uuid);
    changed.dedup_by_key(|z| z.uuid);
    let uuids = changed.iter().map(|z| z.uuid).collect();
    if !changed.is_empty() {
        let tx = cx.tx()?;
        for mut zone in changed {
            zone.shelved = shelved;
            tx.replace(Object::Zone(InDocument::new(document, zone)))?;
        }
    }
    Ok(Shelved { changed: uuids })
}

/// The "no Zone" error pointed at item `i` of `zones`.
pub(super) fn at(mut e: CommandError, i: usize) -> CommandError {
    e.details = Some(serde_json::json!({ "pointer": format!("/zones/{i}") }));
    e
}
