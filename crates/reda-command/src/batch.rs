//! `batch {commands}`: several edit and query steps as one atomic edit.
//!
//! The registry runs it, because its steps are other Commands. This module holds its
//! metadata, its params and how a step's error is reported.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{CommandError, ErrorCode, Kind, Meta};

pub(crate) const META: Meta = Meta {
    id: "batch",
    title: "Batch",
    description: "Runs several edit and query Commands as one atomic edit: if any step \
                  fails, every step is rolled back. Later steps refer to objects created by \
                  earlier ones through UUIDs the caller supplied in the earlier step's \
                  params. Returns the combined changes and each step's result, in order.",
    category: "edit",
    icon: "batch",
    kind: Kind::Edit,
};

/// The params of `batch`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BatchParams {
    /// The steps, run in order.
    pub commands: Vec<BatchStep>,
}

/// One step of a batch.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BatchStep {
    /// An edit or query Command id, or one of their Presets as `<id>:<preset>`.
    pub id: String,
    /// The step's params, as the Command takes them on its own.
    #[serde(default)]
    pub params: Value,
}

/// Says which step failed: the message is prefixed, `details.step` is its index, and an
/// `invalid_params` pointer is rebased onto the batch's own params.
pub(crate) fn step_error(index: usize, id: &str, mut err: CommandError) -> CommandError {
    err.message = format!("step {index} ({id}): {}", err.message);
    let mut details = match err.details.take() {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    if err.code == ErrorCode::InvalidParams {
        let inner = details
            .get("pointer")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        details.insert(
            "pointer".into(),
            json!(format!("/commands/{index}/params{inner}")),
        );
    }
    details.insert("step".into(), json!(index));
    err.details = Some(Value::Object(details));
    err
}

/// File and view Commands, and `batch` itself, cannot be steps.
pub(crate) fn refused_step(id: &str, kind: Kind) -> CommandError {
    let what = match kind {
        Kind::File => "a file Command",
        Kind::View => "a view Command",
        Kind::Edit | Kind::Query => "a batch",
    };
    CommandError::rejected(format!(
        "only edit and query Commands can be batch steps, and {id} is {what}"
    ))
}
