//! The wire format: method names, JSON-RPC error codes and the instance file.
//!
//! Every message is one line of JSON-RPC 2.0. The first request on a connection must be
//! `auth {token, source?}`; its result is `{project}`, the instance's open Project file or
//! `null`. Then:
//!
//! - `commands.list {}` → the registry catalogue, `{commands: [...]}`.
//! - `commands.describe {id}` → one catalogue entry (`<id>:<preset>` names its Command).
//! - `commands.run {id, params?}` → the Command's result.
//!
//! A failed Command is the error [`COMMAND_FAILED`] with the `{code, message, details?}`
//! Command error as `data`. Requests without an `id` (notifications) are ignored.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const AUTH: &str = "auth";
pub const LIST: &str = "commands.list";
pub const DESCRIBE: &str = "commands.describe";
pub const RUN: &str = "commands.run";

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
