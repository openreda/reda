//! Shared elements: the first change to an open Symbol or Footprint that two or more
//! Library Parts use stops and asks, in a modal listing those Parts, whether to Edit
//! Shared (change it for all of them), Make a Copy (a new UUID and a `_1` name, which the
//! change goes to instead) or Cancel. Opening an element to look at it asks nothing.
//!
//! The window passes every Command it runs for the user through [`hold`]; a held change
//! waits in the view state ([`SharedElements::ask`]) until [`answer`] runs it, sends it to
//! the copy, or drops it. An element opened from a Part editor's Edit button remembers
//! that Part ([`opened_from_part`]), which Make a Copy repoints to the copy.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Id, Role, WidgetInfo};
use reda_engine::command::{CommandError, Kind};
use reda_engine::model::{LibraryElement, Uuid};
use serde_json::{Value, json};

use crate::element_editor::Invocation;
use crate::shell::Shell;
use crate::tr;

/// The shared-element prompt's view state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SharedElements {
    /// Elements whose change was let through: Edit Shared was answered, or none is shared.
    pub approved: BTreeSet<Uuid>,
    /// The Part whose editor opened each element.
    pub opened_from: BTreeMap<Uuid, Uuid>,
    /// The question being asked, if any.
    pub ask: Option<SharedAsk>,
}

/// One question: the element, the Parts using it, and the changes waiting on the answer.
#[derive(Clone, Debug, PartialEq)]
pub struct SharedAsk {
    pub element: Uuid,
    pub name: String,
    /// The names of the Parts that use it.
    pub parts: Vec<String>,
    pub pending: Vec<Invocation>,
}

/// The answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    EditShared,
    MakeCopy,
    Cancel,
}

fn state(shell: &Shell) -> SharedElements {
    (shell.engine().view().get::<SharedElements>())
        .cloned()
        .unwrap_or_default()
}

fn state_mut(shell: &mut Shell) -> &mut SharedElements {
    let view = shell.engine.view_mut();
    if view.get::<SharedElements>().is_none() {
        view.set(SharedElements::default());
    }
    view.get_mut::<SharedElements>().expect("just set")
}

/// The question being asked, if any.
pub fn asking(shell: &Shell) -> Option<SharedAsk> {
    state(shell).ask
}

/// Notes that the Part editor of `part` opened `element` (its Edit button).
pub fn opened_from_part(shell: &mut Shell, element: Uuid, part: Uuid) {
    state_mut(shell).opened_from.insert(element, part);
}

/// The open, editable Symbol or Footprint that Command `id` with `params` changes, if any.
pub fn changed_element(shell: &Shell, id: &str, params: &Value) -> Option<Uuid> {
    let base = id.split(':').next().unwrap_or(id);
    if base == "batch" {
        let first = params["commands"].as_array()?.first()?;
        return changed_element(shell, first["id"].as_str()?, &first["params"]);
    }
    let uuid = |v: &Value| v.as_str().and_then(|s| s.parse::<Uuid>().ok());
    let element = if base == "library.symbol.generate" {
        uuid(&params["target"]["symbol"])?
    } else {
        let kind = shell.engine().registry().get(base).map(|m| m.kind);
        if kind != Some(Kind::Edit) {
            return None;
        }
        uuid(&params["symbol"]).or_else(|| uuid(&params["footprint"]))?
    };
    let engine = shell.engine();
    let editable = matches!(
        engine.element(element),
        Some(LibraryElement::Symbol(_) | LibraryElement::Footprint(_))
    ) && engine.element_source(element).is_some_and(|s| !s.read_only);
    editable.then_some(element)
}

/// Whether the window may run Command `id` with `params` now. The first change to a
/// shared element waits instead, and the question opens; while it is open, more changes
/// to that element wait with the first and changes to other elements are dropped.
pub fn hold(shell: &mut Shell, id: &str, params: &Value) -> bool {
    let element = changed_element(shell, id, params);
    if let Some(ask) = &mut state_mut(shell).ask {
        if element.is_none() {
            return true;
        }
        if element == Some(ask.element) {
            ask.pending.push((id.to_owned(), params.clone()));
        }
        return false;
    }
    let Some(element) = element else {
        return true;
    };
    if state(shell).approved.contains(&element) {
        return true;
    }
    let index = shell.library_index();
    let parts: Vec<String> = (index.parts_using(element).iter())
        .map(|hit| format!("{} / {}", hit.library.info.name, hit.part.name))
        .collect();
    if parts.len() < 2 {
        state_mut(shell).approved.insert(element);
        return true;
    }
    let name = (shell.engine().element(element))
        .map(|e| e.name().to_owned())
        .unwrap_or_default();
    state_mut(shell).ask = Some(SharedAsk {
        element,
        name,
        parts,
        pending: vec![(id.to_owned(), params.clone())],
    });
    false
}

