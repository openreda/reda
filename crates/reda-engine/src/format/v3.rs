//! The single format 2 to 3 migration. The chain stamps the new version.
//!
//! A board's one Mechanical and one Documentation Layer become the Mechanical Layers
//! Dimensions and Reference right after the Board Outline, and the board gains an empty
//! board-wide `component_height` record. A Board Template's drawing groups name their
//! Layer by Purpose. Every other kind changes only its number. The step writes literal
//! JSON rather than calling today's defaults, so it stays what 0.3.0 shipped.
use std::collections::BTreeSet;

use super::FileKind;
use super::v2::{existing_ids, reserve};
use reda_model::Uuid;
use serde_json::{Value, json};

/// The chain has already checked `file` against the format-2 schemas.
pub(super) fn migrate(kind: FileKind, mut file: Value) -> Result<Value, String> {
    match kind {
        FileKind::Pcb => board(&mut file)?,
        FileKind::BoardTemplate => template(&mut file)?,
        _ => {}
    }
    Ok(file)
}

/// The Purpose and name an old Layer kind becomes.
fn renamed(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "mechanical" => Some(("dimensions", "Dimensions")),
        "documentation" => Some(("reference", "Reference")),
        _ => None,
    }
}

fn board(file: &mut Value) -> Result<(), String> {
    let mut used = BTreeSet::new();
    existing_ids(file, &mut used);
    let layers = file["layers"]["layers"]
        .as_array_mut()
        .ok_or("missing Layers")?;
    let mut kept = Vec::with_capacity(layers.len());
    let (mut dimensions, mut reference) = (Vec::new(), Vec::new());
    for layer in layers.drain(..) {
        match layer["kind"].as_str().and_then(renamed) {
            Some(("dimensions", _)) => dimensions.push(layer),
            Some(_) => reference.push(layer),
            None => kept.push(layer),
        }
    }
    let after_outline = kept
        .iter()
        .position(|l| l["kind"] == "board_outline")
        .map_or(kept.len(), |i| i + 1);
    let mut names = BTreeSet::new();
    let mechanical: Vec<Value> = dimensions
        .into_iter()
        .chain(reference)
        .map(|layer| {
            let (purpose, name) = renamed(layer["kind"].as_str().unwrap_or_default())
                .expect("only renamed kinds were set aside");
            let name = (1..)
                .map(|n| match n {
                    1 => name.to_owned(),
                    n => format!("{name} {n}"),
                })
                .find(|n| names.insert(n.clone()))
                .expect("some number is free");
            json!({"uuid": layer["uuid"], "kind": "mechanical", "name": name, "purpose": purpose})
        })
        .collect();
    kept.splice(after_outline..after_outline, mechanical);
    *layers = kept;

    let document: Uuid =
        serde_json::from_value(file["pcb"]["uuid"].clone()).map_err(|e| e.to_string())?;
    let uuid = reserve(&mut used, document.child("component_height"));
    file["rules"]
        .as_array_mut()
        .ok_or("missing rules")?
        .push(json!({"uuid": uuid, "kind": "component_height", "scope": {}, "values": {}}));
    Ok(())
}

fn template(file: &mut Value) -> Result<(), String> {
    let Some(groups) = file.get_mut("drawing_groups") else {
        return Ok(());
    };
    for group in groups
        .as_array_mut()
        .ok_or("drawing_groups must be an array")?
    {
        let group = group
            .as_object_mut()
            .ok_or("expected a drawing group object")?;
        let layer = group
            .remove("layer")
            .ok_or("a drawing group has no layer")?;
        let (purpose, _) = layer
            .as_str()
            .and_then(renamed)
            .ok_or_else(|| format!("unknown drawing group layer {layer}"))?;
        group.insert("purpose".into(), json!(purpose));
    }
    Ok(())
}
