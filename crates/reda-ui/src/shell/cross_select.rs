//! The Shell's side of cross-select (docs/spec/v0.1/sync.md#cross-select-and-net-highlighting):
//! keeping the Selection's mirrored objects in step with the picks, and, when a pick
//! brings new ones, following the cross-select Preferences: Zoom the other side's
//! canvases to them, Focus the other Document when it is visible, and Reposition (a
//! picked Unit's Footprint floats on the PCB cursor when the PCB canvas is visible).
//! Dim is read where the marks are worked out ([`crate::cross_probe::marks`]).

use reda_engine::model::{Object, Scope, Uuid};

use super::Shell;
use crate::cross_probe;
use crate::layout::TabKey;
use crate::selection::Selection;

impl Shell {
    /// After `view.cross_select.reposition.toggle`: flips the Reposition Preference.
    pub(super) fn after_cross_select_run(&mut self, id: &str) {
        if id == "view.cross_select.reposition.toggle" {
            let on = !self.preferences().cross_select.reposition;
            let _ = self.run_reporting(
                "app.preferences.set",
                serde_json::json!({"key": "cross_select.reposition", "value": on}),
            );
        }
    }

    /// Recomputes `Selection.mirrored` from the picks (nothing while cross-select is
    /// off); when it changed and holds something, follows the Preferences.
    pub(super) fn mirror_selection(&mut self) {
        let on = self.cross_probe_switches().selection;
        let Some(project) = self.engine.project() else {
            return;
        };
        let selection = self.selection();
        let mirrored = if on {
            cross_probe::mirror(project, &selection.objects)
        } else {
            Vec::new()
        };
        if mirrored == selection.mirrored {
            return;
        }
        let units: Vec<Uuid> = (selection.objects.iter())
            .filter(|o| matches!(project.object(**o), Some(Object::Unit(_))))
            .copied()
            .collect();
        let mut targets: Vec<Uuid> = Vec::new();
        for o in &mirrored {
            if let Some(Scope::Document(d)) = project.object(*o).map(|o| o.scope())
                && !targets.contains(&d)
            {
                targets.push(d);
            }
        }
        let pcb = project.pcb().map(|p| p.uuid);
        let queue = reposition_queue(project, &units);
        if let Some(s) = self.engine.view_mut().get_mut::<Selection>() {
            s.mirrored = mirrored.clone();
        }
        if mirrored.is_empty() {
            return;
        }
        let prefs = self.preferences().cross_select.clone();
        if prefs.zoom {
            for document in &targets {
                self.zoom_to_objects(*document, &mirrored);
            }
        }
        let shown = |shell: &Shell, d: Uuid| shell.layout().is_shown(&TabKey::Document(d));
        if prefs.focus {
            let active = self.layout().active_document();
            if !active.is_some_and(|a| targets.contains(&a))
                && let Some(document) = targets.iter().copied().find(|d| shown(self, *d))
            {
                let _ = self.layout_mut().activate(&TabKey::Document(document));
            }
        }
        if prefs.reposition && !queue.is_empty() && pcb.is_some_and(|pcb| shown(self, pcb)) {
            self.with_pcb_tool(|tool, shell| tool.start_queue(shell, queue));
        }
    }
}

/// The unlocked Footprints of `units`, in pick order, for Reposition.
fn reposition_queue(project: &reda_engine::model::Project, units: &[Uuid]) -> Vec<Uuid> {
    if units.is_empty() {
        return Vec::new();
    }
    let selection = Selection {
        objects: units.to_vec(),
        ..Selection::default()
    };
    crate::pcb::placing::queue_of(project, &selection)
}
