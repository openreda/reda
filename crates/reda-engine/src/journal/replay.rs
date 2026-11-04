//! Replaying a script: run each step, check its outcome against the recording, and map
//! each UUID the recording created to the one the replay created.
//!
//! Results are compared by shape, not value: the same JSON types, the same object keys,
//! the same array lengths, and a UUID where the recording has a UUID. Wherever the
//! recording has a UUID string the replay's UUID at the same place is paired with it, and
//! later params have every paired recorded UUID replaced. UUIDs that are object keys or
//! elements of an all-UUID array (the edit envelope's `changes`, which is sorted by UUID)
//! are paired by what is already known first, so a second pass over each result pairs
//! the rest once the positional UUIDs elsewhere in it are known.

use std::collections::BTreeMap;

use reda_command::{CommandError, ErrorCode};
use reda_model::Uuid;
use serde_json::{Map, Value};

use super::{ReplayError, ScriptEntry};
use crate::Engine;

/// Recorded UUID → replayed UUID.
type UuidMap = BTreeMap<String, String>;

/// Runs every step of `script` on `engine`, stopping at the first step whose outcome
/// differs from the recording: a failure where it succeeded, success or another error
/// code where it failed, or a result of a different shape. Each step lands in the
/// engine's Journal as usual.
pub fn replay(engine: &mut Engine, script: &[ScriptEntry]) -> Result<(), ReplayError> {
    replay_with(|id, params| engine.run(id, params), script)
}

/// [`replay`] through any runner of Commands, such as a connection to a running app.
pub fn replay_with(
    mut run: impl FnMut(&str, Value) -> Result<Value, CommandError>,
    script: &[ScriptEntry],
) -> Result<(), ReplayError> {
    let mut map = UuidMap::new();
    for step in script {
        let fail = |pointer: Option<String>, message: String| ReplayError {
            line: step.line,
            id: Some(step.id.clone()),
            pointer,
            message,
        };
        let params = substitute(&step.params, &map);
        let outcome = run(&step.id, params);
        match (&step.outcome, outcome) {
            (None, Ok(_)) => {}
            (Some(Ok(recorded)), Ok(replayed)) => {
                for last_pass in [false, true] {
                    compare(recorded, &replayed, "", &mut map, last_pass)
                        .map_err(|(pointer, message)| fail(Some(pointer), message))?;
                }
            }
            (Some(Err(recorded)), Err(replayed)) if recorded.code == replayed.code => {}
            (Some(Err(recorded)), Err(replayed)) => {
                return Err(fail(
                    None,
                    format!(
                        "failed with {} where the recording failed with {}: {}",
                        code(replayed.code),
                        code(recorded.code),
                        replayed.message
                    ),
                ));
            }
            (Some(Err(recorded)), Ok(_)) => {
                return Err(fail(
                    None,
                    format!(
                        "succeeded where the recording failed with {}",
                        code(recorded.code)
                    ),
                ));
            }
            (None | Some(Ok(_)), Err(err)) => {
                return Err(fail(
                    None,
                    format!("failed: {}: {}", code(err.code), err.message),
                ));
            }
        }
    }
    Ok(())
}

/// The code as it is written in JSON, such as `not_found`.
fn code(code: ErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{code:?}"))
}

/// `params` with every string and object key that is a paired recorded UUID replaced.
fn substitute(params: &Value, map: &UuidMap) -> Value {
    match params {
        Value::String(s) => Value::String(map.get(s).cloned().unwrap_or_else(|| s.clone())),
        Value::Array(items) => Value::Array(items.iter().map(|v| substitute(v, map)).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| {
                    let key = map.get(k).cloned().unwrap_or_else(|| k.clone());
                    (key, substitute(v, map))
                })
                .collect::<Map<_, _>>(),
        ),
        other => other.clone(),
    }
}

fn is_uuid(s: &str) -> bool {
    s.parse::<Uuid>().is_ok()
}

