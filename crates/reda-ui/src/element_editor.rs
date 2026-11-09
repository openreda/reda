//! What every Library element editor tab shares (the Footprint editor here, the Symbol
//! editor beside it): an open Symbol, Footprint or Part is a Document tab keyed by the
//! element's UUID, with or without a Project open (the engine keeps elements opened with
//! no Project on its workbench, and carries them into a Project that opens later).
//!
//! - The Shell opens the tab after `library.element.open` or `.new` from anywhere (menus,
//!   the Palette, an agent), and drops it after `library.element.close`.
//! - Its title is the element's name, marked read-only for starter Library elements, whose
//!   edits the engine refuses.
//! - Its Scene, grid, bars and Keymap context follow the element's kind; the editor of
//!   that kind turns canvas input into `library.*` Commands, which land on the element's
//!   own undo stack.
//! - Save (`library.element.save`) and the Library check ([`check`]) act on the element;
//!   the check's Violations go to the Messages panel.
//!
//! The Part editor has no canvas: its form ([`crate::part_editor`]) fills the tab.
//!
//! A new kind of element editor adds an arm to [`scene`], [`edit`], [`cancel`],
//! [`panels`] and [`has_canvas`].

use std::sync::Arc;

use reda_engine::Engine;
use reda_engine::command::CommandError;
use reda_engine::model::geom::Nm;
use reda_engine::model::{LibraryElement, Project, Uuid, Violation};
use reda_engine::render::{InactiveLayers, LayerView, Scene, board_composites};
use serde_json::{Value, json};

use crate::bars::DocumentType;
use crate::canvas::CanvasInputs;
use crate::grid;
use crate::messages::{self, Note};
use crate::shell::Shell;
use crate::tr;

/// A Command for the window to run after the frame.
pub type Invocation = (String, Value);

/// The `source` of the Library check's notes in the Messages panel.
pub const LIBRARY_CHECK: &str = "library_check";

/// The open element `uuid`, if a tab's UUID names one.
pub fn element(engine: &Engine, uuid: Uuid) -> Option<&LibraryElement> {
    engine.element(uuid)
}

/// Whether `uuid` is an open Library element (rather than a Document).
pub fn is_element(engine: &Engine, uuid: Uuid) -> bool {
    engine.element(uuid).is_some()
}

/// Whether the open element `uuid` is read-only: it comes from the starter Library.
pub fn read_only(engine: &Engine, uuid: Uuid) -> bool {
    engine.element_source(uuid).is_some_and(|s| s.read_only)
}

/// The tab title of the open element `uuid`: its name, marked when read-only.
pub fn title(engine: &Engine, uuid: Uuid) -> Option<String> {
    let name = engine.element(uuid)?.name().to_owned();
    Some(if read_only(engine, uuid) {
        tr!("element-read-only-title", name = name)
    } else {
        name
    })
}

/// The Document type whose bars and Keymap context the element's tab uses.
pub fn document_type(engine: &Engine, uuid: Uuid) -> Option<DocumentType> {
    match engine.element(uuid)? {
        LibraryElement::Symbol(_) => Some(DocumentType::Symbol),
        LibraryElement::Footprint(_) => Some(DocumentType::Footprint),
        LibraryElement::Part(_) => None,
    }
}

/// Whether the open element `uuid` is a Part.
pub fn is_part(engine: &Engine, uuid: Uuid) -> bool {
    matches!(engine.element(uuid), Some(LibraryElement::Part(_)))
}

/// The open elements, by UUID.
pub fn open_elements(engine: &Engine) -> Vec<Uuid> {
    engine
        .elements()
        .map(|p| p.elements().map(|(_, e)| e.uuid()).collect())
        .unwrap_or_default()
}

/// The grid presets of an element's editor, and the one it starts with: board grids for a
/// Footprint, sheet grids for a Symbol.
pub fn grid_presets(element: &LibraryElement) -> Option<(&'static [Nm], Nm)> {
    match element {
        LibraryElement::Footprint(_) => Some((
            &grid::PCB_PRESETS,
            grid::pcb_default(reda_engine::preferences::Units::Mm),
        )),
        LibraryElement::Symbol(_) => Some((&grid::SCHEMATIC_PRESETS, grid::SCHEMATIC_DEFAULT)),
        LibraryElement::Part(_) => None,
    }
}

