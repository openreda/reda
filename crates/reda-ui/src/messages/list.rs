//! How the Messages panel lists its lines: Group By, sorting and the severity chips, with
//! the waived Violations and the stale Waivers each in a collapsed group at the end.

use reda_engine::model::{Project, Severity, Uuid};

use super::{CheckKind, Line, LineKey, MessageGroup, Messages, check_title};
use crate::tr;

/// What the list groups lines by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupBy {
    /// `General`, then each Document in Document order.
    #[default]
    Document,
    /// Each check, by title.
    Check,
    /// Errors, then Warnings.
    Severity,
}

impl GroupBy {
    pub const ALL: [GroupBy; 3] = [GroupBy::Document, GroupBy::Check, GroupBy::Severity];

    pub fn label(self) -> String {
        match self {
            GroupBy::Document => tr!("messages-group-by-document"),
            GroupBy::Check => tr!("messages-group-by-check"),
            GroupBy::Severity => tr!("messages-group-by-severity"),
        }
    }
}

/// The order of lines within a group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    /// As the checks found them.
    #[default]
    Run,
    /// Errors first, then in run order.
    Severity,
    /// By message text.
    Message,
}

impl SortBy {
    pub const ALL: [SortBy; 3] = [SortBy::Run, SortBy::Severity, SortBy::Message];

    pub fn label(self) -> String {
        match self {
            SortBy::Run => tr!("messages-sort-run"),
            SortBy::Severity => tr!("messages-sort-severity"),
            SortBy::Message => tr!("messages-sort-message"),
        }
    }
}

/// What a group holds.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupKind {
    /// Lines about no Document.
    General,
    Document(Uuid),
    /// One check's Violations, by check id; load messages go under `General`.
    Check(String),
    Severity(Severity),
    /// The waived Violations.
    Waived,
    /// The Waivers that match nothing.
    StaleWaivers,
}

fn rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Off => 2,
    }
}

impl Messages {
    /// Every line, grouped as Group By says and sorted as Sort says, leaving out the
    /// severities whose chip is off and empty groups. By Document: `General` first, then
    /// each of `project`'s Documents in Document order, with lines naming a Document that
    /// is gone under `General`. Then, collapsed, the waived Violations (while they show)
    /// and the stale Waivers.
    pub fn groups(&self, project: Option<&Project>) -> Vec<MessageGroup> {
        let documents: Vec<(Uuid, String)> = project
            .map(|p| p.documents().map(|d| (d.uuid, d.title.clone())).collect())
            .unwrap_or_default();
        let known = |d: Option<Uuid>| d.filter(|d| documents.iter().any(|(u, _)| u == d));
        let mut lines: Vec<(Option<Uuid>, Option<String>, Line)> = Vec::new();
        for (i, note) in self.notes.iter().enumerate() {
            let line = Line {
                key: LineKey::Note(i),
                severity: note.severity,
                text: note.text.clone(),
                waived: false,
                stale: false,
            };
            lines.push((known(note.document), None, line));
        }
        let mut waived = Vec::new();
        for (key, entry) in self.listed() {
            let line = Line {
                key,
                severity: entry.violation.severity,
                text: entry.text.clone(),
                waived: entry.violation.waiver.is_some(),
                stale: self.stale_key(key),
            };
            if line.waived {
                waived.push(line);
            } else {
                let check = Some(entry.violation.check.clone());
                lines.push((known(entry.document), check, line));
            }
        }
        lines.retain(|(_, _, l)| self.severity_shown(l.severity));

        let mut groups: Vec<MessageGroup> = match self.group_by {
            GroupBy::Document => std::iter::once((GroupKind::General, tr!("messages-general")))
                .chain(
                    documents
                        .iter()
                        .map(|(u, t)| (GroupKind::Document(*u), t.clone())),
                )
                .map(|(kind, title)| group(kind, title, false))
                .collect(),
            GroupBy::Check => {
                let mut checks: Vec<(String, String)> = Vec::new();
                for (_, check, _) in &lines {
                    if let Some(c) = check
                        .as_ref()
                        .filter(|c| !checks.iter().any(|(id, _)| id == *c))
                    {
                        checks.push((c.clone(), check_title(c)));
                    }
                }
                checks.sort_by(|a, b| a.1.cmp(&b.1));
                std::iter::once(group(GroupKind::General, tr!("messages-general"), false))
                    .chain(
                        checks
                            .into_iter()
                            .map(|(id, title)| group(GroupKind::Check(id), title, false)),
                    )
                    .collect()
            }
            GroupBy::Severity => [
                (Severity::Error, tr!("messages-errors")),
                (Severity::Warning, tr!("messages-warnings")),
            ]
            .into_iter()
            .map(|(s, title)| group(GroupKind::Severity(s), title, false))
            .collect(),
        };
        for (document, check, line) in lines {
            let kind = match self.group_by {
                GroupBy::Document => document.map_or(GroupKind::General, GroupKind::Document),
                GroupBy::Check => check.map_or(GroupKind::General, GroupKind::Check),
                GroupBy::Severity if line.severity == Severity::Error => {
                    GroupKind::Severity(Severity::Error)
                }
                GroupBy::Severity => GroupKind::Severity(Severity::Warning),
            };
            if let Some(g) = groups.iter_mut().find(|g| g.kind == kind) {
                g.lines.push(line);
            }
        }
        if self.show_waived {
            let mut g = group(GroupKind::Waived, tr!("messages-waived"), true);
            g.lines = waived;
            groups.push(g);
        }
        let mut stale = group(GroupKind::StaleWaivers, tr!("messages-stale-waivers"), true);
        for run in &self.runs {
            for (i, w) in run.stale_waivers.iter().enumerate() {
                let check = check_title(&w.check);
                stale.lines.push(Line {
                    key: LineKey::StaleWaiver(run.kind, i),
                    severity: Severity::Warning,
                    text: tr!("messages-stale-waiver", check = check.as_str()),
                    waived: false,
                    stale: false,
                });
            }
        }
        groups.push(stale);
        for g in &mut groups {
            self.sort_lines(&mut g.lines);
        }
        groups.retain(|g| !g.lines.is_empty());
        groups
    }

