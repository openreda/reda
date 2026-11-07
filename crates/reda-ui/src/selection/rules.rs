//! The common eligibility rules for every canvas and GUI selection entry point.

use reda_engine::model::{LibraryElement, Uuid};
use reda_engine::render::{Child, Item};

use crate::shell::Shell;

/// Looks up a child by its stable canvas identity, hiding the Scene representation.
/// Picking already has an Item and uses its metadata directly on the hot path.
pub fn child_item(shell: &Shell, document: Uuid, object: Uuid) -> Option<&Item> {
    shell
        .document_tab(document)?
        .scene
        .scene
        .items
        .iter()
        .find(|item| {
            item.child
                .as_ref()
                .is_some_and(|child| child.id() == object)
        })
}

pub fn child(shell: &Shell, document: Uuid, object: Uuid) -> Option<&Child> {
    child_item(shell, document, object)?.child.as_ref()
}

fn child_kind(child: &Child) -> String {
    match child {
        Child::Field { .. } => "field_text",
        Child::Pad { .. } => "pad",
        Child::FootprintText { .. } => "footprint_text",
    }
    .into()
}

/// The Selection Filter kind of an Item, without searching the Scene for its child.
pub fn item_kind(shell: &Shell, document: Uuid, item: &Item) -> Option<String> {
    item.child
        .as_ref()
        .map(child_kind)
        .or_else(|| object_kind(shell, document, item.object))
}

/// The Selection Filter kind of a persisted object or a Library element's child.
pub fn kind(shell: &Shell, document: Uuid, object: Uuid) -> Option<String> {
    object_kind(shell, document, object).or_else(|| child(shell, document, object).map(child_kind))
}

fn object_kind(shell: &Shell, document: Uuid, object: Uuid) -> Option<String> {
    let project = shell.engine().elements()?;
    if let Some(object) = project.object(object) {
        return serde_json::to_value(object.object_type())
            .ok()?
            .as_str()
            .map(str::to_owned);
    }
    match shell.engine().element(document)? {
        LibraryElement::Symbol(s) => {
            if s.pins.iter().any(|p| p.uuid == object) {
                Some("pin".into())
            } else if s.graphics.iter().any(|g| g.uuid == object) {
                Some("graphic".into())
            } else {
                None
            }
        }
        LibraryElement::Footprint(f) => {
            if f.pads.iter().any(|p| p.uuid == object) {
                Some("pad".into())
            } else if f.graphics.iter().any(|g| g.uuid == object) {
                Some("graphic".into())
            } else if f.texts.iter().any(|t| t.uuid == object) {
                Some("text".into())
            } else {
                None
            }
        }
        LibraryElement::Part(_) => None,
    }
}

/// The Filter governs user picks; explicit `selection.set` intentionally bypasses this.
pub fn allowed(shell: &Shell, document: Uuid, object: Uuid) -> bool {
    kind(shell, document, object).is_some_and(|kind| {
        crate::bars::document_type(shell, document)
            .is_some_and(|ty| crate::working_state::filter_kinds(ty).contains(&kind.as_str()))
            && shell.selection_filter(document).allows(&kind)
    })
}

/// Rectangle direction follows the shared Preference on every canvas.
pub fn box_mode(
    shell: &Shell,
    from: (f64, f64),
    to: (f64, f64),
) -> crate::canvas::interact::BoxMode {
    if shell.preferences().editing.directional_rectangle {
        crate::canvas::interact::BoxMode::of_drag(from, to)
    } else {
        crate::canvas::interact::BoxMode::Enclose
    }
}

/// Placement locks apply to the owning Footprint; its own text remains movable.
pub fn locked(shell: &Shell, document: Uuid, object: Uuid) -> bool {
    let child = child(shell, document, object);
    if matches!(
        child,
        Some(reda_engine::render::Child::FootprintText { .. })
    ) {
        return false;
    }
    let owner = child.map_or(object, |c| c.owner());
    shell
        .engine()
        .project()
        .and_then(|p| p.pcb_document(document))
        .and_then(|p| p.footprint(owner))
        .is_some_and(|f| f.locked)
}
