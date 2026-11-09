//! The Shell's side of the check results: running ERC and DRC into Messages, going to a
//! Violation, waiving and severities, Violation Details, and the canvas markers.

use std::collections::BTreeMap;
use std::sync::Arc;

use reda_engine::command::CommandError;
use reda_engine::model::{DrcCheck, Object, Project, Severity, Uuid, Violation};
use serde_json::{Value, json};

use super::Shell;
use crate::messages::{
    self, CheckKind, CheckRun, Entry, GroupBy, LineKey, Marker, MessageGroup, MessageSteps,
    Messages, Note, OverlayLook, SortBy, StaleWaiver, Step, ViolationId, WaivePrompt,
};

impl Shell {
    /// What the Messages panel lists.
    pub fn messages(&self) -> &Messages {
        &self.messages
    }

    /// The Messages panel's lines, grouped as Group By says.
    pub fn message_groups(&self) -> Vec<MessageGroup> {
        self.messages.groups(self.engine.project())
    }

    /// Runs ERC or DRC (`erc.run`/`drc.run`) and shows its Violations, waived ones
    /// included so the panel can toggle them. An error goes to the status bar.
    pub fn run_check(&mut self, kind: CheckKind) -> Result<(), CommandError> {
        let out = self.run_reporting(
            &format!("{}.run", kind.id()),
            json!({"include_waived": true}),
        )?;
        self.show_run(kind, &out, None, true);
        self.open_messages_on_errors(kind);
        Ok(())
    }

    /// Shows Messages when the run of `kind` found an unwaived Error: a run with only
    /// Warnings leaves the panels as they are (docs/spec/v0.1/checks.md, Auto-open).
    fn open_messages_on_errors(&mut self, kind: CheckKind) {
        let errors = self.messages.run(kind).is_some_and(|run| {
            run.entries
                .iter()
                .any(|e| e.violation.waiver.is_none() && e.violation.severity == Severity::Error)
        });
        if errors {
            let _ = self
                .layout_mut()
                .show_panel(crate::layout::PanelId::Messages);
        }
    }

    /// Runs one check only (`checks: [check]`) and replaces just that check's lines,
    /// keeping the rest of the last run.
    pub fn run_check_only(&mut self, kind: CheckKind, check: &str) -> Result<(), CommandError> {
        let out = self.run_reporting(
            &format!("{}.run", kind.id()),
            json!({"include_waived": true, "checks": [check]}),
        )?;
        self.show_run(kind, &out, Some(check), true);
        self.open_messages_on_errors(kind);
        Ok(())
    }

    /// Runs the DRC health checks on the board of the Project just opened and lists what
    /// they find in Messages. It never opens the panel, runs no Command (so the Journal
    /// stays the user's) and edits nothing, so the Project stays clean. A board with
    /// nothing to report leaves Messages as it is.
    pub(super) fn run_health_checks(&mut self) {
        let Some(pcb) = self.engine.project().and_then(Project::pcb) else {
            return;
        };
        let found = reda_engine::pcb::drc::health(pcb, None);
        if found.is_empty() {
            return;
        }
        let severities = pcb.setup().severities;
        let waived = reda_engine::model::apply_waivers(found, pcb.waivers(), true, |id| {
            DrcCheck::HEALTH
                .iter()
                .any(|c| c.id() == id && severities.get(*c) != Severity::Off)
        });
        let out = json!({
            "violations": waived.violations,
            "design": reda_engine::pcb::drc::design_fingerprint(pcb),
        });
        self.show_run(CheckKind::Drc, &out, None, false);
    }

