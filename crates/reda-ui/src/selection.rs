//! The selection: which objects the user has picked, kept as view state (never saved,
//! never undone). Panels such as Properties read it from [`Shell::selection`]; it changes
//! only through the view Command `selection.set`, so scripts and MCP select the same way
//! the canvas will.
//!
//! [`Shell::selection`]: crate::shell::Shell::selection

pub mod filter_ui;
pub mod picking;
pub mod rules;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{Project, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What is selected.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Selection {
    /// Selected objects, by UUID, in the order they were picked.
    pub objects: Vec<Uuid>,
    /// Selected Pads of Footprint instances, which have no UUID of their own.
    pub pads: Vec<PadRef>,
    /// Independently selected Component fields, identified by Unit and field name.
    pub fields: Vec<FieldRef>,
    /// Independently selected text entries of placed Footprints.
    pub texts: Vec<TextRef>,
    /// Highlighted PCB Nets.
    pub nets: Vec<Uuid>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
            && self.pads.is_empty()
            && self.fields.is_empty()
            && self.texts.is_empty()
            && self.nets.is_empty()
    }

    /// How many things are selected: objects, Pads and Nets.
    pub fn len(&self) -> usize {
        self.objects.len()
            + self.pads.len()
            + self.fields.len()
            + self.texts.len()
            + self.nets.len()
    }

    /// GPU identities include children without selecting their parents.
    pub fn canvas_ids(&self) -> Vec<Uuid> {
        let mut ids = self.objects.clone();
        ids.extend(self.fields.iter().map(|r| {
            reda_engine::render::Child::Field {
                unit: r.unit,
                field: r.field.clone(),
            }
            .id()
        }));
        ids.extend(self.texts.iter().map(|r| {
            reda_engine::render::Child::FootprintText {
                footprint: r.footprint,
                text: r.text,
            }
            .id()
        }));
        ids.extend(self.pads.iter().map(|r| {
            reda_engine::render::Child::Pad {
                footprint: r.footprint,
                pad: r.pad,
            }
            .id()
        }));
        ids
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldRef {
    pub unit: Uuid,
    pub field: String,
}

pub use reda_engine::model::{FootprintTextRef as TextRef, PadRef};

/// Adds `selection.set` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(SelectionSet);
    registry.add(picking::Cycle);
    registry.add(Delete);
    registry.add(SelectSimilar);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectSimilarParams {
    /// The selected objects; the first is the template.
    objects: Vec<Uuid>,
}

/// `selection.similar`: selects everything of the same kind as the first object, where it
/// lives (its sheet, the board or its Library element), through `objects.find_similar`.
struct SelectSimilar;

impl Command for SelectSimilar {
    const META: Meta = Meta {
        id: "selection.similar",
        title: "Select Similar",
        description: "Selects every object of the same kind as the first one given, on its \
                      sheet, on the board or in its Library element.",
        category: "selection",
        icon: "select_similar",
        kind: Kind::View,
    };
    type Params = SelectSimilarParams;
    type Output = SelectionSetResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SelectSimilarParams,
    ) -> Result<SelectionSetResult, CommandError> {
        use reda_engine::commands::similar::{
            FindSimilarParams, SimilarScope, Template, find_similar,
        };
        let template = *p
            .objects
            .first()
            .ok_or_else(|| CommandError::invalid_params("/objects", "give an object"))?;
        let project = cx.project()?;
        let found = [
            SimilarScope::Sheet,
            SimilarScope::Board,
            SimilarScope::Element,
        ]
        .into_iter()
        .find_map(|scope| {
            find_similar(
                project,
                &FindSimilarParams {
                    template: Template::Object(template),
                    rules: Default::default(),
                    scope,
                },
            )
            .ok()
        })
        .ok_or_else(|| CommandError::not_found(format!("no object {template}")))?;
        let selection = Selection {
            objects: found.objects,
            pads: found.pads,
            ..Selection::default()
        };
        let count = selection.len();
        *cx.view::<Selection>()? = selection;
        Ok(SelectionSetResult { count })
    }
}

/// Standalone field text is moved or hidden through its own Properties; Delete refuses
/// to remove it from its Component or Footprint.
struct Delete;

impl Command for Delete {
    const META: Meta = Meta {
        id: "selection.delete",
        title: "Delete Selection",
        description: "Refuses deletion of independently selected Field and Designator text.",
        category: "selection",
        icon: "delete",
        kind: Kind::View,
    };
    type Params = reda_engine::command::NoParams;
    type Output = ();
    fn run(&self, cx: &mut Context<'_>, _: Self::Params) -> Result<(), CommandError> {
        let selection = cx.view::<Selection>()?;
        if !selection.fields.is_empty() || !selection.texts.is_empty() {
            Err(CommandError::rejected(
                "Field and Designator text cannot be deleted alone; hide it in Properties instead",
            ))
        } else {
            Err(CommandError::unavailable("select an object to delete"))
        }
    }
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
    #[serde(default)]
    fields: Vec<FieldRef>,
    #[serde(default)]
    texts: Vec<TextRef>,
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
            fields: dedup(p.fields),
            texts: dedup(p.texts),
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
    for field in &selection.fields {
        let unit = project
            .documents()
            .filter_map(|d| project.schematic_sheet(d.uuid))
            .find_map(|s| s.unit(field.unit));
        if !unit.is_some_and(|u| {
            field.field == reda_engine::model::UnitInstance::DESIGNATOR
                || reda_engine::model::Fields::BUILT_IN.contains(&field.field.as_str())
                || u.fields.custom.iter().any(|f| f.name == field.field)
        }) {
            return Err(CommandError::not_found(format!(
                "no field {:?} on Unit {}",
                field.field, field.unit
            )));
        }
    }
    for text in &selection.texts {
        let found = pcb
            .and_then(|p| p.footprint(text.footprint))
            .and_then(|f| pcb.and_then(|p| p.footprint_copy(f.footprint)))
            .is_some_and(|f| f.footprint.texts.iter().any(|t| t.uuid == text.text));
        if !found {
            return Err(CommandError::not_found(format!(
                "no text {} on Footprint {}",
                text.text, text.footprint
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
