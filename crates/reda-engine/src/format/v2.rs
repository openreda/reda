//! The single format 1 to 2 migration. The chain stamps the new version.
use std::collections::BTreeSet;

use super::FileKind;
use reda_model::{Uuid, ViaType, geom::Nm};
use serde_json::{Value, json};

pub(super) fn migrate(kind: FileKind, mut file: Value) -> Result<Value, String> {
    validate_released(kind, &file)?;
    match kind {
        FileKind::Pcb => board(&mut file)?,
        FileKind::FabPreset => preset(&mut file)?,
        FileKind::Rules => {
            let rules = file["rules"].as_array_mut().ok_or("missing rules")?;
            for rule in rules
                .iter_mut()
                .filter(|r| r["kind"] == "via_size" && r["net_class"].is_null())
            {
                take_preferred(rule, "diameter")?;
                take_preferred(rule, "hole")?;
            }
        }
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

/// Check the file as it was declared before introducing defaults or moving fields.
/// Each schema is the released artifact. Cache it across all Project Documents.
fn validate_released(kind: FileKind, file: &Value) -> Result<(), String> {
    use std::sync::LazyLock;
    macro_rules! released {
        ($ext:literal) => {{
            static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
                serde_json::from_str(include_str!(concat!(
                    "../../../../docs/schemas/v1/",
                    $ext,
                    ".schema.json"
                )))
                .expect("released schema is JSON")
            });
            &*SCHEMA
        }};
    }
    let schema = match kind {
        FileKind::Project => released!("reda"),
        FileKind::Pcb => released!("redapcb"),
        FileKind::Sheet => released!("redasch"),
        FileKind::Library => released!("redalib"),
        FileKind::Symbol => released!("redasym"),
        FileKind::Footprint => released!("redafp"),
        FileKind::Part => released!("redapart"),
        FileKind::FabPreset => released!("redafab"),
        FileKind::Rules => released!("redarules"),
        FileKind::BoardTemplate => {
            return Err("Board Template files did not exist in format 1".into());
        }
    };
    reda_command::check_schema(schema, file).map_err(|e| format!("{}: {}", e.pointer, e.message))
}

fn preset(preset: &mut Value) -> Result<(), String> {
    let object = preset
        .as_object_mut()
        .ok_or("expected a Fab Preset object")?;
    object.insert("placement_corrections".into(), json!([]));
    let seeds = object
        .remove("stackup")
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    object.insert(
        "constructions".into(),
        json!([reda_pcb::construction::two_layer(seeds)]),
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

/// A hand-assigned UUID in the released file can coincide with the usual derived ID.
fn existing_ids(value: &Value, used: &mut BTreeSet<Uuid>) {
    match value {
        Value::Object(map) => {
            if let Some(id) = map
                .get("uuid")
                .and_then(Value::as_str)
                .and_then(|s| s.parse().ok())
            {
                used.insert(id);
            }
            for value in map.values() {
                existing_ids(value, used);
            }
        }
        Value::Array(rows) => {
            for value in rows {
                existing_ids(value, used);
            }
        }
        _ => {}
    }
}

fn reserve(used: &mut BTreeSet<Uuid>, mut id: Uuid) -> Uuid {
    while !used.insert(id) {
        id = id.child("next");
    }
    id
}

fn board(file: &mut Value) -> Result<(), String> {
    let mut used = BTreeSet::new();
    existing_ids(file, &mut used);
    let document: Uuid =
        serde_json::from_value(file["pcb"]["uuid"].clone()).map_err(|e| e.to_string())?;
    let rules = file["rules"].as_array_mut().ok_or("missing rules")?;
    let via_size = rules
        .iter_mut()
        .find(|r| {
            let scope = &r["scope"];
            r["kind"] == "via_size" && (scope == &json!({}) || scope == &json!({"net_class": null}))
        })
        .ok_or("missing board-wide via_size record")?;
    let diameter = take_preferred(via_size, "diameter")?;
    let hole = take_preferred(via_size, "hole")?;
    for rule in reda_pcb::multilayer_rules(|kind| reserve(&mut used, document.child(kind.id()))) {
        rules.push(json!(rule));
    }
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
        let through = ViaType::through(
            reserve(&mut used, id.child("through")),
            from,
            to,
            hole,
            diameter,
        );
        stack["via_types"] = json!([through]);
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
    rule.get_mut("values")
        .and_then(|v| v.get_mut(dimension))
        .and_then(Value::as_object_mut)
        .and_then(|bounds| bounds.remove("preferred"))
        .and_then(|n| n.as_i64())
        .map(Nm)
        .ok_or_else(|| format!("missing via_size {dimension}.preferred"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::{
        library::*, read_fab_file, read_rules_file, write_fab_file, write_rules_file,
    };
    use reda_testkit::{assert_golden, read_tree};

    /// Run both released standalone files and the Library local to the Minimal board through
    /// the same readers and canonical writers as their corresponding Commands.
    #[test]
    fn released_standalone_files_save_as_the_goldens_and_reopen_identically() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/formats");
        for dir in ["standalone", "minimal_board/lib"] {
            for (file, before) in read_tree(&root.join("v1").join(dir)) {
                let path = Path::new(&file);
                let encode = |bytes: &[u8]| {
                    match path.extension().unwrap().to_str().unwrap() {
                        "redalib" => write_library_info(&read_library_info(path, bytes).unwrap()),
                        "redasym" => write_symbol(&read_symbol(path, bytes).unwrap()),
                        "redafp" => write_footprint(&read_footprint(path, bytes).unwrap()),
                        "redapart" => write_part(&read_part(path, bytes).unwrap()),
                        "redafab" => write_fab_file(&read_fab_file(path, bytes).unwrap()),
                        "redarules" => write_rules_file(&read_rules_file(path, bytes).unwrap()),
                        ext => panic!("unexpected extension {ext}"),
                    }
                    .unwrap()
                };
                let after = encode(&before);
                assert_golden(root.join("migrated-v2").join(dir).join(path), &after);
                assert_eq!(encode(&after), after, "{file}");
            }
        }
    }
    #[test]
    fn explicit_null_net_class_in_a_legacy_rules_file_imports_as_board_wide() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/formats");
        let path = root.join("v1/standalone/Blinky.redarules");
        let mut released: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let rule = released["rules"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["kind"] == "via_size")
            .unwrap();
        rule["net_class"] = serde_json::Value::Null;
        let imported = read_rules_file(&path, &serde_json::to_vec(&released).unwrap()).unwrap();
        let expected = std::fs::read(root.join("migrated-v2/standalone/Blinky.redarules")).unwrap();
        assert_eq!(write_rules_file(&imported).unwrap(), expected);
    }
}
