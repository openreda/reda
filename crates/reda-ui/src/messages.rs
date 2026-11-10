//! What the Messages panel shows: the last ERC and DRC results, load errors and warnings
//! (Preferences, the Keymap, missing fonts), grouped by Document; and the text of each
//! line. No egui here: the [`Shell`](crate::shell::Shell) keeps a [`Messages`] and the
//! panel draws [`Messages::groups`].
//!
//! A Violation carries no text: its line is built from the check id and its arguments
//! through the UI catalogue (`violation-<check>` keys).

mod list;
mod locate;
mod step;

use fluent_bundle::FluentValue;
use reda_engine::model::geom::Point;
use reda_engine::model::{
    Limit, Measurement, Project, Scope, Severity, Uuid, Violation, ViolationArg,
};
use reda_engine::preferences::{OverlayStyle, OverlayZoomedOut};

pub use list::{GroupBy, GroupKind, SortBy};
pub use locate::{Location, locate};
pub use step::{MessageSteps, Step};

use crate::{i18n, tr};

/// Whether waived Violations show: muted markers on the canvas and their collapsed group
/// in Messages. View state, set by `view.messages.show_waived`; on by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowWaived(pub bool);

impl Default for ShowWaived {
    fn default() -> ShowWaived {
        ShowWaived(true)
    }
}

/// Adds `view.messages.show_waived`, `view.checks.clear` and `view.messages.step` to
/// `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut reda_engine::command::Registry) {
    registry.add(ShowWaivedSet);
    registry.add(ChecksClear);
    registry.add(step::StepCommand);
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
    /// Show waived Violations or not; it flips when left out.
    #[serde(default)]
    show: Option<bool>,
}

/// Whether waived Violations now show.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct ShowWaivedResult {
    pub show: bool,
}

struct ShowWaivedSet;

impl reda_engine::command::Command for ShowWaivedSet {
    const META: reda_engine::command::Meta = reda_engine::command::Meta {
        id: "view.messages.show_waived",
        title: "Show Waived Violations",
        description: "Shows waived Violations, muted, on the canvas and in a collapsed \
                      group of the Messages panel, or hides them. On by default. View \
                      state only.",
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

/// A Waiver whose check ran across the whole design and matched nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleWaiver {
    pub waiver: Uuid,
    /// The check it waives, by id.
    pub check: String,
}

/// The results of the last run of one check. They stay on show until the next run.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckRun {
    pub kind: CheckKind,
    /// Every Violation, waived ones included (each names its Waiver).
    pub entries: Vec<Entry>,
    /// The Waivers that matched nothing.
    pub stale_waivers: Vec<StaleWaiver>,
    /// The design fingerprint the run returned, to tell when the results go stale.
    pub design: String,
    /// The design was edited after the run.
    pub stale: bool,
    /// When the check last ran in full, or for one check; `None` for the health checks a
    /// board runs as it opens, which are not a DRC run.
    pub ran_at: Option<std::time::SystemTime>,
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
    /// The stale Waiver at this index of the check's last run.
    StaleWaiver(CheckKind, usize),
}

/// A Violation as it stays the same across runs: its check and objects, as a Waiver
/// matches it. Violation Details follows one through re-runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViolationId {
    pub kind: CheckKind,
    pub check: String,
    /// Sorted.
    pub objects: Vec<Uuid>,
}

impl ViolationId {
    pub fn of(kind: CheckKind, violation: &Violation) -> ViolationId {
        let mut objects = violation.objects.clone();
        objects.sort();
        ViolationId {
            kind,
            check: violation.check.clone(),
            objects,
        }
    }
}

/// The Waive window: the reason being typed for the Violations it waives.
#[derive(Clone, Debug, PartialEq)]
pub struct WaivePrompt {
    pub violations: Vec<LineKey>,
    pub reason: String,
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

/// One group of lines: about one Document (or none, `General`), one check or one
/// severity, as Group By picks; or the waived Violations or the stale Waivers.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageGroup {
    /// The Document of a Document group.
    pub document: Option<Uuid>,
    pub kind: GroupKind,
    pub title: String,
    pub lines: Vec<Line>,
    /// It starts collapsed: the waived and stale-Waiver groups.
    pub collapsed: bool,
}

