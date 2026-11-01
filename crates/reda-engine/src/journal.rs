//! The session Journal: every Command the engine ran, in order, kept in memory.
//!
//! Export as JSON lines, replay with UUID remapping and the on-disk crash-evidence mirror
//! build on this later.

use reda_command::CommandError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
