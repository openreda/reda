//! The selection: which objects the user has picked, kept as view state (never saved,
//! never undone). Panels such as Properties read it from [`Shell::selection`]; it changes
//! only through the view Command `selection.set`, so scripts and MCP select the same way
//! the canvas will.
//!
//! [`Shell::selection`]: crate::shell::Shell::selection

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{Project, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What is selected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// Selected objects, by UUID, in the order they were picked.
    pub objects: Vec<Uuid>,
    /// Selected Pads of Footprint instances, which have no UUID of their own.
    pub pads: Vec<PadRef>,
    /// Highlighted PCB Nets.
    pub nets: Vec<Uuid>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.pads.is_empty() && self.nets.is_empty()
    }

    /// How many things are selected: objects, Pads and Nets.
    pub fn len(&self) -> usize {
        self.objects.len() + self.pads.len() + self.nets.len()
    }
}

pub use reda_engine::model::PadRef;

/// Adds `selection.set` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(SelectionSet);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectionSetParams {
    /// The objects to select, by UUID; none when left out.
    #[serde(default)]
    objects: Vec<Uuid>,
    /// Pads of Footprint instances to select.
    #[serde(default)]
    pads: Vec<PadRef>,
    /// PCB Nets to highlight, by UUID.
    #[serde(default)]
    nets: Vec<Uuid>,
}

/// How many things are now selected: what `selection.set` and the Commands that add to
/// the selection return.
#[derive(Serialize, JsonSchema)]
pub struct SelectionSetResult {
    pub count: usize,
}

struct SelectionSet;

impl Command for SelectionSet {
    const META: Meta = Meta {
        id: "selection.set",
        title: "Select",
        description: "Replaces the selection with the given objects, Footprint Pads and \
                      highlighted Nets. With no params it clears the selection.",
        category: "selection",
        icon: "select",
        kind: Kind::View,
    };
    type Params = SelectionSetParams;
    type Output = SelectionSetResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SelectionSetParams,
    ) -> Result<SelectionSetResult, CommandError> {
        let selection = Selection {
            objects: dedup(p.objects),
            pads: dedup(p.pads),
            nets: dedup(p.nets),
        };
        if !selection.is_empty() {
            let project = cx.project()?;
            check(project, &selection)?;
        }
        let count = selection.len();
        *cx.view::<Selection>()? = selection;
        Ok(SelectionSetResult { count })
    }
}

fn dedup<T: PartialEq>(items: Vec<T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

/// Refuses anything that is not in the Project or a part of an open Library element.
fn check(project: &Project, selection: &Selection) -> Result<(), CommandError> {
    for uuid in &selection.objects {
        if project.object(*uuid).is_none() && !crate::element_editor::part_of(project, *uuid) {
            return Err(CommandError::not_found(format!("no object {uuid}")));
        }
    }
    let pcb = project.pcb();
    for pad in &selection.pads {
        let found = pcb
            .and_then(|pcb| pcb.footprint(pad.footprint))
            .is_some_and(|f| f.pads.iter().any(|p| p.pad == pad.pad));
        if !found {
            return Err(CommandError::not_found(format!(
                "no Pad {} on Footprint {}",
                pad.pad, pad.footprint
            )));
        }
    }
    for net in &selection.nets {
        if pcb.and_then(|pcb| pcb.net(*net)).is_none() {
            return Err(CommandError::not_found(format!("no PCB Net {net}")));
        }
    }
    Ok(())
}