    /// Shows a check run's result (`erc.run`/`drc.run` output) in Messages and on the
    /// canvas. With `only`, the run covered that one check, so the last run's other
    /// lines stay. `ran` is false for the health checks run on open, which keep the last
    /// run's time.
    fn show_run(&mut self, kind: CheckKind, out: &Value, only: Option<&str>, ran: bool) {
        let violations: Vec<Violation> =
            serde_json::from_value(out["violations"].clone()).unwrap_or_default();
        let unmatched: Vec<Uuid> =
            serde_json::from_value(out["unmatched_waivers"].clone()).unwrap_or_default();
        let project = self.engine.project();
        let mut entries: Vec<Entry> = violations
            .into_iter()
            .map(|violation| Entry {
                document: project.and_then(|p| messages::violation_document(p, &violation)),
                text: messages::violation_text(&violation),
                violation,
            })
            .collect();
        let mut stale_waivers: Vec<StaleWaiver> = unmatched
            .into_iter()
            .map(|waiver| StaleWaiver {
                waiver,
                check: match project.and_then(|p| p.object(waiver)) {
                    Some(Object::Waiver(w)) => w.object.check.clone(),
                    _ => String::new(),
                },
            })
            .collect();
        if let (Some(check), Some(last)) = (only, self.messages.run(kind)) {
            let mut kept: Vec<Entry> = (last.entries.iter())
                .filter(|e| e.violation.check != check)
                .cloned()
                .collect();
            kept.append(&mut entries);
            entries = kept;
            let mut kept: Vec<StaleWaiver> = (last.stale_waivers.iter())
                .filter(|w| w.check != check)
                .cloned()
                .collect();
            kept.append(&mut stale_waivers);
            stale_waivers = kept;
        }
        let current = self
            .messages
            .current()
            .and_then(|k| self.messages.entry(k))
            .map(|e| ViolationId::of(kind, &e.violation));
        self.messages.set_run(CheckRun {
            kind,
            entries,
            stale_waivers,
            design: out["design"].as_str().unwrap_or_default().to_owned(),
            stale: false,
            ran_at: match ran {
                true => Some(std::time::SystemTime::now()),
                false => self.messages.run(kind).and_then(|r| r.ran_at),
            },
        });
        // The line gone to stays current when the run still has it.
        if let Some(key) = current.and_then(|id| self.messages.find(&id)) {
            self.messages.set_current(Some(key));
        }
        self.update_markers();
    }

    /// Shows waived Violations or hides them (`view.messages.show_waived`).
    pub fn set_show_waived(&mut self, show: bool) -> Result<Value, CommandError> {
        self.run_reporting("view.messages.show_waived", json!({ "show": show }))
    }

    /// Follows the view state the Messages Commands set: showing waived Violations,
    /// `view.checks.clear` and the next/previous Violation keys. After a Preferences
    /// change the markers follow the Checks Preferences.
    pub(super) fn sync_messages(&mut self, command: &str) {
        let show = self
            .engine
            .view()
            .get::<messages::ShowWaived>()
            .is_none_or(|s| s.0);
        if show != self.messages.show_waived() {
            self.messages.set_show_waived(show);
            self.update_markers();
        }
        let cleared = self
            .engine
            .view()
            .get::<messages::ChecksCleared>()
            .map_or(0, |c| c.0);
        if cleared != self.checks_cleared {
            self.checks_cleared = cleared;
            self.messages.clear_runs();
            self.update_markers();
        }
        let steps = self
            .engine
            .view_mut()
            .get_mut::<MessageSteps>()
            .map(|s| std::mem::take(&mut s.0))
            .unwrap_or_default();
        for step in steps {
            self.step_violation(step);
        }
        if command.starts_with("app.preferences.") {
            self.update_markers();
        }
    }

    /// Goes to the next or previous Violation in the list's order, wrapping around.
    fn step_violation(&mut self, step: Step) {
        let order = self.messages.order(self.engine.project());
        if order.is_empty() {
            return;
        }
        let at = self
            .messages
            .current()
            .and_then(|c| order.iter().position(|k| *k == c));
        let n = order.len();
        let next = match (step, at) {
            (Step::Next, Some(i)) => (i + 1) % n,
            (Step::Next, None) => 0,
            (Step::Previous, Some(i)) => (i + n - 1) % n,
            (Step::Previous, None) => n - 1,
        };
        self.go_to(order[next]);
    }

    /// Goes to a line's Violation: opens its Document (`document.open`), zooms to it and
    /// selects the objects involved (`selection.set`). A load message about a Document
    /// opens that Document.
    pub fn go_to(&mut self, key: LineKey) {
        let (document, objects) = match key {
            LineKey::Violation(..) => match self.messages.entry(key) {
                Some(e) => (e.document, e.violation.objects.clone()),
                None => return,
            },
            LineKey::Note(i) => match self.messages.notes().get(i) {
                Some(n) => (n.document, Vec::new()),
                None => return,
            },
            LineKey::StaleWaiver(..) => (None, Vec::new()),
        };
        self.messages.set_current(Some(key));
        if let Some(document) = document
            && self
                .run_reporting("document.open", json!({ "document": document }))
                .is_ok()
        {
            let location = self.messages.entry(key).and_then(|e| {
                let tab = self.document_tab(document)?;
                let project = self.engine.project()?;
                messages::locate(project, document, &tab.scene.scene, &e.violation)
            });
            if let (Some(location), Some(tab)) = (location, self.document_tab_mut(document)) {
                tab.zoom_to = Some(location.frame);
            }
        }
        self.select_existing(objects);
        self.update_markers();
    }

