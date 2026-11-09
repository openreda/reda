//! `library.footprint.regenerate` and its preview: running a generated Footprint's
//! stored `generator` params again, or new params for the same generator, on the open
//! Footprint in place.
//!
//! The Footprint keeps its UUID and name, Pads keep their UUIDs by number, and graphics
//! and texts keep theirs in order by Layer (texts by field and Layer), so regenerating
//! to the same geometry gives back the same Footprint and the same hash. Everything else
//! is replaced, hand edits included; the preview lists what they lose.

use std::collections::HashMap;

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{Footprint, FootprintGraphic, FootprintText, LibraryElement, Pad, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{GeneratorWarning, rebuild};
use crate::edit::edit_footprint;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegenerateParams {
    /// The open, generated Footprint.
    pub footprint: Uuid,
    /// New params for the Footprint's generator, in place of the stored ones, as that
    /// generator Command takes them. `library` may be left out, and `name` is ignored:
    /// the Footprint keeps its name.
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// What regenerating changes.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct Regenerated {
    pub footprint: Uuid,
    /// What the Footprint has now that regenerating removes or changes: hand edits, or
    /// what the new params change.
    pub lost: Vec<Lost>,
    /// What regenerating adds.
    pub added: Vec<Lost>,
    /// Whether the regenerated Footprint is the one there now.
    pub unchanged: bool,
    /// The generator's warnings.
    pub warnings: Vec<GeneratorWarning>,
}

/// One part of the Footprint that regenerating removes, changes or adds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Lost {
    /// `pad`, `graphic`, `text` or `footprint` (one of its own properties).
    pub kind: String,
    /// The Pad, graphic or text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<Uuid>,
    /// What it is, such as `Pad 3`, `Line on silkscreen_top`, `Designator on
    /// assembly_top` or `description`.
    pub what: String,
    pub change: Change,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Removed,
    Changed,
    Added,
}

pub struct Regenerate;

impl Command for Regenerate {
    const META: Meta = Meta {
        id: "library.footprint.regenerate",
        title: "Regenerate Footprint",
        description: "Runs an open generated Footprint's generator again, with its stored \
                      params or new `params`, in place: the Footprint keeps its UUID and \
                      name and its Pads keep their UUIDs by number, and everything else is \
                      replaced, hand edits included. `library.footprint.regenerate.preview` \
                      lists what is lost first. One undo step.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::Edit,
    };
    type Params = RegenerateParams;
    type Output = Regenerated;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: RegenerateParams) -> Result<Regenerated, CommandError> {
        let (footprint, out) = regenerated(cx, &p)?;
        edit_footprint(cx, p.footprint, |f| {
            *f = footprint;
            Ok(())
        })?;
        Ok(out)
    }
}

pub struct RegeneratePreview;

impl Command for RegeneratePreview {
    const META: Meta = Meta {
        id: "library.footprint.regenerate.preview",
        title: "Preview Regenerate Footprint",
        description: "What `library.footprint.regenerate` would do with the same params, \
                      changing nothing: what the open Footprint loses (hand edits, or what \
                      new params change), what it gains, and the generator's warnings.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::Query,
    };
    type Params = RegenerateParams;
    type Output = Regenerated;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: RegenerateParams) -> Result<Regenerated, CommandError> {
        Ok(regenerated(cx, &p)?.1)
    }
}

/// The regenerated Footprint, and how it differs from the open one.
fn regenerated(
    cx: &mut Context<'_>,
    p: &RegenerateParams,
) -> Result<(Footprint, Regenerated), CommandError> {
    let (old, library) = {
        let project = cx.project()?;
        let not_open = || {
            CommandError::not_found(format!(
                "no open Footprint has UUID {}; open it with library.element.open",
                p.footprint
            ))
        };
        let (Some(source), Some(LibraryElement::Footprint(old))) = (
            project.element_source(p.footprint),
            project.element(p.footprint),
        ) else {
            return Err(not_open());
        };
        (old.clone(), source.library)
    };
    regenerate_footprint(&old, library, p.params.as_ref(), &mut || cx.new_id())
}

