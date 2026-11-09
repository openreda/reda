//! Silkscreen Preparation in the GUI (pcb.md#silkscreen-preparation): its modal options
//! dialog, which opens on every call from a menu or the palette with each option at its
//! default, and the Messages rows for text that found no clear place.

use reda_engine::model::{Severity, Uuid};
use serde_json::{Map, Value};

use crate::command_dialog::{Dialogs, ParamForm};
use crate::messages::{Note, mm};
use crate::shell::Shell;
use crate::tr;

/// The board's and the Footprint editor's Silkscreen Preparation.
pub const COMMANDS: [&str; 2] = [
    "pcb.silkscreen.prepare",
    "library.footprint.silkscreen.prepare",
];

/// The source of its Messages rows: each run replaces the last one's.
pub const SOURCE: &str = "silkscreen";

/// The params that take a length, shown in millimetres.
const LENGTHS: [&str; 3] = ["clearance", "min_remaining_length", "max_distance"];

/// Opens the options dialog on every GUI call of both Commands.
pub fn register_dialogs(dialogs: &mut Dialogs) {
    for id in COMMANDS {
        dialogs.register_always(id, move |shell, given| {
            let registry = shell.engine().registry();
            let params = with_defaults(registry.params_schema(id), given);
            Box::new(ParamForm::new(registry, id, &params))
        });
    }
}

/// `given` with every option it leaves out at the default its schema states.
pub fn with_defaults(schema: Option<&Value>, given: Value) -> Value {
    let mut params = match given {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let properties = schema.and_then(|s| s["properties"].as_object());
    for (name, property) in properties.into_iter().flatten() {
        let Some(default) = property.get("default") else {
            continue;
        };
        if params.contains_key(name) {
            continue;
        }
        let value = match default.as_i64() {
            Some(nm) if LENGTHS.contains(&name.as_str()) => Value::String(mm(nm)),
            _ => default.clone(),
        };
        params.insert(name.clone(), value);
    }
    Value::Object(params)
}

/// After a run of `id` with result `out`: one Messages row per text that stayed because
/// it found no clear place, on the board or the open Footprint.
pub fn after_run(shell: &mut Shell, id: &str, params: &Value, out: &Value) {
    if !COMMANDS.contains(&id) {
        return;
    }
    let document = match id {
        "pcb.silkscreen.prepare" => shell
            .engine()
            .project()
            .and_then(|p| p.pcb())
            .map(|p| p.uuid),
        _ => params["footprint"]
            .as_str()
            .and_then(|s| s.parse::<Uuid>().ok()),
    };
    let notes = out["result"]["unplaced"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| Note {
            source: SOURCE.to_owned(),
            document,
            severity: Severity::Warning,
            text: tr!(
                "message-silkscreen-unplaced",
                text = u["content"].as_str().unwrap_or_default()
            ),
        })
        .collect();
    shell.set_notes(SOURCE, notes);
}
