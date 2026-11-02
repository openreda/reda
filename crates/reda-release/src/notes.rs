//! Release notes, made from the titles of the pull requests merged since the previous
//! release. There is no CHANGELOG.

use crate::Version;

/// One change in a release: a merged pull request, or a commit pushed without one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub pr: Option<u64>,
    pub title: String,
}

/// Reads the changes the workflow gathered: one `<pr number or empty>\t<title>` line per
/// commit, oldest first. Blank lines are ignored, and a pull request listed twice keeps its
/// first place.
pub fn parse_changes(text: &str) -> Vec<Change> {
    let mut changes: Vec<Change> = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (pr, title) = line.split_once('\t').unwrap_or(("", line));
        let change = Change {
            pr: pr.trim().parse().ok(),
            title: title.trim().to_owned(),
        };
        let seen = change.pr.is_some() && changes.iter().any(|c| c.pr == change.pr);
        if !seen {
            changes.push(change);
        }
    }
    changes
}

/// The Markdown body of a GitHub release.
pub fn release_notes(version: Version, changes: &[Change]) -> String {
    let mut notes = format!(
        "REDA {version} is an early tester build. The packages are not signed yet, so macOS \
         and Windows warn before the first launch: see [Installing an unsigned build]\
         (https://github.com/openreda/reda#installing-an-unsigned-build).\n\n## Changes\n\n"
    );
    if changes.is_empty() {
        notes.push_str("- No pull requests since the previous release.\n");
    }
    for change in changes {
        match change.pr {
            Some(pr) => notes.push_str(&format!("- {} (#{pr})\n", change.title)),
            None => notes.push_str(&format!("- {}\n", change.title)),
        }
    }
    notes.push_str("\nEvery file's SHA-256 is in `SHA256SUMS`.\n");
    notes
}
