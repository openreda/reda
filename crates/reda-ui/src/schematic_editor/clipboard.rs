//! Cut, copy and paste on a sheet in the GUI (docs/spec/v0.1/schematic.md#cut-copy-and-paste):
//!
//! - `tool.copy` (Mod+C, Mod+X with `cut`) copies at once, or with the **Clipboard
//!   Reference** Preference first asks for a click on the reference point a paste holds.
//! - `tool.paste` (Mod+V) floats the clipboard on the cursor until a click, which runs one
//!   `schematic.paste`. A hotspot of the pasted objects (a Wire vertex, a Symbol origin, a
//!   label's anchor) is held on the snap grid, so Pins and Wire ends stay on grid.
//! - `tool.duplicate` (Mod+D) picks up a copy of the selection as a repeating stamp: each
//!   click pastes another, with Designators following the paste rule, until Esc.
//! - `schematic.copy_as_text` puts the selection's names and Designators on the system
//!   clipboard ([`Shell::take_copied_text`]).
//!
//! Nothing in flight touches the Project: the canvas previews the paste
//! ([`Engine::preview`](reda_engine::Engine::preview)).

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_engine::model::Uuid;
use reda_engine::model::geom::Point;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Cx, Interaction, P, restate};
use crate::canvas::interact::Pointer;
use crate::shell::Shell;
use crate::tr;

/// A clipboard floating on the cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct Pasting {
    /// What `schematic.copy` returned.
    pub clipboard: Value,
    /// From the clipboard's origin to the hotspot held on the grid.
    pub hold: Point,
    /// Where the clipboard's origin goes now.
    pub at: Point,
    /// A Duplicate stamp: each click pastes another until Esc.
    pub repeat: bool,
    /// The preview drawn: for (at, Scene generation).
    pub(super) shown: Option<(Point, u64)>,
}

/// Cut or copy waiting for the click on its reference point.
#[derive(Clone, Debug, PartialEq)]
pub struct Referencing {
    pub objects: Vec<Uuid>,
    pub cut: bool,
}

/// The hotspot of `clipboard` a paste keeps on the grid, from its origin: the first Wire
/// vertex, else the first Unit's origin, else the first label, Port, Power Port or
/// No-Connect anchor; the origin itself when there is none.
fn hotspot(clipboard: &Value) -> Point {
    let read = |v: &Value| -> Option<Point> {
        Some(Point::new(
            reda_engine::model::geom::Nm(v["x"].as_i64()?),
            reda_engine::model::geom::Nm(v["y"].as_i64()?),
        ))
    };
    let origin = read(&clipboard["origin"]).unwrap_or_default();
    let first = |key: &str, field: &str| clipboard[key].get(0).and_then(|o| read(&o[field]));
    let spot = clipboard["wires"]
        .get(0)
        .and_then(|w| read(&w["vertices"][0]))
        .or_else(|| first("units", "at"))
        .or_else(|| first("net_labels", "at"))
        .or_else(|| first("ports", "at"))
        .or_else(|| first("power_ports", "at"))
        .or_else(|| first("no_connects", "at"));
    spot.map_or(Point::default(), |s| s - origin)
}

/// Floats `clipboard` on `document`'s canvas, held by its origin or a hotspot; a
/// Duplicate stamp with `repeat`.
pub fn start_paste(shell: &mut Shell, document: Uuid, clipboard: Value, repeat: bool) {
    // An origin on the grid (a reference point clicked, or a corner that happens to be)
    // is held itself; else a hotspot of the objects is.
    let grid = shell.grid(document).0.max(1);
    let origin = (
        clipboard["origin"]["x"].as_i64(),
        clipboard["origin"]["y"].as_i64(),
    );
    let hold = match origin {
        (Some(x), Some(y)) if x % grid == 0 && y % grid == 0 => Point::default(),
        _ => hotspot(&clipboard),
    };
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    let origin = clipboard["origin"].clone();
    let fallback = (
        origin["x"].as_f64().unwrap_or_default(),
        origin["y"].as_f64().unwrap_or_default(),
    );
    let pointer = tab.cursor.unwrap_or(fallback);
    let mut pasting = Pasting {
        clipboard,
        hold,
        at: Point::default(),
        repeat,
        shown: None,
    };
    pasting.at = held_at(shell, document, hold, pointer);
    if let Some(tab) = shell.document_tab_mut(document) {
        tab.interaction = Interaction::Pasting(pasting);
    }
    restate(shell, document);
}