/// `old` regenerated with `params` (its stored params when `None`), as the Footprint
/// regenerating gives and how it differs from `old`, without changing anything: the
/// preview a regenerate form shows. `library` is the one `old` is in, used when `params`
/// leave it out; `new_id` gives the UUIDs of what is new.
pub fn regenerate_footprint(
    old: &Footprint,
    library: Uuid,
    params: Option<&serde_json::Value>,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Result<(Footprint, Regenerated), CommandError> {
    let Some(generator) = &old.generator else {
        return Err(CommandError::rejected(format!(
            "{} was not made by a generator, so it has no params to run again",
            old.name
        )));
    };
    let params_given = params.is_some();
    let mut params = params.cloned().unwrap_or_else(|| generator.params.clone());
    let Some(fields) = params.as_object_mut() else {
        return Err(CommandError::invalid_params(
            "/params",
            "the params are an object, as the generator takes them",
        ));
    };
    fields
        .entry("library")
        .or_insert_with(|| serde_json::json!(library));
    fields.insert("name".into(), serde_json::json!(old.name));
    let mut next = 0u128;
    let (mut new, warnings) = rebuild(&generator.command, &params, &mut || {
        next += 1;
        Uuid::from_u128(next)
    })
    .ok_or_else(|| {
        CommandError::rejected(format!(
            "{} records the generator {}, which this version of REDA does not have",
            old.name, generator.command
        ))
    })?
    .map_err(|e| under_params(e, params_given))?;
    keep_uuids(old, &mut new, new_id);
    let (lost, added) = differences(old, &new);
    let unchanged = new == *old;
    let out = Regenerated {
        footprint: old.uuid,
        lost,
        added,
        unchanged,
        warnings,
    };
    Ok((new, out))
}

/// Whether `footprint`'s stored generator params still make it: the same Pads, graphics,
/// texts and properties, whatever their UUIDs. `None` when no generator made it,
/// `Some(false)` when the params now make something else, fail, or name a generator REDA
/// lacks. The name is the Footprint's own (as regenerating keeps it) and the stored
/// record itself is not compared, so a renamed Footprint still matches.
pub fn still_made(footprint: &Footprint) -> Option<bool> {
    let generator = footprint.generator.as_ref()?;
    let mut params = generator.params.clone();
    let Some(fields) = params.as_object_mut() else {
        return Some(false);
    };
    fields
        .entry("library")
        .or_insert_with(|| serde_json::json!(Uuid::NIL));
    fields.insert("name".into(), serde_json::json!(footprint.name));
    let mut next = 0u128;
    let mut new_id = || {
        next += 1;
        Uuid::from_u128(next)
    };
    let Some(Ok((new, _))) = rebuild(&generator.command, &params, &mut new_id) else {
        return Some(false);
    };
    Some(without_uuids(&new) == without_uuids(footprint))
}

/// A Footprint's contents with every UUID cleared and each list in a fixed order, so two
/// Footprints that differ only in UUIDs compare equal.
fn without_uuids(footprint: &Footprint) -> (Vec<String>, Vec<String>, Vec<String>, String) {
    fn canonical<T: Clone + Serialize>(items: &[T], clear: impl Fn(&mut T)) -> Vec<String> {
        let mut out: Vec<String> = items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                clear(&mut item);
                serde_json::to_string(&item).unwrap_or_default()
            })
            .collect();
        out.sort();
        out
    }
    let properties = serde_json::json!({
        "description": footprint.description,
        "height": footprint.height,
    });
    (
        canonical(&footprint.pads, |p| p.uuid = Uuid::NIL),
        canonical(&footprint.graphics, |g| g.uuid = Uuid::NIL),
        canonical(&footprint.texts, |t| t.uuid = Uuid::NIL),
        properties.to_string(),
    )
}

/// A generator's `invalid_params` error, with its pointer under `/params` when the
/// caller gave the params.
fn under_params(mut e: CommandError, given: bool) -> CommandError {
    if !given {
        return e;
    }
    if let Some(details) = e.details.as_mut()
        && let Some(pointer) = details.get("pointer").and_then(|p| p.as_str())
    {
        details["pointer"] = serde_json::json!(format!("/params{pointer}"));
    }
    e
}

