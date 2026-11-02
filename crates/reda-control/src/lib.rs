//! The control port: loopback JSON-RPC 2.0 with token auth (architecture.md#cli-and-control-port).
//!
//! A host (the app, or [`headless::HeadlessHost`] in tests) starts a [`ControlServer`],
//! which listens on `127.0.0.1` with an OS-assigned port and writes `instances/<pid>.json`
//! (port, a random 256-bit token, the open Project file) to the owner-only runtime folder.
//! Each connection's first message must present the token; after that the client sends
//! `commands.list`, `commands.describe` and `commands.run`, one JSON-RPC message per line.
//! Runs reach the host as [`Call`]s, which it runs on its single writer and answers.
//!
//! The client side ([`client`]) reads the instance files and connects to the instance that
//! holds a Project, so the CLI and MCP server bridge instead of opening it a second time
//! (ADR-0007). Bridged sessions never save: the server refuses `project.save`, and the
//! Commands that would replace the host's Project.

pub mod client;
pub mod headless;
mod os_trash;
pub mod protocol;
mod server;

pub use os_trash::OsTrash;

use std::path::{Path, PathBuf};

use reda_engine::Engine;
use reda_engine::store;

pub use server::{Admission, Agent, Call, ControlServer};

/// The open Project's file (`<folder>/<name>.reda`), [`normalize`]d, as instance files
/// record it.
pub fn project_file(engine: &Engine) -> Option<PathBuf> {
    let folder = engine.location()?;
    let name = &engine.project()?.props().name;
    Some(normalize(&store::project_file_path(folder, name)))
}

/// The form both ends compare Project paths in: canonical when the file exists (a new,
/// unsaved Project's does not), else absolute.
pub fn normalize(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}
