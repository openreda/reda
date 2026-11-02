//! Release tooling for `.github/workflows/release.yml`
//! (docs/spec/v0.1/quality-and-release.md#releases).
//!
//! Nothing here touches git or GitHub: the workflow gathers the tags, labels, commit
//! message and PR titles, and this crate turns them into decisions and text.

mod notes;
mod plan;
mod stamp;
mod version;

pub use notes::{Change, parse_changes, release_notes};
pub use plan::{Bump, Plan, PlanInput, SKIP_LABEL, SKIP_MARKER, plan};
pub use stamp::{stamp_lock, stamp_manifest};
pub use version::Version;