/// Where the clipboard's origin goes with the pointer at `pointer`: the pointer holds the
/// point `hold` from the origin, snapped to the grid.
fn held_at(shell: &Shell, document: Uuid, hold: Point, pointer: P) -> Point {
    shell.snap_to_grid(document, pointer) - hold
}

/// `tool.copy`: copies (or cuts) `objects` now, or with Clipboard Reference on asks for
/// the reference point first.
pub fn copy(shell: &mut Shell, objects: Vec<Uuid>, cut: bool) {
    let id = if cut {
        "schematic.cut"
    } else {
        "schematic.copy"
    };
    let sheet = shell.active_sheet();
    match sheet {
        Some(sheet) if shell.preferences().schematic.clipboard_reference => {
            if let Some(tab) = shell.document_tab_mut(sheet) {
                tab.interaction = Interaction::Referencing(Referencing { objects, cut });
            }
            restate(shell, sheet);
            shell.report(tr!("clipboard-reference-prompt"));
        }
        _ => {
            let _ = shell.run_reporting(id, json!({ "objects": objects }));
        }
    }
}

/// `tool.paste`: floats `clipboard` on `document`'s canvas.
pub fn paste(shell: &mut Shell, document: Uuid, clipboard: Value) {
    start_paste(shell, document, clipboard, false);
}

/// `tool.duplicate`: picks a copy of `objects` up as a repeating stamp.
pub fn duplicate(shell: &mut Shell, objects: Vec<Uuid>) {
    let Some(sheet) = shell.active_sheet() else {
        return;
    };
    match shell
        .engine
        .run("schematic.copy", json!({ "objects": objects }))
    {
        Ok(clipboard) => start_paste(shell, sheet, clipboard, true),
        Err(e) => shell.report(tr!("error-command", error = e.message)),
    }
}

impl Cx<'_> {
    /// A floating paste on one pointer event: follows the pointer, pastes on a click.
    pub(super) fn paste_step(&mut self, mut p: Pasting, event: Pointer) -> Interaction {
        match event {
            Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragStart { at, .. } => {
                p.at = held_at(self.shell, self.document, p.hold, at);
                Interaction::Pasting(p)
            }
            Pointer::Click { at, .. } | Pointer::DragEnd(at) => {
                p.at = held_at(self.shell, self.document, p.hold, at);
                self.commands.push((
                    "schematic.paste".to_owned(),
                    json!({
                        "document": self.document,
                        "clipboard": p.clipboard,
                        "at": {"x": p.at.x.0, "y": p.at.y.0},
                    }),
                ));
                if p.repeat {
                    p.shown = None;
                    Interaction::Pasting(p)
                } else {
                    Interaction::Idle
                }
            }
        }
    }

    /// Waiting for the reference point: a click copies (or cuts) with it.
    pub(super) fn reference_step(&mut self, r: Referencing, event: Pointer) -> Interaction {
        let Pointer::Click { at, .. } = event else {
            return Interaction::Referencing(r);
        };
        let targets = self
            .sheet()
            .map(crate::snap::hotspots::sheet)
            .unwrap_or_default();
        let origin = crate::snap::snap_at(self.shell, self.document, at, &targets).at;
        let id = if r.cut {
            "schematic.cut"
        } else {
            "schematic.copy"
        };
        self.commands.push((
            id.to_owned(),
            json!({"objects": r.objects, "origin": {"x": origin.x.0, "y": origin.y.0}}),
        ));
        Interaction::Idle
    }
}

/// Draws the paste in flight into `document`'s Scene, unless it shows already.
pub(super) fn preview(shell: &mut Shell, document: Uuid, p: &mut Pasting, generation: u64) {
    if p.shown == Some((p.at, generation)) {
        return;
    }
    let params = json!({
        "document": document,
        "clipboard": p.clipboard,
        "at": {"x": p.at.x.0, "y": p.at.y.0},
    });
    let parts = reda_engine::library::ReadyParts::new(Vec::new());
    if let Ok((project, _)) = shell.engine().preview("schematic.paste", params, &parts) {
        shell.show_preview(document, &project);
    }
    let generation = shell
        .document_tab(document)
        .map_or(generation, |t| t.scene.generation);
    p.shown = Some((p.at, generation));
}

