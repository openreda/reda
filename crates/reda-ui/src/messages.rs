//! What the Messages panel shows: the last ERC and DRC results, load errors and warnings
//! (Preferences, the Keymap, missing fonts), grouped by Document; and the text of each
//! line. No egui here: the [`Shell`](crate::shell::Shell) keeps a [`Messages`] and the
//! panel draws [`Messages::groups`].
//!
//! A Violation carries no text: its line is built from the check id and its arguments
//! through the UI catalogue (`violation-<check>` keys).

mod locate;

use fluent_bundle::FluentValue;
use reda_engine::model::geom::Point;
use reda_engine::model::{Project, Scope, Severity, Uuid, Violation, ViolationArg};

pub use locate::{Location, locate};

use crate::{i18n, tr};

/// Whether the Messages panel lists waived Violations: view state, set by
/// `view.messages.show_waived`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShowWaived(pub bool);

/// Adds `view.messages.show_waived` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut reda_engine::command::Registry) {
    registry.add(ShowWaivedSet);
    registry.add(ChecksClear);
}

/// How many times `view.checks.clear` has run: view state the Shell follows to forget
/// the check results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChecksCleared(pub u64);

struct ChecksClear;

impl reda_engine::command::Command for ChecksClear {
    const META: reda_engine::command::Meta = reda_engine::command::Meta {
        id: "view.checks.clear",
        title: "Clear Check Results",
        description: "Clears the ERC and DRC results from the Messages panel and their \
                      markers from the canvas. View state only: it resolves nothing, so \
                      the next run lists them again.",
        category: "view",
        icon: "close",
        kind: reda_engine::command::Kind::View,
    };
    type Params = reda_engine::command::NoParams;
    type Output = ();

    fn run(
        &self,
        cx: &mut reda_engine::command::Context<'_>,
        _: reda_engine::command::NoParams,
    ) -> Result<(), reda_engine::command::CommandError> {
        let cleared = cx.view::<ChecksCleared>()?;
        cleared.0 += 1;
        Ok(())
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShowWaivedParams {
    /// List waived Violations or not; it flips when left out.
    #[serde(default)]
    show: Option<bool>,
}

/// Whether waived Violations are now listed.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct ShowWaivedResult {
    pub show: bool,
}

struct ShowWaivedSet;

impl reda_engine::command::Command for ShowWaivedSet {
    const META: reda_engine::command::Meta = reda_engine::command::Meta {
        id: "view.messages.show_waived",
        title: "Show Waived Violations",
        description: "Lists waived Violations in the Messages panel (and marks them on the \
                      canvas), or hides them again. View state only.",
        category: "view",
        icon: "waive",
        kind: reda_engine::command::Kind::View,
    };
    type Params = ShowWaivedParams;
    type Output = ShowWaivedResult;

    fn run(
        &self,
        cx: &mut reda_engine::command::Context<'_>,
        p: ShowWaivedParams,
    ) -> Result<ShowWaivedResult, reda_engine::command::CommandError> {
        let state = cx.view::<ShowWaived>()?;
        state.0 = p.show.unwrap_or(!state.0);
        Ok(ShowWaivedResult { show: state.0 })
    }
}

/// A check whose results the panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckKind {
    Erc,
    Drc,
}

impl CheckKind {
    pub const ALL: [CheckKind; 2] = [CheckKind::Erc, CheckKind::Drc];

    /// The Command prefix: `erc` or `drc`.
    pub const fn id(self) -> &'static str {
        match self {
            CheckKind::Erc => "erc",
            CheckKind::Drc => "drc",
        }
    }
}

/// One Violation of a run, with the Document it belongs to and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub violation: Violation,
    /// The Document holding its first object, when that object still exists.
    pub document: Option<Uuid>,
    pub text: String,
}

/// The results of the last run of one check. They stay on show until the next run.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckRun {
    pub kind: CheckKind,
    /// Every Violation, waived ones included (each names its Waiver).
    pub entries: Vec<Entry>,
    /// The design fingerprint the run returned, to tell when the results go stale.
    pub design: String,
    /// The design was edited after the run.
    pub stale: bool,
}

