//! In-place edit (docs/spec/v0.1/schematic.md#fields): F2 (`schematic.edit_in_place`) or
//! a slow second click on what is selected edits a Net Label, Port or Power Port name, a
//! Designator, value or visible field, or free text, in a text field on the canvas. Enter
//! commits one Command, Esc cancels, and Shift+Enter adds a line in free text. A
//! Designator another Component uses is refused inline, and a rename applies to every
//! Unit (`schematic.component.set`). The **In-Place Editing** Preference turns it off.
//!
//! Tab while placing a Net Label, Port or Power Port pauses the Tool and edits the name
//! the next placement takes the same way: Enter resumes with it (later placements count
//! up from it), Esc resumes without the edit. A "Paused" chip shows meanwhile.

use reda_engine::model::geom::Point;
use reda_engine::model::{Object, Project, UnitInstance, Uuid};
use serde_json::{Value, json};

use super::{Interaction, Invocation, restate};
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;

/// What an in-place edit changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditTarget {
    NetLabel(Uuid),
    Port(Uuid),
    PowerPort(Uuid),
    /// Free text.
    Text(Uuid),
    /// A Unit's Designator (`designator`), value (`value`) or another field, by name.
    Field {
        unit: Uuid,
        field: String,
    },
}

/// An in-place edit in flight.
#[derive(Clone, Debug, PartialEq)]
pub struct InPlace {
    pub target: EditTarget,
    /// The text being typed.
    pub text: String,
    /// Where on the sheet the text field shows.
    pub at: Point,
    /// Free text: Shift+Enter adds a line.
    pub multiline: bool,
    /// Why Enter was refused (a Designator in use), shown under the field.
    pub error: Option<String>,
}

/// The text `target` holds now, and where it sits.
fn current(project: &Project, target: &EditTarget) -> Option<(String, Point)> {
    match target {
        EditTarget::NetLabel(uuid) => match project.object(*uuid)? {
            Object::NetLabel(l) => Some((l.object.name.clone(), l.object.at)),
            _ => None,
        },
        EditTarget::Port(uuid) => match project.object(*uuid)? {
            Object::Port(p) => Some((p.object.name.clone(), p.object.at)),
            _ => None,
        },
        EditTarget::PowerPort(uuid) => match project.object(*uuid)? {
            Object::PowerPort(p) => Some((p.object.name.clone(), p.object.at)),
            _ => None,
        },
        EditTarget::Text(uuid) => match project.object(*uuid)? {
            Object::SheetText(t) => Some((t.object.text.clone(), t.object.at)),
            _ => None,
        },
        EditTarget::Field { unit, field } => match project.object(*unit)? {
            Object::Unit(u) => {
                let text = if field == UnitInstance::DESIGNATOR {
                    u.object.designator.clone()
                } else {
                    u.object.fields.get(field).to_owned()
                };
                Some((text, u.object.at))
            }
            _ => None,
        },
    }
}

/// The target an object (or a Unit's field) gives: a Unit edits its Designator.
pub fn target_of(project: &Project, object: Uuid, field: Option<&str>) -> Option<EditTarget> {
    Some(match project.object(object)? {
        Object::NetLabel(_) => EditTarget::NetLabel(object),
        Object::Port(_) => EditTarget::Port(object),
        Object::PowerPort(_) => EditTarget::PowerPort(object),
        Object::SheetText(_) => EditTarget::Text(object),
        Object::Unit(u) => {
            let field = field.unwrap_or(UnitInstance::DESIGNATOR);
            // Only a field drawn on the sheet is edited there.
            if field != UnitInstance::DESIGNATOR && !u.object.visible_fields.contains(field) {
                return None;
            }
            EditTarget::Field {
                unit: object,
                field: field.to_owned(),
            }
        }
        _ => return None,
    })
}

/// The target F2 edits: the one selected field, or the one selected object.
fn selected_target(shell: &Shell) -> Option<EditTarget> {
    let project = shell.engine().project()?;
    let selection = shell.selection();
    match (selection.fields.as_slice(), selection.objects.as_slice()) {
        ([f], _) => target_of(project, f.unit, Some(&f.field)),
        ([], [object]) => target_of(project, *object, None),
        _ => None,
    }
}

