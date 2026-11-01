//! The session Journal: every Command the engine ran, in order, kept in memory.
//!
//! It exports as JSON lines ([`export`]), one entry per line:
//! `{"id", "source", "params", "result"}` or `{"id", "source", "params", "error"}`. The same
//! format is a script: [`parse`] reads one and [`replay`] runs it on an engine, remapping
//! the UUIDs each step creates. Data-driven scenario tests and `reda run --script` are
//! built on this. The on-disk crash-evidence mirror is not built yet.

mod replay;

use reda_command::CommandError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

pub use replay::replay;

/// Who asked for a Command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Gui,
    #[default]
    Cli,
    Mcp,
}

/// One Command run: its id, params and source, and the result or error.
#[derive(Clone, Debug, PartialEq)]
pub struct JournalEntry {
    pub id: String,
    pub params: Value,
    pub source: Source,
    pub outcome: Result<Value, CommandError>,
}

/// One step of a script: a Command to run and, when the script recorded one, the outcome
/// it must reproduce.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptEntry {
    /// The 1-based line of the script it came from.
    pub line: usize,
    pub id: String,
    pub params: Value,
    /// `None` when the line has neither `result` nor `error`: the step then only has to
    /// succeed. Hand-written scenario steps use this.
    pub outcome: Option<Result<Value, CommandError>>,
}

/// Why a script could not be read or replayed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReplayError {
    /// The 1-based script line.
    pub line: usize,
    /// The Command id of that line, once it parsed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// For a shape mismatch, the JSON pointer into the result where the recording and
    /// the replay part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer: Option<String>,
    pub message: String,
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}", self.line)?;
        if let Some(id) = &self.id {
            write!(f, " ({id})")?;
        }
        if let Some(pointer) = &self.pointer {
            write!(f, " at '{pointer}'")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for ReplayError {}

/// The on-disk shape of one line. Field order is the key order written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Line {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<Source>,
    #[serde(default = "empty_object")]
    params: Value,
    /// `Some(Value::Null)` for a recorded `"result": null`, `None` when absent.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<CommandError>,
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

/// Reads a present field as `Some`, even when its value is `null`.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

/// Points every `project.new` step of a script at a fresh folder under `dir` (`dir/1`,
/// `dir/2`, … in script order) and returns the paths it replaced, in the same order.
/// `project.new` writes the Project to disk, so a replay must not reuse the recorded
/// folder; tests and the scenario harness relocate to a temporary folder.
pub fn relocate_new_projects(steps: &mut [ScriptEntry], dir: &std::path::Path) -> Vec<Value> {
    let mut replaced = vec![];
    for step in steps.iter_mut().filter(|s| s.id == "project.new") {
        let Some(params) = step.params.as_object_mut() else {
            continue;
        };
        let path = dir.join((replaced.len() + 1).to_string());
        let old = params.insert("path".into(), Value::from(path.to_string_lossy().as_ref()));
        replaced.push(old.unwrap_or(Value::Null));
    }
    replaced
}

/// The entries as JSON lines, each ending in `\n`.
pub fn export(entries: &[JournalEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        let (result, error) = match &entry.outcome {
            Ok(value) => (Some(value.clone()), None),
            Err(err) => (None, Some(err.clone())),
        };
        let line = Line {
            id: entry.id.clone(),
            source: Some(entry.source),
            params: entry.params.clone(),
            result,
            error,
        };
        out.push_str(&serde_json::to_string(&line).expect("Journal lines serialise"));
        out.push('\n');
    }
    out
}

/// Reads a script in the Journal format. Blank lines are skipped; `source` is accepted and
/// ignored, and `params` defaults to `{}`.
pub fn parse(text: &str) -> Result<Vec<ScriptEntry>, ReplayError> {
    let mut out = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let parsed: Line = serde_json::from_str(raw).map_err(|e| ReplayError {
            line,
            id: None,
            pointer: None,
            message: format!("not a Journal entry: {e}"),
        })?;
        let outcome = match (parsed.result, parsed.error) {
            (Some(_), Some(_)) => {
                return Err(ReplayError {
                    line,
                    id: Some(parsed.id),
                    pointer: None,
                    message: "an entry has a result or an error, not both".into(),
                });
            }
            (Some(result), None) => Some(Ok(result)),
            (None, Some(error)) => Some(Err(error)),
            (None, None) => None,
        };
        out.push(ScriptEntry {
            line,
            id: parsed.id,
            params: parsed.params,
            outcome,
        });
    }
    Ok(out)
}