/// A load error or warning, such as a Preference the file got wrong.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// What reported it (`preferences`, `keymap`, `fonts`…); a source's notes are replaced
    /// together.
    pub source: String,
    /// The Document it concerns; `None` for the whole application.
    pub document: Option<Uuid>,
    pub severity: Severity,
    pub text: String,
}

/// Which message a line shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineKey {
    /// The Violation at this index of the check's last run.
    Violation(CheckKind, usize),
    /// The load message at this index.
    Note(usize),
}

/// One line of the panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub key: LineKey,
    pub severity: Severity,
    pub text: String,
    /// A Waiver waives it; shown only while waived Violations are.
    pub waived: bool,
    /// From a run the design has changed since.
    pub stale: bool,
}

/// The lines about one Document, or about none (`General`).
#[derive(Clone, Debug, PartialEq)]
pub struct MessageGroup {
    pub document: Option<Uuid>,
    pub title: String,
    pub lines: Vec<Line>,
}

/// A Violation's marker on its Document's canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    /// Where, in Document nm.
    pub at: Point,
    pub severity: Severity,
    pub waived: bool,
    /// The Violation last gone to from the panel.
    pub current: bool,
}

/// Everything the Messages panel lists.
#[derive(Clone, Debug, Default)]
pub struct Messages {
    runs: Vec<CheckRun>,
    notes: Vec<Note>,
    show_waived: bool,
    current: Option<LineKey>,
}

impl Messages {
    /// The last run of `kind`, if it has run.
    pub fn run(&self, kind: CheckKind) -> Option<&CheckRun> {
        self.runs.iter().find(|r| r.kind == kind)
    }

    pub(crate) fn run_mut(&mut self, kind: CheckKind) -> Option<&mut CheckRun> {
        self.runs.iter_mut().find(|r| r.kind == kind)
    }

    /// Replaces the results of `run.kind`.
    pub(crate) fn set_run(&mut self, run: CheckRun) {
        if self
            .current
            .is_some_and(|c| matches!(c, LineKey::Violation(k, _) if k == run.kind))
        {
            self.current = None;
        }
        self.runs.retain(|r| r.kind != run.kind);
        self.runs.push(run);
        self.runs.sort_by_key(|r| r.kind);
    }

    /// Forgets every check result, as when another Project opens.
    pub(crate) fn clear_runs(&mut self) {
        self.runs.clear();
        self.current = None;
    }