/// Starts editing `target` on `document`'s canvas. Returns whether it started: the
/// In-Place Editing Preference is on, the Select Tool is active and nothing is in flight.
pub fn begin(shell: &mut Shell, document: Uuid, target: EditTarget) -> bool {
    let Some(edit) = start(shell, target) else {
        return false;
    };
    let Some(tab) = shell.document_tab_mut(document) else {
        return false;
    };
    if !matches!(tab.interaction, Interaction::Idle) {
        return false;
    }
    tab.interaction = Interaction::Editing(edit);
    restate(shell, document);
    true
}

/// The edit of `target`, when in-place editing may start: the In-Place Editing
/// Preference is on and the Select Tool is active.
pub(super) fn start(shell: &Shell, target: EditTarget) -> Option<InPlace> {
    if !shell.preferences().schematic.in_place_editing || shell.tool() != Tool::Select {
        return None;
    }
    let (text, at) = current(shell.engine().project()?, &target)?;
    let multiline = matches!(target, EditTarget::Text(_));
    Some(InPlace {
        target,
        text,
        at,
        multiline,
        error: None,
    })
}

/// Starts editing `target` on the active sheet's canvas (Properties or a test).
pub fn begin_edit(shell: &mut Shell, target: EditTarget) -> bool {
    match shell.active_sheet() {
        Some(sheet) => begin(shell, sheet, target),
        None => false,
    }
}

/// F2: edits the selected object in place.
pub fn edit_selected(shell: &mut Shell, document: Uuid) -> bool {
    match selected_target(shell) {
        Some(target) => begin(shell, document, target),
        None => false,
    }
}

/// The edit in flight on `document`'s canvas.
pub fn editing(shell: &Shell, document: Uuid) -> Option<&InPlace> {
    match &shell.document_tab(document)?.interaction {
        Interaction::Editing(e) => Some(e),
        _ => None,
    }
}

/// Types `text` into the edit (or the paused placement's name) on `document`'s canvas.
pub fn set_text(shell: &mut Shell, document: Uuid, text: String) {
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    match &mut tab.interaction {
        Interaction::Editing(e) => {
            e.text = text;
            e.error = None;
        }
        Interaction::Placing(p) if p.paused.is_some() => p.paused = Some(text),
        _ => {}
    }
}

/// Enter: the Command the edit makes, and the edit ends; a refused Designator keeps it
/// open with the reason. A paused placement resumes with the name typed.
pub fn commit(shell: &mut Shell, document: Uuid) -> Vec<Invocation> {
    if resume(shell, document, true) {
        return Vec::new();
    }
    let Some(edit) = editing(shell, document).cloned() else {
        return Vec::new();
    };
    let Some(project) = shell.engine().project() else {
        return Vec::new();
    };
    let unchanged = current(project, &edit.target).is_some_and(|(t, _)| t == edit.text);
    let command = match &edit.target {
        _ if unchanged => None,
        EditTarget::NetLabel(uuid) => Some((
            "schematic.net_label.set",
            json!({"uuid": uuid, "name": edit.text}),
        )),
        EditTarget::Port(uuid) => Some((
            "schematic.port.set",
            json!({"uuid": uuid, "name": edit.text}),
        )),
        EditTarget::PowerPort(uuid) => Some((
            "schematic.power_port.set",
            json!({"uuid": uuid, "name": edit.text}),
        )),
        EditTarget::Text(uuid) => Some((
            "schematic.text.set",
            json!({"uuid": uuid, "text": edit.text}),
        )),
        EditTarget::Field { unit, field } => {
            let Some(Object::Unit(u)) = project.object(*unit) else {
                return Vec::new();
            };
            let component = u.object.component.uuid;
            if field == UnitInstance::DESIGNATOR {
                let wanted = edit.text.trim();
                if let Some(problem) = designator_problem(project, component, wanted) {
                    if let Some(Interaction::Editing(e)) =
                        shell.document_tab_mut(document).map(|t| &mut t.interaction)
                    {
                        e.error = Some(problem);
                    }
                    return Vec::new();
                }
                Some((
                    "schematic.component.set",
                    json!({"components": [component], "designator": wanted}),
                ))
            } else if field == "value" {
                Some((
                    "schematic.component.set",
                    json!({"components": [component], "value": edit.text}),
                ))
            } else {
                Some((
                    "schematic.component.set",
                    json!({"components": [component], "fields": {field.as_str(): edit.text}}),
                ))
            }
        }
    };
    cancel_edit(shell, document);
    command
        .map(|(id, params): (&str, Value)| (id.to_owned(), params))
        .into_iter()
        .collect()
}

