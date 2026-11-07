//! The per-user folders, as plain paths. Finding them on the platform is the engine's job
//! (`reda_engine::user_dirs`); this crate only carries them in the [`crate::Session`].

use std::path::{Path, PathBuf};

/// Where REDA keeps one user's files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDirs {
    /// Hand-edited files: `preferences.toml`, `keymap.toml`, Saved Layouts, Fab Presets.
    pub config: PathBuf,
    /// Machine-written files: `state.json`, Recovery Snapshots, Crash Reports.
    pub data: PathBuf,
    /// Per-session files for the bridge: `instances/<pid>.json`. Owner-only.
    pub runtime: PathBuf,
}

impl UserDirs {
    /// Everything under one folder, as `REDA_HOME` asks: `config/`, `data/`, `runtime/`.
    pub fn under(home: &Path) -> UserDirs {
        UserDirs {
            config: home.join("config"),
            data: home.join("data"),
            runtime: home.join("runtime"),
        }
    }

    pub fn preferences_file(&self) -> PathBuf {
        self.config.join("preferences.toml")
    }

    pub fn state_file(&self) -> PathBuf {
        self.data.join("state.json")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.runtime.join("instances")
    }
}

/// Supplies the name recorded on new Waivers. The engine resolves Preferences; domain
/// crates can also be registered alone, in which case they use the OS user name.
pub type UserName = fn(Option<&UserDirs>) -> String;

pub fn os_user_name(_dirs: Option<&UserDirs>) -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}