/// Adds `tool.copy`, `tool.paste` and `tool.duplicate` to `registry`.
pub(super) fn register(registry: &mut Registry) {
    registry.add(ToolCopy);
    registry.add(ToolPaste);
    registry.add(ToolDuplicate);
}

fn needs_sheet(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    let has_sheet = cx
        .project
        .is_some_and(|p| p.documents().any(|d| p.schematic_sheet(d.uuid).is_some()));
    if has_sheet {
        Ok(())
    } else {
        Err(Unavailable {
            reason: "no Project with a Schematic Sheet is open".into(),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolCopyParams {
    /// The objects to copy, by UUID, all on one sheet.
    objects: Vec<Uuid>,
    /// Cut rather than copy.
    #[serde(default)]
    cut: bool,
}

/// What a GUI clipboard Command asks of the Shell, which carries it out.
#[derive(Serialize, JsonSchema)]
pub struct Asked {
    /// The objects it acts on, when it takes some.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<Uuid>,
    pub cut: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<Uuid>,
    /// The clipboard to float, for a paste.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<Value>,
}

struct ToolCopy;

impl Command for ToolCopy {
    const META: Meta = Meta {
        id: "tool.copy",
        title: "Copy",
        description: "Copies (or with `cut`, cuts) the selected objects to the clipboard in \
                      the window, with schematic.copy or schematic.cut. With the Clipboard \
                      Reference Preference on, a click on the reference point comes first.",
        category: "tool",
        icon: "copy",
        kind: Kind::View,
    };
    type Params = ToolCopyParams;
    type Output = Asked;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_sheet(cx)
    }

    fn run(&self, _cx: &mut Context<'_>, p: ToolCopyParams) -> Result<Asked, CommandError> {
        if p.objects.is_empty() {
            return Err(CommandError::rejected("nothing is selected"));
        }
        Ok(Asked {
            objects: p.objects,
            cut: p.cut,
            document: None,
            clipboard: None,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolPasteParams {
    /// The Schematic Sheet to paste on.
    document: Uuid,
    /// What schematic.copy or schematic.cut returned.
    clipboard: Value,
}

struct ToolPaste;

impl Command for ToolPaste {
    const META: Meta = Meta {
        id: "tool.paste",
        title: "Paste",
        description: "Floats the clipboard on the cursor over the sheet, with a hotspot on \
                      the snap grid, until a click pastes it with schematic.paste.",
        category: "tool",
        icon: "paste",
        kind: Kind::View,
    };
    type Params = ToolPasteParams;
    type Output = Asked;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_sheet(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ToolPasteParams) -> Result<Asked, CommandError> {
        if cx.project()?.schematic_sheet(p.document).is_none() {
            return Err(CommandError::invalid_params(
                "/document",
                format!("{} is not a Schematic Sheet", p.document),
            ));
        }
        Ok(Asked {
            objects: Vec::new(),
            cut: false,
            document: Some(p.document),
            clipboard: Some(p.clipboard),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolDuplicateParams {
    /// The objects to duplicate, by UUID, all on one sheet.
    objects: Vec<Uuid>,
}

struct ToolDuplicate;

impl Command for ToolDuplicate {
    const META: Meta = Meta {
        id: "tool.duplicate",
        title: "Duplicate",
        description: "Picks up a copy of the selected objects as a repeating stamp: each \
                      click pastes another (schematic.paste), Designators keeping theirs \
                      when free or taking the next free one, until Escape.",
        category: "tool",
        icon: "duplicate",
        kind: Kind::View,
    };
    type Params = ToolDuplicateParams;
    type Output = Asked;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_sheet(cx)
    }

    fn run(&self, _cx: &mut Context<'_>, p: ToolDuplicateParams) -> Result<Asked, CommandError> {
        if p.objects.is_empty() {
            return Err(CommandError::rejected("nothing is selected"));
        }
        Ok(Asked {
            objects: p.objects,
            cut: false,
            document: None,
            clipboard: None,
        })
    }
}
