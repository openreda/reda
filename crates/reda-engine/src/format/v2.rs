//! The single format 1 to 2 migration. The chain stamps the new version.
use super::FileKind;
use reda_model::{Uuid, ViaType, geom::Nm};
use serde_json::{Value, json};

pub(super) fn migrate(kind: FileKind, mut file: Value) -> Result<Value, String> {
    match kind {
        FileKind::Pcb => board(&mut file)?,
        FileKind::FabPreset => preset(&mut file)?,
        FileKind::Project => {
            if let Some(copies) = file.get_mut("fab_presets") {
                for copy in copies
                    .as_array_mut()
                    .ok_or("fab_presets must be an array")?
                {
                    preset(copy)?;
                }
            }
        }
        _ => {}
    }
    Ok(file)
}

fn preset(preset: &mut Value) -> Result<(), String> {
    let object = preset
        .as_object_mut()
        .ok_or("expected a Fab Preset object")?;
    let seeds = object
        .remove("stackup")
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    object.insert(
        "constructions".into(),
        json!([reda_model::Construction::two_layer(seeds)]),
    );
    let rules = object
        .get_mut("rules")
        .and_then(Value::as_object_mut)
        .ok_or("missing Fab Preset rules")?;
    rules.insert(
        "clearance_hole".into(),
        json!(reda_model::HoleClearance::GENERIC),
    );
    Ok(())
}

fn board(file: &mut Value) -> Result<(), String> {
    let document: Uuid =
        serde_json::from_value(file["pcb"]["uuid"].clone()).map_err(|e| e.to_string())?;
    let rules = file["rules"].as_array_mut().ok_or("missing rules")?;
    let via_size = rules
        .iter_mut()
        .find(|r| r["kind"] == "via_size" && r["scope"] == json!({}))
        .ok_or("missing board-wide via_size record")?;
    let diameter = take_preferred(via_size, "diameter")?;
    let hole = take_preferred(via_size, "hole")?;
    rules.extend(
        reda_pcb::multilayer_rules(document)
            .iter()
            .map(|r| json!(r)),
    );
    for stack in file["stackups"].as_array_mut().ok_or("missing stackups")? {
        let id: Uuid = serde_json::from_value(stack["uuid"].clone()).map_err(|e| e.to_string())?;
        let entries = stack["entries"]
            .as_array_mut()
            .ok_or("missing Stackup entries")?;
        let mut copper = Vec::new();
        for entry in entries {
            if let Some(layer) = entry.get("layer") {
                copper.push(serde_json::from_value(layer.clone()).map_err(|e| e.to_string())?);
            } else if let Some(dielectric) = entry.as_object_mut() {
                dielectric.insert("kind".into(), json!("core"));
            } else {
                return Err("expected a Stackup entry object".into());
            }
        }
        let (Some(&from), Some(&to)) = (copper.first(), copper.last()) else {
            return Err("the Stackup has no copper".into());
        };
        stack["via_types"] = json!([ViaType::through(id, from, to, hole, diameter)]);
    }
    if let Some(vias) = file.get_mut("vias") {
        for via in vias.as_array_mut().ok_or("vias must be an array")? {
            let start = via["diameter"].as_i64().ok_or("invalid Via diameter")?;
            via["diameter"] = json!({"start":start});
        }
    }
    Ok(())
}

fn take_preferred(rule: &mut Value, dimension: &str) -> Result<Nm, String> {
    rule["values"][dimension]
        .as_object_mut()
        .and_then(|bounds| bounds.remove("preferred"))
        .and_then(|n| n.as_i64())
        .map(Nm)
        .ok_or_else(|| format!("missing via_size {dimension}.preferred"))
}