/// A Violation's marker on its Document's canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    /// Where, in Document nm.
    pub at: Point,
    pub severity: Severity,
    pub waived: bool,
    /// From a run the design has changed since: drawn dimmed.
    pub stale: bool,
    /// The Violation last gone to from the panel.
    pub current: bool,
    /// Its line in Messages: a right-click on the marker waives it or sets its severity.
    pub key: LineKey,
    /// The check's Details are on: the ring, and zoomed in the value and the line.
    pub details: bool,
    /// The value against the limit, such as `0.1mm under 0.2mm`, shown zoomed in.
    pub value: Option<String>,
    /// The two closest points, joined by a line zoomed in.
    pub points: Option<[Point; 2]>,
    /// The Layers involved: the marker draws only while one shows. Empty on a sheet.
    pub layers: Vec<Uuid>,
    /// The objects the overlay colours, with how; `None` when the check's Overlay is off.
    pub overlay: Option<OverlayLook>,
    pub objects: Vec<Uuid>,
}

/// How a marker's overlay is drawn: the Checks Preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayLook {
    pub style: OverlayStyle,
    pub zoomed_out: OverlayZoomedOut,
}

/// Everything the Messages panel lists, and how.
#[derive(Clone, Debug)]
pub struct Messages {
    runs: Vec<CheckRun>,
    notes: Vec<Note>,
    show_waived: bool,
    current: Option<LineKey>,
    /// More lines picked with Mod+click, for Waive.
    picked: Vec<LineKey>,
    group_by: GroupBy,
    sort: SortBy,
    /// The severities whose chips are off.
    hidden: Vec<Severity>,
    /// The Violation the Violation Details window shows, while it is open.
    details: Option<ViolationId>,
    waive_prompt: Option<WaivePrompt>,
}

impl Default for Messages {
    fn default() -> Messages {
        Messages {
            runs: Vec::new(),
            notes: Vec::new(),
            show_waived: true,
            current: None,
            picked: Vec::new(),
            group_by: GroupBy::default(),
            sort: SortBy::default(),
            hidden: Vec::new(),
            details: None,
            waive_prompt: None,
        }
    }
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
        self.picked.clear();
        if self
            .waive_prompt
            .as_ref()
            .is_some_and(|p| p.violations.iter().any(|k| key_kind(*k) == Some(run.kind)))
        {
            self.waive_prompt = None;
        }
        self.runs.retain(|r| r.kind != run.kind);
        self.runs.push(run);
        self.runs.sort_by_key(|r| r.kind);
    }

    /// Forgets every check result, as when another Project opens.
    pub(crate) fn clear_runs(&mut self) {
        self.runs.clear();
        self.current = None;
        self.picked.clear();
        self.details = None;
        self.waive_prompt = None;
    }

    /// The Violation a line shows.
    pub fn entry(&self, key: LineKey) -> Option<&Entry> {
        match key {
            LineKey::Violation(kind, i) => self.run(kind)?.entries.get(i),
            LineKey::Note(_) | LineKey::StaleWaiver(..) => None,
        }
    }

    /// The stale Waiver a line shows.
    pub fn stale_waiver(&self, key: LineKey) -> Option<&StaleWaiver> {
        match key {
            LineKey::StaleWaiver(kind, i) => self.run(kind)?.stale_waivers.get(i),
            _ => None,
        }
    }

    /// The line of the Violation `id` names in the last runs, if it is still there.
    pub fn find(&self, id: &ViolationId) -> Option<LineKey> {
        let run = self.run(id.kind)?;
        run.entries
            .iter()
            .position(|e| ViolationId::of(id.kind, &e.violation) == *id)
            .map(|i| LineKey::Violation(id.kind, i))
    }

    /// The lines picked: the current one and those Mod+clicked, in the order picked.
    pub fn picked(&self) -> Vec<LineKey> {
        let mut out: Vec<LineKey> = self.current.into_iter().collect();
        out.extend(self.picked.iter().filter(|k| Some(**k) != self.current));
        out
    }

    /// Adds `key` to the picked lines, or takes it out again.
    pub(crate) fn toggle_picked(&mut self, key: LineKey) {
        match self.picked.iter().position(|k| *k == key) {
            Some(i) => {
                self.picked.remove(i);
            }
            None if self.current == Some(key) => self.current = None,
            None => self.picked.push(key),
        }
    }

    pub fn group_by(&self) -> GroupBy {
        self.group_by
    }

    pub(crate) fn set_group_by(&mut self, group_by: GroupBy) {
        self.group_by = group_by;
    }

    pub fn sort(&self) -> SortBy {
        self.sort
    }

    pub(crate) fn set_sort(&mut self, sort: SortBy) {
        self.sort = sort;
    }

    /// Whether the chip of `severity` is on, so its lines are listed.
    pub fn severity_shown(&self, severity: Severity) -> bool {
        !self.hidden.contains(&severity)
    }

    pub(crate) fn set_severity_shown(&mut self, severity: Severity, shown: bool) {
        self.hidden.retain(|s| *s != severity);
        if !shown {
            self.hidden.push(severity);
        }
    }

    /// The Violation the Violation Details window shows, while it is open.
    pub fn details(&self) -> Option<&ViolationId> {
        self.details.as_ref()
    }

    pub(crate) fn set_details(&mut self, id: Option<ViolationId>) {
        self.details = id;
    }

    /// The Waive window's state, while it is open.
    pub fn waive_prompt(&self) -> Option<&WaivePrompt> {
        self.waive_prompt.as_ref()
    }

    pub(crate) fn waive_prompt_mut(&mut self) -> Option<&mut WaivePrompt> {
        self.waive_prompt.as_mut()
    }

    pub(crate) fn set_waive_prompt(&mut self, prompt: Option<WaivePrompt>) {
        self.waive_prompt = prompt;
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
    format!("{}mm", reda_engine::model::geom::Nm(n).mm_exact())
}