    fn sort_lines(&self, lines: &mut [Line]) {
        match self.sort {
            SortBy::Run => {}
            SortBy::Severity => lines.sort_by_key(|l| rank(l.severity)),
            SortBy::Message => lines.sort_by(|a, b| a.text.cmp(&b.text)),
        }
    }

    fn stale_key(&self, key: LineKey) -> bool {
        match key {
            LineKey::Violation(kind, _) => self.run(kind).is_some_and(|r| r.stale),
            _ => false,
        }
    }

    /// How many unwaived Violations and load messages there are of each severity, for
    /// the chips: `(errors, warnings)`. The chips themselves don't change the counts.
    pub fn counts(&self) -> (usize, usize) {
        let severities = self.notes.iter().map(|n| n.severity).chain(
            self.listed()
                .filter(|(_, e)| e.violation.waiver.is_none())
                .map(|(_, e)| e.violation.severity),
        );
        severities.fold((0, 0), |(e, w), s| match s {
            Severity::Error => (e + 1, w),
            _ => (e, w + 1),
        })
    }

    /// The Violation lines in the order the list shows them, outside the collapsed
    /// groups: what next and previous Violation step through.
    pub fn order(&self, project: Option<&Project>) -> Vec<LineKey> {
        self.groups(project)
            .into_iter()
            .filter(|g| !g.collapsed)
            .flat_map(|g| g.lines)
            .map(|l| l.key)
            .filter(|k| matches!(k, LineKey::Violation(..)))
            .collect()
    }

    /// The check kinds that have run.
    pub fn kinds(&self) -> Vec<CheckKind> {
        self.runs.iter().map(|r| r.kind).collect()
    }
}

fn group(kind: GroupKind, title: String, collapsed: bool) -> MessageGroup {
    MessageGroup {
        document: match kind {
            GroupKind::Document(d) => Some(d),
            _ => None,
        },
        kind,
        title,
        lines: Vec::new(),
        collapsed,
    }
}
