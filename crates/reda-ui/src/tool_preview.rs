//! A Tool's in-flight preview: what the user is part-way through, and the objects it was
//! drawn from. An agent's bridged edit never waits for the user; when it changes one of
//! those objects the preview is stale, so the Shell cancels it and leaves a status-bar
//! note (architecture.md#cli-and-control-port).
//!
//! Tools (#104 on) call [`Shell::begin_tool`] when their preview starts and
//! [`Shell::end_tool`] when it commits or is abandoned, and check [`Shell::tool_preview`] each
//! frame: `None` while they think they are active means a bridged edit cancelled them.
//!
//! [`Shell::begin_tool`]: crate::shell::Shell::begin_tool
//! [`Shell::end_tool`]: crate::shell::Shell::end_tool
//! [`Shell::tool_preview`]: crate::shell::Shell::tool_preview

use std::collections::BTreeSet;

use reda_engine::model::Uuid;
use serde_json::Value;

/// The preview of the Tool in use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolPreview {
    /// The Tool's title, for the status-bar note ("Move").
    pub title: String,
    /// The objects the preview depends on.
    pub objects: BTreeSet<Uuid>,
}

impl ToolPreview {
    pub fn new(title: impl Into<String>, objects: impl IntoIterator<Item = Uuid>) -> ToolPreview {
        ToolPreview {
            title: title.into(),
            objects: objects.into_iter().collect(),
        }
    }

    /// Whether the edit result `out` (the `{changes: {<scope>: {created, modified,
    /// deleted}}}` envelope) modified or deleted any object the preview depends on.
    pub fn touched_by(&self, out: &Value) -> bool {
        let Some(changes) = out["changes"].as_object() else {
            return false;
        };
        changes
            .values()
            .flat_map(|scope| ["modified", "deleted"].map(|k| &scope[k]))
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(|uuid| uuid.as_str()?.parse::<Uuid>().ok())
            .any(|uuid| self.objects.contains(&uuid))
    }
}