/// Gives `new` the UUIDs of `old`: its own, its Pads' by number, its graphics' in order
/// on each Layer and its texts' in order by field and Layer; anything without a match
/// takes one from `new_id`.
fn keep_uuids(old: &Footprint, new: &mut Footprint, new_id: &mut dyn FnMut() -> Uuid) {
    new.uuid = old.uuid;
    let mut pads: HashMap<&str, Uuid> = old
        .pads
        .iter()
        .map(|pad| (pad.number.as_str(), pad.uuid))
        .collect();
    for pad in &mut new.pads {
        pad.uuid = pads
            .remove(pad.number.as_str())
            .unwrap_or_else(&mut *new_id);
    }
    // `new`'s graphics and texts are in the order the generator made them, which their
    // UUIDs (counting up) keep.
    let mut graphics = queues(&old.graphics, |g| layer(g.layer), |g| g.uuid);
    for graphic in &mut new.graphics {
        let key = layer(graphic.layer);
        graphic.uuid = next(&mut graphics, &key).unwrap_or_else(&mut *new_id);
    }
    let text_key = |t: &FootprintText| format!("{} {}", t.field, layer(t.layer));
    let mut texts = queues(&old.texts, text_key, |t| t.uuid);
    for text in &mut new.texts {
        let key = text_key(text);
        text.uuid = next(&mut texts, &key).unwrap_or_else(&mut *new_id);
    }
    new.pads.sort_by_key(|p| p.uuid);
    new.graphics.sort_by_key(|g| g.uuid);
    new.texts.sort_by_key(|t| t.uuid);
}

/// `items`' UUIDs, in order, grouped by `key`.
fn queues<T>(
    items: &[T],
    key: impl Fn(&T) -> String,
    uuid: impl Fn(&T) -> Uuid,
) -> HashMap<String, Vec<Uuid>> {
    let mut queues: HashMap<String, Vec<Uuid>> = HashMap::new();
    for item in items {
        queues.entry(key(item)).or_default().push(uuid(item));
    }
    for queue in queues.values_mut() {
        queue.reverse();
    }
    queues
}

fn next(queues: &mut HashMap<String, Vec<Uuid>>, key: &str) -> Option<Uuid> {
    queues.get_mut(key)?.pop()
}

/// A Layer's name, such as `silkscreen_top`.
fn layer(layer: reda_model::FootprintLayer) -> String {
    serde_json::to_value(layer)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// What `old` has that `new` removes or changes, and what `new` adds.
fn differences(old: &Footprint, new: &Footprint) -> (Vec<Lost>, Vec<Lost>) {
    let mut lost = vec![];
    let mut added = vec![];
    let pad_what = |p: &Pad| format!("Pad {}", p.number);
    let graphic_what = |g: &FootprintGraphic| {
        let kind = serde_json::to_value(&g.shape)
            .ok()
            .and_then(|v| v["kind"].as_str().map(str::to_owned))
            .unwrap_or_default();
        let mut kind = kind.replace('_', " ");
        if let Some(first) = kind.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        format!("{kind} on {}", layer(g.layer))
    };
    let text_what = |t: &FootprintText| format!("{} on {}", t.field, layer(t.layer));
    compare(
        "pad",
        &old.pads,
        &new.pads,
        |p| p.uuid,
        pad_what,
        &mut lost,
        &mut added,
    );
    compare(
        "graphic",
        &old.graphics,
        &new.graphics,
        |g| g.uuid,
        graphic_what,
        &mut lost,
        &mut added,
    );
    compare(
        "text",
        &old.texts,
        &new.texts,
        |t| t.uuid,
        text_what,
        &mut lost,
        &mut added,
    );
    let property = |what: &str| Lost {
        kind: "footprint".into(),
        uuid: None,
        what: what.into(),
        change: Change::Changed,
    };
    if old.description != new.description {
        lost.push(property("description"));
    }
    if old.height != new.height {
        lost.push(property("height"));
    }
    (lost, added)
}

fn compare<T: PartialEq>(
    kind: &str,
    old: &[T],
    new: &[T],
    uuid: impl Fn(&T) -> Uuid,
    what: impl Fn(&T) -> String,
    lost: &mut Vec<Lost>,
    added: &mut Vec<Lost>,
) {
    let entry = |item: &T, change| Lost {
        kind: kind.into(),
        uuid: Some(uuid(item)),
        what: what(item),
        change,
    };
    for item in old {
        match new.iter().find(|n| uuid(n) == uuid(item)) {
            None => lost.push(entry(item, Change::Removed)),
            Some(n) if n != item => lost.push(entry(item, Change::Changed)),
            Some(_) => {}
        }
    }
    for item in new {
        if !old.iter().any(|o| uuid(o) == uuid(item)) {
            added.push(entry(item, Change::Added));
        }
    }
}
