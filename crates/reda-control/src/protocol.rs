//! The wire format: method names, JSON-RPC error codes and the instance file.
//!
//! Every message is one line of JSON-RPC 2.0. The first request on a connection must be
//! `auth {token, source?, agent?}`; its result is `{project}`, the instance's open Project
//! file or `null`. `agent` names the client for the app's "Allow this agent?" prompt; the
//! app may hold the answer until the user decides, and a refusal is [`DENIED`]. Then:
//!
//! - `commands.list {}` → the registry catalogue, `{commands: [...]}`.
//! - `commands.describe {id}` → one catalogue entry (`<id>:<preset>` names its Command).
//! - `commands.run {id, params?, progress?}` → the Command's result. With `progress: true`
//!   the server sends `progress {id, progress, total?, message}` notifications (`id` is
//!   the request's) before the response.
//! - the notification `cancel {id}` cancels that run: a queued run is not run, a running
//!   one stops at its next check and rolls back. Either answers `cancelled`.
//!
//! A failed Command is the error [`COMMAND_FAILED`] with the `{code, message, details?}`
//! Command error as `data`. Other notifications are ignored.

use std::path::PathBuf;

use reda_engine::journal::Source;
use serde::{Deserialize, Serialize};

pub const AUTH: &str = "auth";
pub const LIST: &str = "commands.list";
pub const DESCRIBE: &str = "commands.describe";
pub const RUN: &str = "commands.run";
/// Client → server notification: cancel the run with this request id.
pub const CANCEL: &str = "cancel";
/// Server → client notification: progress of a running Command.
pub const PROGRESS: &str = "progress";

/// The one Command another REDA app (`source: gui`) may run: hand over a file the OS asked
/// it to open, to the instance holding its Project.
pub const OPEN_FILE: &str = "app.file.open";

/// The line was not JSON.
pub const PARSE_ERROR: i64 = -32700;
/// The JSON was not a JSON-RPC 2.0 request.
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
/// The method's params have the wrong shape (a Command's own params errors are
/// [`COMMAND_FAILED`] with `invalid_params`).
pub const INVALID_PARAMS: i64 = -32602;
/// The Command failed; `data` is the Command error.
pub const COMMAND_FAILED: i64 = -32000;
/// The first message did not present the right token. The server closes the connection.
pub const UNAUTHORIZED: i64 = -32001;
/// The host did not allow this agent. The server closes the connection.
pub const DENIED: i64 = -32002;

/// The agent name of a client that gives none in `auth`.
pub fn default_agent_name(source: Source) -> &'static str {
    match source {
        Source::Gui => "REDA",
        Source::Cli => "reda CLI",
        Source::Mcp => "MCP client",
    }
}

/// `instances/<pid>.json`: how to reach one running instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstanceFile {
    pub pid: u32,
    pub port: u16,
    /// 256 random bits as 64 lowercase hex digits.
    pub token: String,
    /// The open Project file, normalized (see [`crate::normalize`]), or `None`.
    pub project: Option<PathBuf>,
}