    /// `selection.set` takes only objects the Project has; a Violation may also name what
    /// has no UUID of its own in the model.
    fn select_existing(&mut self, objects: Vec<Uuid>) {
        let objects: Vec<Uuid> = match self.engine.project() {
            Some(project) => objects
                .into_iter()
                .filter(|o| project.object(*o).is_some())
                .collect(),
            None => Vec::new(),
        };
        let _ = self.run_reporting("selection.set", json!({ "objects": objects }));
    }

    /// Goes to one object of a Violation, from the details pane or Violation Details:
    /// opens `document`, zooms to the object and selects it.
    pub fn go_to_object(&mut self, document: Option<Uuid>, object: Uuid) {
        if let Some(document) = document
            && self
                .run_reporting("document.open", json!({ "document": document }))
                .is_ok()
        {
            let _ = self.run_reporting(
                "view.zoom_to",
                json!({ "document": document, "objects": [object] }),
            );
        }
        self.select_existing(vec![object]);
    }

    /// Mod+click on a line: adds it to the lines Waive covers, or takes it out.
    pub fn toggle_picked(&mut self, key: LineKey) {
        self.messages.toggle_picked(key);
    }

    pub fn set_group_by(&mut self, group_by: GroupBy) {
        self.messages.set_group_by(group_by);
    }

    pub fn set_sort(&mut self, sort: SortBy) {
        self.messages.set_sort(sort);
    }

    /// Turns a severity chip on or off.
    pub fn set_severity_shown(&mut self, severity: Severity, shown: bool) {
        self.messages.set_severity_shown(severity, shown);
    }

    /// Opens Violation Details on a line's Violation; it follows the Violation through
    /// later runs.
    pub fn open_details(&mut self, key: LineKey) {
        if let LineKey::Note(i) = key
            && crate::pcb::measure::show_again(self, i)
        {
            return;
        }
        if let LineKey::Violation(kind, _) = key
            && let Some(e) = self.messages.entry(key)
        {
            let id = ViolationId::of(kind, &e.violation);
            self.messages.set_details(Some(id));
        }
    }

    pub fn close_details(&mut self) {
        self.messages.set_details(None);
    }

    /// The line Violation Details shows, while it is open and the Violation is listed.
    pub fn details_line(&self) -> Option<LineKey> {
        self.messages
            .details()
            .and_then(|id| self.messages.find(id))
    }

    /// Opens the Waive window for `keys`' Violations: Waive asks for the reason first.
    pub fn start_waive(&mut self, keys: Vec<LineKey>) {
        let violations: Vec<LineKey> = keys
            .into_iter()
            .filter(|k| {
                self.messages
                    .entry(*k)
                    .is_some_and(|e| e.violation.waiver.is_none())
            })
            .collect();
        if !violations.is_empty() {
            self.messages.set_waive_prompt(Some(WaivePrompt {
                violations,
                reason: String::new(),
            }));
        }
    }

    /// The reason typed in the Waive window.
    pub fn set_waive_reason(&mut self, reason: String) {
        if let Some(prompt) = self.messages.waive_prompt_mut() {
            prompt.reason = reason;
        }
    }

    /// Closes the Waive window, waiving its Violations with its reason when `confirm`.
    pub fn finish_waive(&mut self, confirm: bool) -> Result<(), CommandError> {
        let Some(prompt) = self.messages.waive_prompt().cloned() else {
            return Ok(());
        };
        self.messages.set_waive_prompt(None);
        if confirm {
            self.waive_many(&prompt.violations, prompt.reason.trim())?;
        }
        Ok(())
    }

    /// Waives a line's Violation and runs its check again.
    pub fn waive(&mut self, key: LineKey) -> Result<(), CommandError> {
        self.waive_many(&[key], "")
    }

