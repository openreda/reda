//! Whether a merge to `main` releases, and as which version.

use crate::Version;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

/// What the workflow knows about one push to `main` (or one manual run).
#[derive(Clone, Copy, Debug)]
pub struct PlanInput<'a> {
    /// Every tag in the repository; only `vX.Y.Z` ones count.
    pub tags: &'a [&'a str],
    /// The labels of the pull request that was merged.
    pub labels: &'a [&'a str],
    /// The full message of the commit that landed on `main`.
    pub message: &'a str,
    /// The bump asked for by `workflow_dispatch`, which releases even when skipping is asked.
    pub forced: Option<Bump>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    Skip {
        reason: String,
    },
    Release {
        version: Version,
        previous: Option<Version>,
    },
}

/// Decides the release for one push to `main`:
/// - The previous release is the highest `vX.Y.Z` tag; with none, the first release is 0.0.1.
/// - Every release bumps the patch, unless a `release:minor` or `release:major` label (or a
///   forced bump) asks for more. The largest bump wins.
/// - `release:skip` or `[skip release]` in the commit message skips, unless forced.
pub fn plan(input: &PlanInput<'_>) -> Plan {
    if input.forced.is_none() {
        if input.labels.contains(&SKIP_LABEL) {
            return Plan::Skip {
                reason: format!("the pull request has the {SKIP_LABEL} label"),
            };
        }
        if input.message.contains(SKIP_MARKER) {
            return Plan::Skip {
                reason: format!("the commit message contains {SKIP_MARKER}"),
            };
        }
    }
    let previous = input
        .tags
        .iter()
        .filter_map(|tag| Version::from_tag(tag))
        .max();
    let bump = input
        .labels
        .iter()
        .filter_map(|label| Bump::from_label(label))
        .chain(input.forced)
        .max()
        .unwrap_or(Bump::Patch);
    Plan::Release {
        version: previous.unwrap_or_default().bump(bump),
        previous,
    }
}

/// The label that skips a release, and the marker that does the same in a commit message.
pub const SKIP_LABEL: &str = "release:skip";
pub const SKIP_MARKER: &str = "[skip release]";

impl Bump {
    /// `release:minor` and `release:major`; every other label leaves the patch bump.
    pub fn from_label(label: &str) -> Option<Bump> {
        match label {
            "release:minor" => Some(Bump::Minor),
            "release:major" => Some(Bump::Major),
            _ => None,
        }
    }
}
