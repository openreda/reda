//! Resolving the objects an editing Command acts on, and tidying the part table after.

use std::collections::BTreeSet;

use reda_command::{CommandError, Context};
use reda_model::{Object, Project, Scope, Uuid};

/// Resolves a selection by UUID to the objects themselves, in the given order without
/// repeats. Only objects drawn on a Schematic Sheet can be selected: Units, Wires, Net
/// Labels, No-Connect Markers, text and graphics. `pointer` names the param.
pub(crate) fn resolve(
    project: &Project,
    uuids: &[Uuid],
    pointer: &str,
) -> Result<Vec<Object>, CommandError> {
    if uuids.is_empty() {
        return Err(CommandError::invalid_params(
            pointer,
            "select at least one object",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(uuids.len());
    for (i, &uuid) in uuids.iter().enumerate() {
        if !seen.insert(uuid) {
            continue;
        }
        let Some(object) = project.object(uuid) else {
            return Err(CommandError::not_found(format!(
                "no object has UUID {uuid}"
            )));
        };
        if !selectable(&object) {
            return Err(CommandError::invalid_params(
                &format!("{pointer}/{i}"),
                format!(
                    "{uuid} ({:?}) is not an object drawn on a sheet",
                    object.object_type()
                ),
            ));
        }
        out.push(object);
    }
    Ok(out)
}

/// Whether `object` is drawn on a Schematic Sheet and can be selected there.
pub(crate) fn selectable(object: &Object) -> bool {
    matches!(
        object,
        Object::Unit(_)
            | Object::Wire(_)
            | Object::NetLabel(_)
            | Object::NoConnect(_)
            | Object::SheetText(_)
            | Object::SheetGraphic(_)
    )
}

/// The sheet a selectable object is on.
pub(crate) fn sheet_of(object: &Object) -> Uuid {
    match object.scope() {
        Scope::Document(sheet) => sheet,
        _ => unreachable!("selectable objects live on a sheet"),
    }
}

/// The one sheet every object of a selection is on, or `invalid_params` at `pointer`.
pub(crate) fn one_sheet(objects: &[Object], pointer: &str) -> Result<Uuid, CommandError> {
    let sheet = sheet_of(&objects[0]);
    match objects.iter().all(|o| sheet_of(o) == sheet) {
        true => Ok(sheet),
        false => Err(CommandError::invalid_params(
            pointer,
            "the objects must all be on one sheet",
        )),
    }
}

/// Removes the part-table entries on `sheet` that no Unit uses any more, then the Footprint
/// copies no remaining entry lists. Returns what it removed.
pub(crate) fn prune_part_table(
    cx: &mut Context<'_>,
    sheet: Uuid,
) -> Result<Vec<Uuid>, CommandError> {
    let body = crate::sheet(cx.project()?, sheet)?;
    let used: BTreeSet<Uuid> = body.units().map(|u| u.part).collect();
    let entries: Vec<Uuid> = body
        .parts()
        .filter(|e| !used.contains(&e.uuid))
        .map(|e| e.uuid)
        .collect();
    let listed: BTreeSet<Uuid> = body
        .parts()
        .filter(|e| used.contains(&e.uuid))
        .flat_map(|e| e.footprints.iter().copied())
        .collect();
    let copies: Vec<Uuid> = body
        .footprints()
        .filter(|c| !listed.contains(&c.uuid))
        .map(|c| c.uuid)
        .collect();
    let removed: Vec<Uuid> = entries.into_iter().chain(copies).collect();
    for &uuid in &removed {
        cx.tx()?.remove(uuid)?;
    }
    Ok(removed)
}