/// Carries out `answer` to the open question: Edit Shared runs the waiting changes,
/// Make a Copy copies the element beside itself, opens the copy in its place, repoints
/// the Part whose editor opened it and runs the changes on the copy, and Cancel drops
/// them. Returns the copy's UUID after Make a Copy.
pub fn answer(shell: &mut Shell, answer: Answer) -> Result<Option<Uuid>, CommandError> {
    let Some(ask) = state_mut(shell).ask.take() else {
        return Ok(None);
    };
    match answer {
        Answer::Cancel => Ok(None),
        Answer::EditShared => {
            state_mut(shell).approved.insert(ask.element);
            for (id, params) in ask.pending {
                shell.run_reporting(&id, params)?;
            }
            Ok(None)
        }
        Answer::MakeCopy => {
            let library = (shell.engine().element_source(ask.element))
                .map(|s| s.library)
                .ok_or_else(|| CommandError::not_found("the element is no longer open"))?;
            let is_symbol = matches!(
                shell.engine().element(ask.element),
                Some(LibraryElement::Symbol(_))
            );
            let out = shell.run_reporting(
                "library.element.copy",
                json!({"element": ask.element, "library": library}),
            )?;
            let copy: Uuid = (out["element"].as_str())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| CommandError::internal("the copy has no UUID"))?;
            let part = state(shell).opened_from.get(&ask.element).copied();
            shell.run_reporting("library.element.close", json!({"element": ask.element}))?;
            shell.run_reporting("library.element.open", json!({"element": copy}))?;
            state_mut(shell).approved.insert(copy);
            if let Some(part) = part {
                repoint(shell, part, ask.element, copy, is_symbol)?;
                state_mut(shell).opened_from.insert(copy, part);
            }
            let (from, to) = (ask.element.to_string(), copy.to_string());
            for (id, params) in ask.pending {
                let params = swap_uuid(params, &from, &to);
                shell.run_reporting(&id, params)?;
            }
            Ok(Some(copy))
        }
    }
}

/// The Part editor's Duplicate & Edit (library.md#shared-elements): copies the Part's
/// Symbol or Footprint `element` (a new UUID and a `_1` name) into its own Library, or
/// the Part's when that one is read-only, points the Part at the copy and opens it,
/// without asking. Returns the copy.
pub fn duplicate_and_edit(
    shell: &mut Shell,
    part: Uuid,
    element: Uuid,
) -> Result<Uuid, CommandError> {
    let library = match crate::part_editor::library_of(shell, element) {
        Some((library, true)) => library,
        _ => match crate::part_editor::library_of(shell, part) {
            Some((library, true)) => library,
            _ => crate::library_dialogs::writable_libraries(shell)
                .first()
                .map(|(l, _)| *l)
                .ok_or_else(|| CommandError::rejected("no Library can be written to"))?,
        },
    };
    let is_symbol = crate::part_editor::symbol(shell, element).is_some();
    let out = shell.run_reporting(
        "library.element.copy",
        json!({"element": element, "library": library}),
    )?;
    let copy: Uuid = (out["element"].as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| CommandError::internal("the copy has no UUID"))?;
    repoint(shell, part, element, copy, is_symbol)?;
    state_mut(shell).approved.insert(copy);
    state_mut(shell).opened_from.insert(copy, part);
    shell.run_reporting("library.element.open", json!({"element": copy}))?;
    Ok(copy)
}

/// Points the open Part `part` at `copy` in place of `original`.
fn repoint(
    shell: &mut Shell,
    part: Uuid,
    original: Uuid,
    copy: Uuid,
    is_symbol: bool,
) -> Result<(), CommandError> {
    let Some(LibraryElement::Part(p)) = shell.engine().element(part).cloned() else {
        return Ok(());
    };
    if is_symbol {
        shell.run_reporting(
            "library.part.symbol.set",
            json!({"part": part, "symbol": copy}),
        )?;
    } else {
        let footprints: Vec<Uuid> = (p.footprints.iter())
            .map(|f| if *f == original { copy } else { *f })
            .collect();
        shell.run_reporting(
            "library.part.footprints.set",
            json!({"part": part, "footprints": footprints}),
        )?;
    }
    Ok(())
}

/// `value` with every string equal to `from` replaced by `to`.
fn swap_uuid(value: Value, from: &str, to: &str) -> Value {
    match value {
        Value::String(s) if s == from => Value::String(to.to_owned()),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|v| swap_uuid(v, from, to)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, swap_uuid(v, from, to)))
                .collect(),
        ),
        other => other,
    }
}

/// Draws the question, while one is open; returns the answer once given.
pub fn show(ctx: &egui::Context, shell: &Shell) -> Option<Answer> {
    let ask = asking(shell)?;
    let mut answer = None;
    let modal = crate::modal::Dialog::new(Id::NULL.with("shared-element"), tr!("shared-title"))
        .width(440.0)
        .show(ctx, |ui| {
            ui.label(tr!(
                "shared-intro",
                name = ask.name.as_str(),
                count = ask.parts.len().to_string()
            ));
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for part in &ask.parts {
                        let r = ui.label(part);
                        let label = part.clone();
                        r.widget_info(|| WidgetInfo::labeled(Role::ListItem, true, label.clone()));
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("shared-edit")).clicked() {
                    answer = Some(Answer::EditShared);
                }
                if ui.button(tr!("shared-copy")).clicked() {
                    answer = Some(Answer::MakeCopy);
                }
                if ui.button(tr!("dialog-cancel")).clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
        });
    if answer.is_none() && modal.should_close() {
        answer = Some(Answer::Cancel);
    }
    answer
}
