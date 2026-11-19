//! Shared Teardrop permission for routed copper and individual placed Pads.
use egui::Ui;
use reda_engine::model::{PadKind, Project};
use serde_json::{Value, json};

use super::Out;
use crate::properties::Shared;
use crate::selection::Selection;
use crate::{tr, widgets};

pub(super) fn show(ui: &mut Ui, project: &Project, selection: &Selection) -> Option<Out> {
    let pcb = project.pcb()?;
    let tracks = selection.objects.iter().filter_map(|id| pcb.track(*id));
    let vias = selection.objects.iter().filter_map(|id| pcb.via(*id));
    let free = selection
        .objects
        .iter()
        .filter_map(|id| pcb.free_pad(*id))
        .filter(|pad| pad.kind != PadKind::NonPlated);
    let pads = selection.pads.iter().filter_map(|reference| {
        let pad = pcb.footprint_pad(*reference)?;
        (pad.kind != PadKind::NonPlated).then_some((reference, pad))
    });
    let current = Shared::of(
        tracks
            .clone()
            .map(|t| t.teardrop)
            .chain(vias.clone().map(|v| v.teardrop))
            .chain(free.clone().map(|p| p.teardrop))
            .chain(pads.clone().map(|(_, p)| p.teardrop)),
    )?;
    let mixed = current == Shared::Mixed;
    let mut allow = current.same().copied().unwrap_or(false);
    if !ui
        .add(widgets::checkbox(&mut allow, tr!("properties-allow-teardrops")).indeterminate(mixed))
        .clicked()
    {
        return None;
    }

    let mut steps = Vec::new();
    let tracks: Vec<_> = tracks
        .filter(|t| t.teardrop != allow)
        .map(|t| t.uuid)
        .collect();
    if !tracks.is_empty() {
        steps.push(("pcb.track.set", json!({"tracks":tracks,"teardrop":allow})));
    }
    let vias: Vec<_> = vias
        .filter(|v| v.teardrop != allow)
        .map(|v| v.uuid)
        .collect();
    if !vias.is_empty() {
        steps.push(("pcb.via.set", json!({"vias":vias,"teardrop":allow})));
    }
    for p in free.filter(|p| p.teardrop != allow) {
        steps.push(("pcb.free_pad.edit", json!({"pad":p.uuid,"teardrop":allow})));
    }
    for (reference, _) in pads.filter(|(_, p)| p.teardrop != allow) {
        steps.push((
            "pcb.pad.edit",
            json!({"instance":reference.footprint,"pad":reference.pad,"teardrop":allow}),
        ));
    }
    if !mixed && steps.len() == 1 {
        let (id, params) = steps.pop()?;
        return Some(Out::Run(id, params));
    }
    let commands: Vec<Value> = steps
        .into_iter()
        .map(|(id, params)| json!({"id":id,"params":params}))
        .collect();
    Some(Out::Run("batch", json!({"commands":commands})))
}