/// Why `designator` cannot be `component`'s: blank, or another Component uses it.
fn designator_problem(project: &Project, component: Uuid, designator: &str) -> Option<String> {
    if designator.is_empty() {
        return Some(tr!("in-place-designator-blank"));
    }
    let taken = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units())
        .any(|u| u.designator == designator && u.component.uuid != component);
    taken.then(|| {
        tr!(
            "in-place-designator-taken",
            designator = designator.to_owned()
        )
    })
}

/// Esc: drops the edit; a paused placement resumes without it.
pub fn cancel_edit(shell: &mut Shell, document: Uuid) {
    if resume(shell, document, false) {
        return;
    }
    if let Some(tab) = shell.document_tab_mut(document)
        && matches!(tab.interaction, Interaction::Editing(_))
    {
        tab.interaction = Interaction::Idle;
        restate(shell, document);
    }
}

/// Tab while placing a Net Label, Port or Power Port: pauses the Tool to edit the name
/// the next placement takes. Returns whether it paused.
pub fn pause_placing(shell: &mut Shell, document: Uuid) -> bool {
    let Some(tab) = shell.document_tab_mut(document) else {
        return false;
    };
    match &mut tab.interaction {
        Interaction::Placing(p)
            if matches!(p.tool, Tool::NetLabel | Tool::Port | Tool::PowerPort)
                && p.paused.is_none() =>
        {
            p.paused = Some(p.name.clone());
            true
        }
        _ => false,
    }
}

/// Ends a paused placement on `document`'s canvas: with `keep`, its name becomes the
/// typed one. Returns whether one was paused.
fn resume(shell: &mut Shell, document: Uuid, keep: bool) -> bool {
    let Some(tab) = shell.document_tab_mut(document) else {
        return false;
    };
    let Interaction::Placing(p) = &mut tab.interaction else {
        return false;
    };
    let Some(name) = p.paused.take() else {
        return false;
    };
    let name = name.trim().to_owned();
    if keep && !name.is_empty() {
        p.name = name;
    }
    restate(shell, document);
    true
}

/// The name a paused placement on `document`'s canvas is editing, and where.
pub fn paused(shell: &Shell, document: Uuid) -> Option<(&str, Point)> {
    match &shell.document_tab(document)?.interaction {
        Interaction::Placing(p) => p.paused.as_deref().map(|n| (n, p.at)),
        _ => None,
    }
}

/// `schematic.edit_in_place` (F2): the Shell starts the edit of the selected object.
pub(super) struct EditInPlace;

impl reda_engine::command::Command for EditInPlace {
    const META: reda_engine::command::Meta = reda_engine::command::Meta {
        id: "schematic.edit_in_place",
        title: "Edit In Place",
        description: "Edits the selected Net Label, Port or Power Port name, Designator, \
                      value, visible field or free text in a text field on the canvas: \
                      Enter commits one Command, Esc cancels. Needs the In-Place Editing \
                      Preference.",
        category: "schematic",
        icon: "rename",
        kind: reda_engine::command::Kind::View,
    };
    type Params = reda_engine::command::NoParams;
    type Output = ();

    fn run(
        &self,
        _cx: &mut reda_engine::command::Context<'_>,
        _: reda_engine::command::NoParams,
    ) -> Result<(), reda_engine::command::CommandError> {
        Ok(())
    }
}
