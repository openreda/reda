//! The common eligibility rules for every canvas and GUI selection entry point.

use reda_engine::model::{LibraryElement, Uuid};
use reda_engine::render::{Child, Item};

use crate::canvas::interact::BoxMode;
use crate::shell::Shell;
use crate::working_state::{FilterKind, filter_kinds};

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

fn child_kind(child: &Child) -> FilterKind {
    match child {
        Child::Field { .. } => FilterKind::FieldText,
        Child::Pad { .. } => FilterKind::Pad,
        Child::FootprintText { .. } => FilterKind::FootprintText,
    }
}

/// The Selection Filter kind of an Item, without searching the Scene for its child.
pub fn item_kind(shell: &Shell, document: Uuid, item: &Item) -> Option<FilterKind> {
    item.child
        .as_ref()
        .map(child_kind)
        .or_else(|| object_kind(shell, document, item.object))
}

/// The Selection Filter kind of a persisted object or a Library element's child.
pub fn kind(shell: &Shell, document: Uuid, object: Uuid) -> Option<FilterKind> {
    object_kind(shell, document, object).or_else(|| child(shell, document, object).map(child_kind))
}

fn object_kind(shell: &Shell, document: Uuid, object: Uuid) -> Option<FilterKind> {
    let project = shell.engine().elements()?;
    if let Some(object) = project.object(object) {
        return FilterKind::of_object(object.object_type());
    }
    match shell.engine().element(document)? {
        LibraryElement::Symbol(s) => {
            if s.pins.iter().any(|p| p.uuid == object) {
                Some(FilterKind::Pin)
            } else if s.graphics.iter().any(|g| g.uuid == object) {
                Some(FilterKind::Graphic)
            } else {
                None
            }
        }
        LibraryElement::Footprint(f) => {
            if f.pads.iter().any(|p| p.uuid == object) {
                Some(FilterKind::Pad)
            } else if f.graphics.iter().any(|g| g.uuid == object) {
                Some(FilterKind::Graphic)
            } else if f.texts.iter().any(|t| t.uuid == object) {
                Some(FilterKind::Text)
            } else {
                None
            }
        }
        LibraryElement::Part(_) => None,
    }
}

/// Whether `document`'s type lists `kind` and its Filter has it on.
fn kind_allowed(shell: &Shell, document: Uuid, kind: FilterKind) -> bool {
    crate::bars::document_type(shell, document).is_some_and(|ty| filter_kinds(ty).contains(&kind))
        && shell.selection_filter(document).allows(kind)
}

/// The Filter governs user picks; explicit `selection.set` intentionally bypasses this.
pub fn allowed(shell: &Shell, document: Uuid, object: Uuid) -> bool {
    kind(shell, document, object).is_some_and(|kind| kind_allowed(shell, document, kind))
}

/// [`allowed`] for a Scene Item.
pub fn item_allowed(shell: &Shell, document: Uuid, item: &Item) -> bool {
    item_kind(shell, document, item).is_some_and(|kind| kind_allowed(shell, document, kind))
}

/// Placement locks apply to the owning Footprint; its own text remains movable.
pub fn locked(shell: &Shell, document: Uuid, object: Uuid) -> bool {
    child_locked(shell, document, object, child(shell, document, object))
}

fn child_locked(shell: &Shell, document: Uuid, object: Uuid, child: Option<&Child>) -> bool {
    if matches!(child, Some(Child::FootprintText { .. })) {
        return false;
    }
    let owner = child.map_or(object, Child::owner);
    shell
        .engine()
        .project()
        .and_then(|p| p.pcb_document(document))
        .and_then(|p| p.footprint(owner))
        .is_some_and(|f| f.locked)
}

fn protects_locks(shell: &Shell) -> bool {
    shell.preferences().editing.protect_locked_objects
}

/// What a click, rectangle or drag may pick or move: the Filter allows it, and Protect
/// Locked Objects doesn't protect it.
pub fn pickable(shell: &Shell, document: Uuid, object: Uuid) -> bool {
    allowed(shell, document, object) && !(protects_locks(shell) && locked(shell, document, object))
}

/// [`pickable`] for a Scene Item. With `protect` false, locked objects count too, as an
/// explicit pick list or `Mod`+A offers them.
pub fn item_pickable(shell: &Shell, document: Uuid, item: &Item, protect: bool) -> bool {
    item_allowed(shell, document, item)
        && !(protect
            && protects_locks(shell)
            && child_locked(shell, document, item.object, item.child.as_ref()))
}

/// How a rectangle dragged from `from` to `to` (Document nm) picks, following the
/// Directional Rectangle Preference. The direction is the one seen on screen, so a board
/// seen from below keeps left to right enclosing.
pub fn box_mode(shell: &Shell, document: Uuid, from: (f64, f64), to: (f64, f64)) -> BoxMode {
    if !shell.preferences().editing.directional_rectangle {
        return BoxMode::Enclose;
    }
    let mirrored = shell.document_tab(document).is_some_and(|t| t.mirrored());
    let (from, to) = if mirrored { (to, from) } else { (from, to) };
    BoxMode::of_drag(from, to)
}
