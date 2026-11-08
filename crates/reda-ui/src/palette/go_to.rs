//! Choosing a Document, Component or Net row: going to it.

use reda_engine::model::{ComponentRef, DocumentKind, Uuid};
use serde_json::json;

use super::Target;
use crate::shell::Shell;

/// Goes to `target`: opens a Document, selects and zooms to a Component, highlights a
/// Net. A Command row is the window's to run (it may need a form), so it does nothing
/// here, nor does a prefix row.
pub fn go_to(shell: &mut Shell, target: &Target) {
    match target {
        Target::Document(document) => {
            let _ = shell.run_reporting("document.open", json!({ "document": document }));
        }
        Target::Net(name) => {
            let _ = shell.run_reporting("view.highlight", json!({ "net": name }));
        }
        Target::Component(component) => {
            if let Some((document, objects)) = component_place(shell, component) {
                shell.reveal(document, &objects);
            }
        }
        Target::Run(_) | Target::Prefix(_) => {}
    }
}

/// Where to show `component`: its Footprint when the active Document is a board that has
/// it, else its Units on the active sheet if they are there, else on the first sheet
/// that has any.
fn component_place(shell: &Shell, component: &ComponentRef) -> Option<(Uuid, Vec<Uuid>)> {
    let project = shell.engine().project()?;
    let active = shell.layout().active_document();
    let on = |document: Uuid| -> Vec<Uuid> {
        match project.document(document).map(|d| d.kind) {
            Some(DocumentKind::Pcb) => (project.pcb_document(document).into_iter())
                .flat_map(|pcb| pcb.footprints())
                .filter(|f| f.component.as_ref() == Some(component))
                .map(|f| f.uuid)
                .collect(),
            Some(DocumentKind::Schematic) => (project.schematic_sheet(document).into_iter())
                .flat_map(|sheet| sheet.units())
                .filter(|u| &u.component == component)
                .map(|u| u.uuid)
                .collect(),
            None => Vec::new(),
        }
    };
    if let Some(active) = active {
        let objects = on(active);
        if !objects.is_empty() {
            return Some((active, objects));
        }
    }
    project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .map(|d| (d.uuid, on(d.uuid)))
        .find(|(_, objects)| !objects.is_empty())
}
