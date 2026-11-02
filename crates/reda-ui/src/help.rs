//! What the Help menu shows and does: the About box with the third-party notices, the
//! on-demand update check and, on macOS, putting the `reda` CLI on the PATH.

mod cli_install;
mod dialogs;
mod updates;

use reda_engine::Engine;
use reda_engine::command::Registry;

pub use cli_install::{BIN_DIR, CliInstalled, InstallError, install_cli};
pub use updates::{
    CheckStarted, LATEST_RELEASE_API, Release, ReleaseSource, USER_ONLY, UpdateCheck, Updates,
    configure, is_newer, parse_github_release, take_checks, updates, version,
};

pub(crate) use dialogs::Dialogs;

/// The third-party notices, built into the binary by the build script from
/// `packaging/notices/vendored.txt` and, in a release, cargo-about's crate licences.
pub const NOTICES: &str = include_str!(concat!(env!("OUT_DIR"), "/THIRD_PARTY_NOTICES"));

/// Adds the Help Commands to `registry`, with no release source: `app.update.check` stays
/// unavailable. For catalogues and tests; the Shell uses [`install`].
pub fn register_all(registry: &mut Registry) {
    register_with(registry, Updates::default());
}

fn register_with(registry: &mut Registry, updates: Updates) {
    registry.add(updates::CheckForUpdates(updates));
    registry.add(cli_install::InstallCli);
}

/// Adds the Help Commands to `engine`, sharing the [`Updates`] in its view state (which
/// [`configure`] fills in). The Shell does this.
pub fn install(engine: &mut Engine) {
    let shared = updates(engine);
    if engine.registry().get("app.update.check").is_none() {
        engine.register(|r| register_with(r, shared));
    }
}