/// The check a line belongs to.
fn key_kind(key: LineKey) -> Option<CheckKind> {
    match key {
        LineKey::Violation(kind, _) | LineKey::StaleWaiver(kind, _) => Some(kind),
        LineKey::Note(_) => None,
    }
}

/// A measured value as text: a length in mm, an area in mm².
pub fn measurement(m: &Measurement) -> String {
    match m {
        Measurement::Length(n) => mm(n.0),
        Measurement::Area(a) => {
            let text = mm(*a / 1_000_000);
            format!("{}mm²", text.trim_end_matches("mm"))
        }
    }
}

/// A Violation's value against its limit, such as `0.1mm under 0.2mm`; `None` for a
/// check that measures nothing.
pub fn value_text(violation: &Violation) -> Option<String> {
    let (measured, required) = (violation.measured.as_ref()?, violation.required.as_ref()?);
    let (measured, required) = (measurement(measured), measurement(required));
    Some(match violation.limit {
        Some(Limit::Max) => tr!("marker-over", measured = measured, required = required),
        _ => tr!("marker-under", measured = measured, required = required),
    })
}

/// Where Show rule goes for a check: a DRC check's Board Setup rule page (its DRC page
/// when no rule page has it), or the ERC page of Project Settings. The window and the
/// page's title.
pub fn rule_page(kind: CheckKind, check: &str) -> (crate::layout::SettingsWindow, String) {
    use crate::layout::SettingsWindow;
    match kind {
        CheckKind::Erc => (SettingsWindow::ProjectSettings, tr!("settings-page-erc")),
        CheckKind::Drc => {
            let page = crate::settings::board_setup::Rule::ALL
                .into_iter()
                .find(|r| {
                    r.sections()
                        .iter()
                        .any(|s| s.check.is_some_and(|c| c.id() == check))
                })
                .map_or_else(|| tr!("board-setup-page-drc"), |r| r.title());
            (SettingsWindow::BoardSetup, page)
        }
    }
}
