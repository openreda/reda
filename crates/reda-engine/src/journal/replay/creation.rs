//! Board creation results name UUIDs by physical row or rule kind. Pair those
//! identities before the generic shape comparison visits a UUID-sorted change set.
use super::{ScriptEntry, Uuid, UuidMap, Value, is_uuid};

pub(super) fn prepare(step: &ScriptEntry, old: &Value, new: &mut Value, map: &mut UuidMap) {
    if step.id == "project.new" {
        payload(&step.id, old, new, &Value::Null, map);
        return;
    }
    let Some(result) = new.get_mut("result") else {
        return;
    };
    let changes = &old["changes"];
    if step.id == "batch" {
        if let (Some(commands), Some(recorded), Some(replayed)) = (
            step.params["commands"].as_array(),
            old["result"].as_array(),
            result.as_array_mut(),
        ) {
            for ((command, a), b) in commands.iter().zip(recorded).zip(replayed) {
                if let Some(id) = command["id"].as_str() {
                    payload(id, a, b, changes, map);
                }
            }
        }
    } else {
        payload(&step.id, &old["result"], result, changes, map);
    }
}

fn payload(id: &str, old: &Value, new: &mut Value, changes: &Value, map: &mut UuidMap) {
    let Some(fields) = new.as_object_mut() else {
        return;
    };
    match id {
        "document.add" | "project.new" => {
            if let Some(known) = old.get("seeded") {
                if let Some(actual) = fields.get("seeded") {
                    pair_rows(known, actual, map);
                }
            } else {
                // Recordings from before seed identities were reported have exactly
                // the original result shape. All other keys must still agree.
                fields.remove("seeded");
            }
        }
        "pcb.stackup.apply_construction" => pair_rows(old, new, map),
        "pcb.rules.apply_fab_preset" if old.get("created_rules").is_none() => {
            if let Some(Value::Object(rules)) = fields.remove("created_rules")
                && let Some(changes) = changes.as_object()
            {
                // Older versions derived each inner rule UUID from its PCB Document
                // and kind. Check the recorded creations before using that identity.
                for (document, change) in changes {
                    let Ok(document) = document.parse::<Uuid>() else {
                        continue;
                    };
                    let Some(created) = change["created"].as_array() else {
                        continue;
                    };
                    for (kind, new) in &rules {
                        let old = document.child(&format!("inner.{kind}")).to_string();
                        if created.iter().any(|v| v.as_str() == Some(&old)) {
                            pair_uuid(&Value::String(old), new, map);
                        }
                    }
                }
            }
        }
        "pcb.rules.apply_fab_preset" => {
            if let Some(actual) = fields.get("created_rules").and_then(Value::as_object) {
                for (kind, uuid) in actual {
                    pair_uuid(&old["created_rules"][kind], uuid, map);
                }
            }
        }
        _ => {}
    }
}

fn pair_rows(old: &Value, new: &Value, map: &mut UuidMap) {
    for key in ["layers", "dielectrics", "via_types", "rules"] {
        if let (Some(a), Some(b)) = (old[key].as_array(), new[key].as_array()) {
            for (recorded, actual) in a.iter().zip(b) {
                pair_uuid(recorded, actual, map);
            }
        }
    }
}

fn pair_uuid(old: &Value, new: &Value, map: &mut UuidMap) {
    if let (Some(a), Some(b)) = (old.as_str(), new.as_str())
        && is_uuid(a)
        && is_uuid(b)
    {
        map.insert(a.to_owned(), b.to_owned());
    }
}
