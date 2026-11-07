//! The compare of a [Locally Edited Footprint](crate::EditedFrom) with the copy it came
//! from: each Pad or graphic property whose edited value differs from the original. Sync's
//! Change List and the Update from Library preview show it as the local edits a
//! Footprint change removes.

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::{Footprint, Uuid};

/// One property of one Pad or graphic that differs from the copy it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LocalEdit {
    /// The Pad, for a Pad edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad: Option<Uuid>,
    /// The Pad's number, for a Pad edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// The graphic, for a graphic edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphic: Option<Uuid>,
    /// The property, as the file names it; `graphic` for a graphic added or deleted whole.
    pub property: String,
    /// The original value; left out when it wasn't set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original: Option<Value>,
    /// The edited value; left out when it isn't set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited: Option<Value>,
}

/// The local edits of `edited` against `original`: Pads by UUID, then graphics by UUID,
/// each property in name order.
pub fn local_edits(original: &Footprint, edited: &Footprint) -> Vec<LocalEdit> {
    let mut out = Vec::new();
    for pad in &edited.pads {
        let Some(before) = original.pads.iter().find(|p| p.uuid == pad.uuid) else {
            continue;
        };
        for (property, original, edited) in properties(before, pad) {
            out.push(LocalEdit {
                pad: Some(pad.uuid),
                number: Some(pad.number.clone()),
                graphic: None,
                property,
                original,
                edited,
            });
        }
    }
    let mut graphics: Vec<Uuid> = original
        .graphics
        .iter()
        .chain(&edited.graphics)
        .map(|g| g.uuid)
        .collect();
    graphics.sort();
    graphics.dedup();
    for uuid in graphics {
        let before = original.graphics.iter().find(|g| g.uuid == uuid);
        let after = edited.graphics.iter().find(|g| g.uuid == uuid);
        let edit = |property: String, original, edited| LocalEdit {
            pad: None,
            number: None,
            graphic: Some(uuid),
            property,
            original,
            edited,
        };
        match (before, after) {
            (Some(b), Some(a)) => {
                out.extend(properties(b, a).into_iter().map(|(p, o, e)| edit(p, o, e)))
            }
            (b, a) => out.push(edit("graphic".into(), b.map(json), a.map(json))),
        }
    }
    out
}

fn json<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("a Footprint row serialises")
}

fn object<T: Serialize>(value: &T) -> Map<String, Value> {
    match json(value) {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Each property whose value differs, with the value before and after.
fn properties<T: Serialize>(before: &T, after: &T) -> Vec<(String, Option<Value>, Option<Value>)> {
    let (before, after) = (object(before), object(after));
    let mut keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|k| before.get(*k) != after.get(*k))
        .map(|k| (k.clone(), before.get(k).cloned(), after.get(k).cloned()))
        .collect()
}