/// What the element's canvas draws: its Scene and, for a Footprint, the Layers to
/// composite (every Layer, top copper active).
pub fn scene(
    engine: &Engine,
    uuid: Uuid,
) -> Option<(Scene, Option<Arc<[reda_engine::render::Composite]>>)> {
    match engine.element(uuid)? {
        LibraryElement::Footprint(f) => {
            let drawn = reda_engine::footprint_scene(f, engine.fonts());
            let active = drawn
                .layers
                .iter()
                .find(|l| {
                    l.kind == reda_engine::model::LayerKind::Copper
                        && l.side == Some(reda_engine::model::Side::Top)
                })
                .map(|l| l.uuid);
            let view = LayerView {
                visible: None,
                active,
                inactive: InactiveLayers::Normal,
                from_bottom: false,
                ratsnest: false,
                pinned: Vec::new(),
            };
            let layers = board_composites(&drawn.layers, &view).into();
            Some((drawn.scene, Some(layers)))
        }
        LibraryElement::Symbol(_) => Some((crate::symbol_editor::scene(engine, uuid)?, None)),
        // The Part editor draws no canvas.
        LibraryElement::Part(_) => Some((Scene::default(), None)),
    }
}

/// Whether the element's tab has a canvas: a Symbol's or Footprint's does, a Part's is
/// all form.
pub fn has_canvas(engine: &Engine, uuid: Uuid) -> bool {
    matches!(
        engine.element(uuid),
        Some(LibraryElement::Symbol(_) | LibraryElement::Footprint(_))
    )
}

/// Whether `uuid` names a part of an open element that its editor can select: a Pad,
/// graphic or text of a Footprint, a Pin or graphic of a Symbol.
pub fn has_part(engine: &Engine, uuid: Uuid) -> bool {
    engine.elements().is_some_and(|p| part_of(p, uuid))
}

/// [`has_part`] among the elements open in `project`.
pub fn part_of(project: &Project, uuid: Uuid) -> bool {
    project
        .elements()
        .any(|(_, element)| element_parts(element).contains(&uuid))
}

/// Everything selectable on the open element `element`.
pub fn parts(engine: &Engine, element: Uuid) -> Vec<Uuid> {
    engine
        .element(element)
        .map(element_parts)
        .unwrap_or_default()
}

fn element_parts(element: &LibraryElement) -> Vec<Uuid> {
    match element {
        LibraryElement::Footprint(f) => crate::footprint_editor::selectable(f),
        LibraryElement::Symbol(s) => {
            let pins = s.pins.iter().map(|p| p.uuid);
            pins.chain(s.graphics.iter().map(|g| g.uuid)).collect()
        }
        LibraryElement::Part(_) => Vec::new(),
    }
}

/// The selected parts of `element`, in selection order.
pub fn selected_on(shell: &Shell, element: Uuid) -> Vec<Uuid> {
    let parts = parts(shell.engine(), element);
    shell
        .selection()
        .objects
        .iter()
        .copied()
        .filter(|o| parts.contains(o))
        .collect()
}

/// Opens a tab for the element a successful `library.element.open` or `.new` returned.
pub(crate) fn after_open(shell: &mut Shell, out: &Value) {
    let Some(element) = out["element"].as_str() else {
        return;
    };
    let _ = shell.run("document.open", json!({ "document": element }));
}

/// Runs the editor of `element`'s kind on this frame's canvas input; returns the
/// Commands to run after the frame.
pub(crate) fn edit(shell: &mut Shell, element: Uuid, input: &CanvasInputs) -> Vec<Invocation> {
    match shell.engine().element(element) {
        Some(LibraryElement::Footprint(_)) => crate::footprint_editor::pointer(
            shell,
            element,
            &input.events,
            input.camera.nm_per_point,
        ),
        Some(LibraryElement::Symbol(_)) => {
            crate::symbol_editor::pointer(shell, element, &input.events, input.camera.nm_per_point)
        }
        _ => Vec::new(),
    }
}

/// Drops what the editor of `element` has in flight (Escape, another Tool).
pub(crate) fn cancel(shell: &mut Shell, element: Uuid) {
    match shell.engine().element(element) {
        Some(LibraryElement::Footprint(_)) => crate::footprint_editor::cancel(shell, element),
        Some(LibraryElement::Symbol(_)) => crate::symbol_editor::cancel(shell, element),
        _ => {}
    }
}