    /// Waives a selection with one reason. A mixed ERC/DRC selection is one atomic
    /// batch; normal per-Document history gives each affected Document one undo entry.
    pub fn waive_many(&mut self, keys: &[LineKey], reason: &str) -> Result<(), CommandError> {
        let mut groups: BTreeMap<CheckKind, Vec<Value>> = BTreeMap::new();
        for key in keys {
            if let Some((kind, v)) = self.violation_of(*key) {
                groups
                    .entry(kind)
                    .or_default()
                    .push(json!({"check":v.check,"objects":v.objects}));
            }
        }
        let commands: Vec<Value> = groups
            .iter()
            .map(|(kind, violations)| {
                json!({
                    "id": format!("{}.waive", kind.id()),
                    "params": {"violations": violations, "reason": reason},
                })
            })
            .collect();
        match commands.as_slice() {
            [] => return Ok(()),
            [one] => {
                self.run_reporting(
                    one["id"].as_str().expect("Command id"),
                    one["params"].clone(),
                )?;
            }
            _ => {
                self.run_reporting("batch", json!({"commands":commands}))?;
            }
        }
        for kind in groups.keys() {
            self.run_check(*kind)?;
        }
        Ok(())
    }

    /// Deletes the Waiver waiving a line's Violation, or a stale Waiver's line's Waiver,
    /// and runs the check again.
    pub fn unwaive(&mut self, key: LineKey) -> Result<(), CommandError> {
        let (kind, waiver) = match key {
            LineKey::StaleWaiver(kind, _) => match self.messages.stale_waiver(key) {
                Some(w) => (kind, w.waiver),
                None => return Ok(()),
            },
            _ => match self.violation_of(key) {
                Some((
                    kind,
                    Violation {
                        waiver: Some(w), ..
                    },
                )) => (kind, w),
                _ => return Ok(()),
            },
        };
        self.run_reporting(
            &format!("{}.unwaive", kind.id()),
            json!({"waivers": [waiver]}),
        )?;
        self.run_check(kind)
    }

    /// Edits the reason of the Waiver waiving a line's Violation
    /// (`checks.waiver.set_reason`) and runs the check again.
    pub fn set_waiver_reason(&mut self, key: LineKey, reason: &str) -> Result<(), CommandError> {
        let Some((
            kind,
            Violation {
                waiver: Some(waiver),
                ..
            },
        )) = self.violation_of(key)
        else {
            return Ok(());
        };
        self.run_reporting(
            "checks.waiver.set_reason",
            json!({"waivers": [waiver], "reason": reason}),
        )?;
        self.run_check(kind)
    }

    /// Sets the severity of a line's check (`erc.set_severity`/`drc.set_severity`) and
    /// runs the check again. For a Pin Matrix check it sets the cell of the line's pair of
    /// types instead (`erc.set_pin_matrix`).
    pub fn set_severity(&mut self, key: LineKey, severity: Severity) -> Result<(), CommandError> {
        let Some((kind, v)) = self.violation_of(key) else {
            return Ok(());
        };
        match crate::messages::matrix_cell(&v).filter(|_| kind == CheckKind::Erc) {
            Some((a, b)) => self.run_reporting(
                "erc.set_pin_matrix",
                json!({"cells": [{"a": a, "b": b, "severity": severity}]}),
            )?,
            None => self.run_reporting(
                &format!("{}.set_severity", kind.id()),
                json!({"check": v.check, "severity": severity}),
            )?,
        };
        self.run_check(kind)
    }

    /// Highlights a line's Violation's objects (`view.highlight`), from Violation Details.
    pub fn highlight_violation(&mut self, key: LineKey) -> Result<(), CommandError> {
        let Some((_, v)) = self.violation_of(key) else {
            return Ok(());
        };
        let objects: Vec<Uuid> = match self.engine.project() {
            Some(p) => v
                .objects
                .into_iter()
                .filter(|o| p.object(*o).is_some())
                .collect(),
            None => Vec::new(),
        };
        self.run_reporting("view.highlight", json!({ "objects": objects }))
            .map(|_| ())
    }

    /// Centres a line's Violation's Document on its marker (`view.jump_to`), opening it
    /// first, without changing the selection or the zoom.
    pub fn jump_to_violation(&mut self, key: LineKey) -> Result<(), CommandError> {
        let Some(entry) = self.messages.entry(key) else {
            return Ok(());
        };
        let (Some(document), Some(at)) = (entry.document, entry.violation.at) else {
            return Ok(());
        };
        self.run_reporting("document.open", json!({ "document": document }))?;
        self.run_reporting(
            "view.jump_to",
            json!({ "document": document, "x": at.x.0, "y": at.y.0 }),
        )?;
        self.messages.set_current(Some(key));
        self.update_markers();
        Ok(())
    }

