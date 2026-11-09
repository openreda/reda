//! The view Commands that open the Library dialogs from their entry points:
//! `schematic.update_from_library.open` (Component ▸ Update from Library… on a selection,
//! the Properties badge, a Library panel row, a Library editor's Update in Project…),
//! `schematic.component.replace.open` (the Properties Part row's Replace…) and
//! `pcb.update_from_library.open` (the PCB's Design ▸ Update from Library…).

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_engine::model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{ReplaceDialog, SchematicDialog, UpdateDialog, UpdateScope};
use crate::bars::{Request, request};
use crate::shell::Shell;

/// A Library dialog to open.
#[derive(Clone, Debug, PartialEq)]
pub enum LibraryDialog {
    /// Update from Library on the selected objects' Components, these Parts, or the Parts
    /// that use this Library element (saved first).
    Update {
        objects: Vec<Uuid>,
        parts: Vec<Uuid>,
        element: Option<Uuid>,
    },
    /// Replace Part on the selected objects' Components.
    Replace(Vec<Uuid>),
    /// The PCB's Update from Library.
    Board,
}

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(OpenUpdate);
    registry.add(OpenReplace);
    registry.add(OpenBoardUpdate);
}

/// What the opening Commands return.
#[derive(Serialize, JsonSchema)]
pub struct Opened {}

fn has_sheet(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    let ok = cx
        .project
        .is_some_and(|p| p.documents().any(|d| p.schematic_sheet(d.uuid).is_some()));
    ok.then_some(()).ok_or_else(|| Unavailable {
        reason: "no Project with a Schematic Sheet is open".into(),
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenUpdateParams {
    /// Selected Units (or synced Footprints): Selected Parts Only starts ticked with their
    /// Components.
    #[serde(default)]
    objects: Vec<Uuid>,
    /// Only these Library Parts start ticked.
    #[serde(default)]
    parts: Vec<Uuid>,
    /// A Library element open in its editor: it is saved, and only the Parts that use it
    /// start ticked.
    element: Option<Uuid>,
}

struct OpenUpdate;

impl Command for OpenUpdate {
    const META: Meta = Meta {
        id: "schematic.update_from_library.open",
        title: "Update from Library…",
        description: "Opens the Update from Library dialog: on the selected Components \
                      (Selected Parts Only), on some Library Parts, or on the Parts that use \
                      a Library element, which is saved first (Update in Project…). \
                      Nothing updates without its preview.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::View,
    };
    type Params = OpenUpdateParams;
    type Output = Opened;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        has_sheet(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: OpenUpdateParams) -> Result<Opened, CommandError> {
        request(
            cx,
            Request::Library(LibraryDialog::Update {
                objects: p.objects,
                parts: p.parts,
                element: p.element,
            }),
        )?;
        Ok(Opened {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenReplaceParams {
    /// The selected Units (or synced Footprints); their Components all take the Part
    /// chosen.
    objects: Vec<Uuid>,
}

struct OpenReplace;

impl Command for OpenReplace {
    const META: Meta = Meta {
        id: "schematic.component.replace.open",
        title: "Replace Part…",
        description: "Opens Replace Part on the selected Components: the Library's Parts \
                      with the current one on top and the pin differences under them.",
        category: "schematic",
        icon: "library_update",
        kind: Kind::View,
    };
    type Params = OpenReplaceParams;
    type Output = Opened;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        has_sheet(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: OpenReplaceParams) -> Result<Opened, CommandError> {
        request(cx, Request::Library(LibraryDialog::Replace(p.objects)))?;
        Ok(Opened {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenBoardUpdateParams {}

struct OpenBoardUpdate;

impl Command for OpenBoardUpdate {
    const META: Meta = Meta {
        id: "pcb.update_from_library.open",
        title: "Update from Library…",
        description: "Opens the PCB's Update from Library dialog: the Mechanical Parts \
                      placed on the board whose Footprint differs from the Library's, with \
                      the Pads changed and the local edits each update removes.",
        category: "pcb",
        icon: "library_update",
        kind: Kind::View,
    };
    type Params = OpenBoardUpdateParams;
    type Output = Opened;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        if cx.project.is_some_and(|p| p.pcb().is_some()) {
            Ok(())
        } else {
            Err(Unavailable {
                reason: "no Project with a PCB is open".into(),
            })
        }
    }

    fn run(&self, cx: &mut Context<'_>, _: OpenBoardUpdateParams) -> Result<Opened, CommandError> {
        request(cx, Request::Library(LibraryDialog::Board))?;
        Ok(Opened {})
    }
}

impl LibraryDialog {
    /// The dialog this asks for, on the Shell's Project.
    pub fn open(self, shell: &mut Shell) -> Option<SchematicDialog> {
        let project = shell.engine().project()?;
        Some(match self {
            LibraryDialog::Update {
                objects,
                mut parts,
                element,
            } => {
                let mut scope = UpdateScope::of_selection(project, &objects);
                if let Some(element) = element {
                    parts.extend(parts_using(shell, element));
                }
                scope.parts = parts;
                SchematicDialog::Update(Box::new(UpdateDialog::new(shell, scope)))
            }
            LibraryDialog::Replace(objects) => {
                let scope = UpdateScope::of_selection(project, &objects);
                if scope.components.is_empty() {
                    return None;
                }
                SchematicDialog::Replace(Box::new(ReplaceDialog::new(shell, scope.components)))
            }
            LibraryDialog::Board => SchematicDialog::Board(Box::new(
                crate::pcb::update_dialog::BoardUpdateDialog::new(shell),
            )),
        })
    }
}

/// Saves the Library element `element` and returns the Parts that use it: itself for a
/// Part, else the Parts across the Libraries naming it.
fn parts_using(shell: &mut Shell, element: Uuid) -> Vec<Uuid> {
    let _ = shell.run_reporting("library.element.save", json!({ "element": element }));
    let Ok(out) = shell.run("library.where_used", json!({ "element": element })) else {
        return vec![element];
    };
    let parts: Vec<Uuid> = (out["parts"].as_array().into_iter().flatten())
        .filter_map(|p| p["uuid"].as_str()?.parse().ok())
        .collect();
    if out["kind"] == "part" || parts.is_empty() {
        vec![element]
    } else {
        parts
    }
}
