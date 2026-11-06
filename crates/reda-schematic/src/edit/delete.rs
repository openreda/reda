//! `schematic.delete`: removing a selection from its sheets.

use std::collections::BTreeSet;

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::selection::{prune_part_table, resolve, sheet_of};
use crate::normalise::{Wiring, normalise};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteParams {
    /// The objects to delete, by UUID: Units, Wires, Net Labels, No-Connect Markers,
    /// Ports, Power Ports, text and graphics, on any sheets.
    pub objects: Vec<Uuid>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// What `schematic.delete` removed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteResult {
    /// The objects deleted, including the part-table entries and Footprint copies that no
    /// Unit uses any more.
    pub deleted: Vec<Uuid>,
}

pub struct Delete;

impl Command for Delete {
    const META: Meta = Meta {
        id: "schematic.delete",
        title: "Delete",
        description: "Deletes the selected objects from their sheets. Part-table entries no \
                      remaining Unit uses, and Footprint copies no remaining entry lists, \
                      go with them.",
        category: "schematic",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = DeleteParams;
    type Output = DeleteResult;

    fn run(&self, cx: &mut Context<'_>, p: DeleteParams) -> Result<DeleteResult, CommandError> {
        let objects = resolve(cx.project()?, &p.objects, "/objects")?;
        Ok(DeleteResult {
            deleted: delete_objects(cx, &objects, &p.wiring.unwrap_or_default())?,
        })
    }
}

/// Removes `objects`, prunes the part tables of the sheets they were on and normalises the
/// Wires left touching them. Returns every UUID removed, Wires merged away included.
pub(crate) fn delete_objects(
    cx: &mut Context<'_>,
    objects: &[reda_model::Object],
    wiring: &Wiring,
) -> Result<Vec<Uuid>, CommandError> {
    let mut deleted = Vec::with_capacity(objects.len());
    let sheets: BTreeSet<Uuid> = objects.iter().map(sheet_of).collect();
    let mut before = Vec::with_capacity(sheets.len());
    for &sheet in &sheets {
        before.push(crate::sheet(cx.project()?, sheet)?.clone());
    }
    for object in objects {
        cx.tx()?.remove(object.uuid())?;
        deleted.push(object.uuid());
    }
    for sheet in &before {
        deleted.extend(prune_part_table(cx, sheet.uuid)?);
        normalise(cx, sheet.uuid, sheet, wiring)?;
        let left = crate::sheet(cx.project()?, sheet.uuid)?;
        let merged: Vec<Uuid> = (sheet.wires())
            .map(|w| w.uuid)
            .filter(|u| left.wire(*u).is_none() && !deleted.contains(u))
            .collect();
        deleted.extend(merged);
    }
    Ok(deleted)
}