/// A hash over content that holds UUIDs, such as a Sync Change List's `hash` and its
/// change `id`s or a check's `design` fingerprint: 16 or more lowercase hex digits. A
/// replay with other UUIDs gets other hashes, so they are paired like UUIDs.
fn is_content_hash(s: &str) -> bool {
    s.len() >= 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(s) if is_uuid(s) => "a UUID",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

type Mismatch = (String, String);

fn mismatch(pointer: &str, message: String) -> Result<(), Mismatch> {
    Err((
        pointer.to_owned(),
        format!("result shape differs: {message}"),
    ))
}

/// Checks that `replayed` has the shape of `recorded` and pairs their UUIDs into `map`.
/// UUID keys and all-UUID arrays whose pairing is still ambiguous are left for the last
/// pass, which pairs any remainder in order.
fn compare(
    recorded: &Value,
    replayed: &Value,
    pointer: &str,
    map: &mut UuidMap,
    last_pass: bool,
) -> Result<(), Mismatch> {
    match (recorded, replayed) {
        (Value::Null, Value::Null)
        | (Value::Bool(_), Value::Bool(_))
        | (Value::Number(_), Value::Number(_)) => Ok(()),
        (Value::String(a), Value::String(b)) => match (is_uuid(a), is_uuid(b)) {
            (true, true) => {
                map.entry(a.clone()).or_insert_with(|| b.clone());
                Ok(())
            }
            (false, false) => {
                if is_content_hash(a) && is_content_hash(b) {
                    map.entry(a.clone()).or_insert_with(|| b.clone());
                }
                Ok(())
            }
            _ => mismatch(
                pointer,
                format!("recorded {}, replayed {}", kind(recorded), kind(replayed)),
            ),
        },
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return mismatch(
                    pointer,
                    format!("recorded {} items, replayed {}", a.len(), b.len()),
                );
            }
            let uuids = |items: &[Value]| -> Option<Vec<String>> {
                items
                    .iter()
                    .map(|v| v.as_str().filter(|s| is_uuid(s)).map(str::to_owned))
                    .collect()
            };
            if let (Some(ua), Some(ub)) = (uuids(a), uuids(b))
                && !a.is_empty()
            {
                for (x, y) in pair(&ua, &ub, map, last_pass) {
                    map.entry(x).or_insert(y);
                }
                return Ok(());
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                compare(x, y, &format!("{pointer}/{i}"), map, last_pass)?;
            }
            Ok(())
        }
        (Value::Object(a), Value::Object(b)) => {
            let (plain_a, uuid_a): (Vec<&String>, Vec<&String>) =
                a.keys().partition(|k| !is_uuid(k));
            let (plain_b, uuid_b): (Vec<&String>, Vec<&String>) =
                b.keys().partition(|k| !is_uuid(k));
            for key in &plain_a {
                let here = format!("{pointer}/{}", escape(key));
                match b.get(*key) {
                    Some(value) => compare(&a[*key], value, &here, map, last_pass)?,
                    None => return mismatch(&here, "the replayed result lacks this key".into()),
                }
            }
            if let Some(key) = plain_b.iter().find(|k| !a.contains_key(**k)) {
                return mismatch(
                    &format!("{pointer}/{}", escape(key)),
                    "the recorded result lacks this key".into(),
                );
            }
            if uuid_a.len() != uuid_b.len() {
                return mismatch(
                    pointer,
                    format!(
                        "recorded {} UUID keys, replayed {}",
                        uuid_a.len(),
                        uuid_b.len()
                    ),
                );
            }
            let ua: Vec<String> = uuid_a.into_iter().cloned().collect();
            let ub: Vec<String> = uuid_b.into_iter().cloned().collect();
            for (x, y) in pair(&ua, &ub, map, last_pass) {
                map.entry(x.clone()).or_insert_with(|| y.clone());
                compare(
                    &a[&x],
                    &b[&y],
                    &format!("{pointer}/{}", escape(&x)),
                    map,
                    last_pass,
                )?;
            }
            Ok(())
        }
        _ => mismatch(
            pointer,
            format!("recorded {}, replayed {}", kind(recorded), kind(replayed)),
        ),
    }
}

/// Pairs two equally long UUID sets: by `map` first, then a UUID present in both as
/// itself, then a single leftover on each side. Any further leftovers are paired in order
/// on the last pass and left out before it.
fn pair(
    recorded: &[String],
    replayed: &[String],
    map: &UuidMap,
    last_pass: bool,
) -> Vec<(String, String)> {
    let mut left: Vec<&String> = Vec::new();
    let mut free: Vec<&String> = replayed.iter().collect();
    let mut out = Vec::new();
    for x in recorded {
        let target = map.get(x).unwrap_or(x);
        if let Some(i) = free.iter().position(|y| *y == target) {
            out.push((x.clone(), free.remove(i).clone()));
        } else {
            left.push(x);
        }
    }
    if left.len() == 1 || last_pass {
        out.extend(left.into_iter().cloned().zip(free.into_iter().cloned()));
    }
    out
}