/// The panels around the element's canvas (the Footprint's inspector beside it, the
/// Symbol's Unit tabs over it and pin table under it), drawn into `ui` before the canvas
/// takes what is left; returns the Commands their controls ask for.
pub(crate) fn panels(ui: &mut egui::Ui, shell: &mut Shell, element: Uuid) -> Vec<Invocation> {
    match shell.engine().element(element) {
        Some(LibraryElement::Footprint(_)) => {
            let id = egui::Id::NULL.with(("element-side-panel", element));
            egui::Panel::right(id)
                .resizable(false)
                .exact_size(crate::footprint_editor::inspector::WIDTH)
                .show(ui, |ui| {
                    crate::footprint_editor::inspector::show(ui, shell, element)
                })
                .inner
        }
        Some(LibraryElement::Symbol(_)) => crate::symbol_editor::view::panels(ui, shell, element),
        Some(LibraryElement::Part(_)) => crate::part_editor::view::show(ui, shell, element),
        None => Vec::new(),
    }
}

/// The active Document, when it is an open Symbol.
pub(crate) fn active_symbol(shell: &Shell) -> Option<Uuid> {
    let active = shell.layout().active_document()?;
    matches!(
        shell.engine().element(active),
        Some(LibraryElement::Symbol(_))
    )
    .then_some(active)
}

/// Runs the Library check on the active element, if the active Document is one, after a
/// save: Save writes the file and the check's findings show in Messages.
pub(crate) fn check_after_save(shell: &mut Shell) {
    let active = shell.layout().active_document();
    if let Some(element) = active.filter(|e| is_element(shell.engine(), *e)) {
        let _ = check(shell, element);
    }
}

/// Runs the Library check on `element` (unsaved edits included) and lists what it finds
/// in the Messages panel, replacing the last check's lines.
pub fn check(shell: &mut Shell, element: Uuid) -> Result<(), CommandError> {
    let out = shell.run_reporting("library.check", json!({ "element": element }))?;
    let name = shell
        .engine()
        .element(element)
        .map(|e| e.name().to_owned())
        .unwrap_or_default();
    report(shell, Some(element), &name, &out["violations"]);
    Ok(())
}

/// Lists the Library check that Import part or a Footprint generator ran (its result
/// `out`) in the Messages panel, as [`check`] does.
pub(crate) fn report_created(shell: &mut Shell, out: &Value) {
    // A generator names its Footprint; an import names its files, the Part's last.
    let name = out["name"].as_str().map(str::to_owned).or_else(|| {
        let part = out["files"].as_array()?.last()?.as_str()?;
        let stem = std::path::Path::new(part).file_stem()?;
        Some(stem.to_string_lossy().into_owned())
    });
    report(shell, None, &name.unwrap_or_default(), &out["violations"]);
}

/// Replaces the Library check's lines in Messages with `violations` (a JSON list) found
/// in `name`, or says in the status bar that there were none.
fn report(shell: &mut Shell, element: Option<Uuid>, name: &str, violations: &Value) {
    let violations: Vec<Violation> = serde_json::from_value(violations.clone()).unwrap_or_default();
    let notes: Vec<Note> = violations
        .iter()
        .map(|v| Note {
            source: LIBRARY_CHECK.to_owned(),
            document: element,
            severity: v.severity,
            text: tr!(
                "library-check-line",
                element = name,
                text = messages::violation_text(v)
            ),
        })
        .collect();
    if notes.is_empty() {
        shell.set_note(tr!("library-check-clean", element = name));
    }
    shell.set_notes(LIBRARY_CHECK, notes);
}

/// A text field `width` points wide holding a draft of `current`, announced as `label`;
/// returns the trimmed text once committed (Enter or focus left) and changed.
pub(crate) fn text_field(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    current: &str,
    width: f32,
) -> Option<String> {
    let mut draft = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| current.to_owned());
    let response = ui.add(
        egui::TextEdit::singleline(&mut draft)
            .id(id)
            .desired_width(width),
    );
    let name = label.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, name.clone()));
    let committed = response.lost_focus()
        || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    if response.has_focus() && !committed {
        ui.data_mut(|d| d.insert_temp(id, draft));
        return None;
    }
    ui.data_mut(|d| d.remove::<String>(id));
    (committed && draft.trim() != current).then(|| draft.trim().to_owned())
}
