//! The always-bridge (ADR-0007): when a running instance holds the target Project, `reda
//! run` and `reda mcp` send their Commands to it over the control port instead of opening
//! the Project in-process. Bridged sessions never save; the instance refuses
//! `project.save` and shows the edits as unsaved changes.

use std::path::Path;

use reda_control::client::{self, Connection};
use reda_engine::journal::Source;

/// Commands that choose their own Project. Run on their own, they never bridge.
const OWN_PROJECT: &[&str] = &["project.new", "project.open"];

/// Connects to the instance to bridge to: the one with `project` open, or with no
/// `project`, the only instance that has a Project open. `None` means run in-process.
pub fn connect(project: Option<&str>, source: Source) -> Option<Connection> {
    let dirs = reda_engine::user_dirs::discover()?;
    client::bridge(&dirs, project.map(Path::new), source)
}

/// Whether running `id` on its own may bridge.
pub fn may_bridge(id: &str) -> bool {
    let base = id.split_once(':').map_or(id, |(base, _)| base);
    !OWN_PROJECT.contains(&base)
}

/// The stderr note for `--save` on a bridged run.
pub fn not_saving(connection: &Connection) -> String {
    let project = connection
        .project()
        .map_or_else(|| "the Project".to_owned(), |p| p.display().to_string());
    format!(
        "reda: not saving: a running REDA app has {project} open, so the changes show as \
         unsaved there"
    )
}