    pub(crate) fn violation_of(&self, key: LineKey) -> Option<(CheckKind, Violation)> {
        match key {
            LineKey::Violation(kind, _) => {
                Some((kind, self.messages.entry(key)?.violation.clone()))
            }
            LineKey::Note(_) | LineKey::StaleWaiver(..) => None,
        }
    }

    /// Replaces the load errors and warnings `source` reports (such as `keymap`), each
    /// about the whole application.
    pub fn set_load_messages(&mut self, source: &str, messages: Vec<(Severity, String)>) {
        let notes = messages
            .into_iter()
            .map(|(severity, text)| Note {
                source: source.to_owned(),
                document: None,
                severity,
                text,
            })
            .collect();
        self.messages.set_notes(source, notes);
    }

    /// Replaces the Messages panel lines `source` reports, such as an Output Job run's
    /// blockers and warnings or a Library check's Violations.
    pub fn set_notes(&mut self, source: &str, notes: Vec<Note>) {
        self.messages.set_notes(source, notes);
    }

    /// The Violation markers on `document`'s canvas; none when it is not open.
    pub fn markers(&self, document: Uuid) -> Arc<[Marker]> {
        self.document_tab(document)
            .map(|t| t.markers.clone())
            .unwrap_or_else(|| Arc::from([]))
    }

    /// How many of `document`'s Violations have no marker because of the
    /// `checks.max_markers` cap; 0 when it is not open.
    pub fn markers_not_shown(&self, document: Uuid) -> usize {
        self.document_tab(document)
            .map_or(0, |t| t.markers_not_shown)
    }

    /// Marks check results stale once the design differs from the run's.
    pub(super) fn mark_stale_runs(&mut self) {
        let Some(project) = self.engine.project() else {
            return;
        };
        let erc = reda_engine::schematic::erc::design_fingerprint(project);
        let drc = project.pcb().map(reda_engine::pcb::drc::design_fingerprint);
        if let Some(run) = self.messages.run_mut(CheckKind::Erc) {
            run.stale = run.design != erc;
        }
        if let Some(run) = self.messages.run_mut(CheckKind::Drc) {
            run.stale = drc.as_ref() != Some(&run.design);
        }
    }

    /// Recomputes every open tab's Violation markers, as the Checks Preferences say: a
    /// check with neither Details nor Overlay has none.
    pub(super) fn update_markers(&mut self) {
        let Some(project) = self.engine.project() else {
            return;
        };
        let current = self.messages.current();
        let checks = &self.preferences.values().checks;
        let cap = checks.max_markers as usize;
        let look = OverlayLook {
            style: checks.overlay_style,
            zoomed_out: checks.overlay_zoomed_out,
        };
        for tab in &mut self.documents {
            let mut markers: Vec<Marker> = self
                .messages
                .listed()
                .filter(|(_, e)| e.document == Some(tab.document))
                .filter_map(|(key, e)| {
                    let v = &e.violation;
                    let display = checks.display.get(&v.check);
                    if !display.details && !display.overlay {
                        return None;
                    }
                    let location = messages::locate(project, tab.document, &tab.scene.scene, v)?;
                    let stale = match key {
                        LineKey::Violation(kind, _) => {
                            self.messages.run(kind).is_some_and(|r| r.stale)
                        }
                        _ => false,
                    };
                    Some(Marker {
                        at: location.at,
                        severity: v.severity,
                        waived: v.waiver.is_some(),
                        stale,
                        current: current == Some(key),
                        key,
                        details: display.details,
                        value: messages::value_text(v),
                        points: v.points,
                        layers: v.layers.clone(),
                        overlay: display.overlay.then_some(look),
                        objects: v.objects.clone(),
                    })
                })
                .collect();
            tab.markers_not_shown = markers.len().saturating_sub(cap);
            if markers.len() > cap {
                // The Violation last gone to keeps its marker.
                let kept = markers.iter().position(|m| m.current);
                let current = kept.filter(|i| *i >= cap).map(|i| markers[i].clone());
                markers.truncate(cap);
                if let Some(current) = current
                    && let Some(last) = markers.last_mut()
                {
                    *last = current;
                }
            }
            tab.markers = Arc::from(markers);
        }
    }
}