    /// The Violation a line shows.
    pub fn entry(&self, key: LineKey) -> Option<&Entry> {
        match key {
            LineKey::Violation(kind, i) => self.run(kind)?.entries.get(i),
            LineKey::Note(_) => None,
        }
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// Replaces the load messages from `source`.
    pub(crate) fn set_notes(&mut self, source: &str, notes: Vec<Note>) {
        if self.current.is_some_and(|c| matches!(c, LineKey::Note(_))) {
            self.current = None;
        }
        self.notes.retain(|n| n.source != source);
        self.notes.extend(notes);
    }

    /// Whether waived Violations are listed.
    pub fn show_waived(&self) -> bool {
        self.show_waived
    }

    pub(crate) fn set_show_waived(&mut self, show: bool) {
        self.show_waived = show;
    }

    /// The line last gone to.
    pub fn current(&self) -> Option<LineKey> {
        self.current
    }

    pub(crate) fn set_current(&mut self, key: Option<LineKey>) {
        self.current = key;
    }

    /// The Violations listed now (waived ones only while they are shown), with their keys.
    pub fn listed(&self) -> impl Iterator<Item = (LineKey, &Entry)> {
        self.runs.iter().flat_map(move |run| {
            run.entries
                .iter()
                .enumerate()
                .filter(move |(_, e)| self.show_waived || e.violation.waiver.is_none())
                .map(move |(i, e)| (LineKey::Violation(run.kind, i), e))
        })
    }

    /// Every line, grouped: `General` first, then each of `project`'s Documents in
    /// Document order. Groups with no lines are left out. Lines naming a Document that is
    /// gone go under `General`.
    pub fn groups(&self, project: Option<&Project>) -> Vec<MessageGroup> {
        let documents: Vec<(Uuid, String)> = project
            .map(|p| p.documents().map(|d| (d.uuid, d.title.clone())).collect())
            .unwrap_or_default();
        let known = |d: Option<Uuid>| d.filter(|d| documents.iter().any(|(u, _)| u == d));
        let mut groups: Vec<MessageGroup> = std::iter::once(MessageGroup {
            document: None,
            title: tr!("messages-general"),
            lines: Vec::new(),
        })
        .chain(documents.iter().map(|(uuid, title)| MessageGroup {
            document: Some(*uuid),
            title: title.clone(),
            lines: Vec::new(),
        }))
        .collect();
        let mut push = |document: Option<Uuid>, line: Line| {
            let document = known(document);
            if let Some(g) = groups.iter_mut().find(|g| g.document == document) {
                g.lines.push(line);
            }
        };
        for (i, note) in self.notes.iter().enumerate() {
            push(
                note.document,
                Line {
                    key: LineKey::Note(i),
                    severity: note.severity,
                    text: note.text.clone(),
                    waived: false,
                    stale: false,
                },
            );
        }
        for (key, entry) in self.listed() {
            let stale = match key {
                LineKey::Violation(kind, _) => self.run(kind).is_some_and(|r| r.stale),
                LineKey::Note(_) => false,
            };
            push(
                entry.document,
                Line {
                    key,
                    severity: entry.violation.severity,
                    text: entry.text.clone(),
                    waived: entry.violation.waiver.is_some(),
                    stale,
                },
            );
        }
        groups.retain(|g| !g.lines.is_empty());
        groups
    }
}

/// The check's Document, retained for stale results even when every object is deleted.
pub fn violation_document(project: &Project, violation: &Violation) -> Option<Uuid> {
    violation.document.or_else(|| {
        violation
            .objects
            .iter()
            .find_map(|o| match project.object(*o)?.scope() {
                Scope::Document(d) => Some(d),
                _ => None,
            })
    })
}

/// The text of a Violation, from its check id and arguments; the check id itself when
/// the catalogue has no text for it.
pub fn violation_text(violation: &Violation) -> String {
    let key = format!("violation-{}", violation.check.replace('_', "-"));
    if !i18n::has_key(&key) {
        return violation.check.clone();
    }
    let args: Vec<(&str, FluentValue<'_>)> = violation
        .args
        .iter()
        .map(|(name, value)| {
            let value = match value {
                ViolationArg::Int(n)
                    if reda_engine::model::LENGTH_ARGS.contains(&name.as_str()) =>
                {
                    FluentValue::from(mm(*n))
                }
                ViolationArg::Int(n) => FluentValue::from(*n),
                ViolationArg::Text(s) => FluentValue::from(s.as_str()),
            };
            (name.as_str(), value)
        })
        .collect();
    i18n::translate(&key, &args)
}

/// The Pin Matrix cell an ERC Violation of `unconnected_pin` or `pin_conflict` belongs to,
/// as its two type names: what **Set severity** changes for such a line.
pub fn matrix_cell(violation: &Violation) -> Option<(String, String)> {
    let text = |name: &str| match violation.args.get(name) {
        Some(ViolationArg::Text(t)) => Some(t.clone()),
        _ => None,
    };
    match violation.check.as_str() {
        "unconnected_pin" => Some((text("type")?, "unconnected".to_owned())),
        "pin_conflict" => Some((text("type_a")?, text("type_b")?)),
        _ => None,
    }
}

/// A check's title from the catalogue (`check-<id>`); its id when there is none.
pub fn check_title(check: &str) -> String {
    let key = format!("check-{}", check.replace('_', "-"));
    if i18n::has_key(&key) {
        i18n::translate(&key, &[])
    } else {
        check.to_owned()
    }
}

/// `n` nm as exact millimetres with REDA's suffix: `5.08mm`, `-0.2mm`, `0mm`.
pub fn mm(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let n = n.unsigned_abs();
    let (whole, frac) = (n / 1_000_000, n % 1_000_000);
    if frac == 0 {
        return format!("{sign}{whole}mm");
    }
    let frac = format!("{frac:06}");
    format!("{sign}{whole}.{}mm", frac.trim_end_matches('0'))
}
