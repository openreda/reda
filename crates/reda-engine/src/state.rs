//! `state.json`: per-user state REDA writes for itself, in the data folder.
//!
//! Several instances share the file, so every change is a read-merge-write through
//! [`update`]: re-read the file, change only what the caller changes, write it back through
//! a temp file and a rename. Fields this version does not know are kept.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reda_command::UserDirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How many recent Projects are kept.
pub const MAX_RECENT_PROJECTS: usize = 10;

/// The contents of `state.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserState {
    /// Recently opened Projects, newest first.
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    /// Agents the user allowed to connect through the bridge.
    #[serde(default)]
    pub allowed_agents: Vec<String>,
    /// Whether the vendor-terms notice of Import part has been shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub vendor_terms_notice_shown: bool,
    /// The main window's geometry when no Project is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowGeometry>,
    /// The Fab Preset the last new Project used: the New Project dialog's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fab_preset: Option<String>,
    /// Fields written by another version of REDA, kept as they are.
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

/// A window's position and size in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl UserState {
    /// Puts `path` first in the recent Projects, dropping an older entry for it and the
    /// oldest entries past [`MAX_RECENT_PROJECTS`].
    pub fn add_recent_project(&mut self, path: &Path) {
        self.recent_projects.retain(|p| p != path);
        self.recent_projects.insert(0, path.to_path_buf());
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }
}

/// Reads `state.json`. A missing or unreadable file reads as empty state: the file is
/// machine-written, so there is nothing for the user to fix.
pub fn read(dirs: &UserDirs) -> UserState {
    match std::fs::read(dirs.state_file()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => UserState::default(),
    }
}

/// Re-reads `state.json`, applies `change` and writes the result back. Returns the state
/// as written.
pub fn update(dirs: &UserDirs, change: impl FnOnce(&mut UserState)) -> std::io::Result<UserState> {
    let mut state = read(dirs);
    change(&mut state);
    let mut bytes = serde_json::to_vec_pretty(&state).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    std::fs::create_dir_all(&dirs.data)?;
    crate::fs::write_atomic(&dirs.state_file(), &bytes)?;
    Ok(state)
}
