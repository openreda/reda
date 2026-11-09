//! The canvas context menu (docs/spec/v0.1/app.md#window-shell): a data file per Document
//! type, `context_menus/<type>.json`, in the spec's order, following the leading suite's
//! object-first menu:
//!
//! 1. the object's own submenu, named for it (Track, Zone GND, R12), or **Document
//!    properties** on an empty canvas
//! 2. Cut, Copy, Paste, Duplicate and Delete
//! 3. Select: Similar, Connected and All on Layer
//! 4. Align, with two or more selected
//! 5. Highlight Net, Clear highlight, Cross-probe and Applicable rules
//! 6. Place
//! 7. Properties, which focuses the Panel
//!
//! On a Violation marker it starts with Waive and Set severity.
//!
//! A file is a list of sections, each named by its slot (`violation`, `object`,
//! `document`, `edit`, `select`, `align`, `net`, `place`, `properties`), in that order.
//! A section's items are bar items ([`crate::bars`]): they bind only to Commands, Presets
//! and raw `{command, params}`, with separators and submenus, and no callbacks. A section
//! with a `title` shows as a submenu. The `object` section may add items per Selection
//! Filter kind under `kinds`. Two placeholders are filled from the Violation the menu
//! opened on: `$violation` (`{check, objects}`, as the waive Commands take it) and
//! `$check` (its check id).
//!
//! ```json
//! { "sections": [
//!     { "id": "object", "items": [ … ], "kinds": { "footprint_instance": [ … ] } },
//!     { "id": "document", "items": [ { "id": "context.document_properties", … } ] },
//!     { "id": "select", "title": "context-select", "items": [ … ] } ] }
//! ```

use std::collections::BTreeMap;
use std::sync::OnceLock;

use egui::{Id, Ui};
use reda_engine::model::{Object, Uuid};
use serde_json::{Value, json};

use crate::bars::data::{items, only};
use crate::bars::draw::{self, Invocation};
use crate::bars::{Binding, DocumentType, Item};
use crate::canvas::CanvasInputs;
use crate::messages::{CheckKind, LineKey};
use crate::shell::Shell;
use crate::tr;

/// The sections in the order the menu shows them: the spec's order. `object` and
/// `document` share the first slot; `violation` comes before it on a Violation marker.
pub const SECTION_ORDER: [&str; 9] = [
    "violation",
    "object",
    "document",
    "edit",
    "select",
    "align",
    "net",
    "place",
    "properties",
];

/// One section of a context menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// Its slot, one of [`SECTION_ORDER`].
    pub id: String,
    /// The catalogue key of its submenu's title; `None` shows the items inline.
    pub title: Option<String>,
    pub items: Vec<Item>,
    /// More items by Selection Filter kind (the `object` section only).
    pub kinds: BTreeMap<String, Vec<Item>>,
}

/// A Document type's canvas context menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextMenu {
    pub sections: Vec<Section>,
}

impl ContextMenu {
    /// The section in slot `id`, if the file has one.
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }
}

/// Reads one context menu data file. Sections must come in [`SECTION_ORDER`], each once;
/// anything else, such as a callback field, is refused.
pub fn parse(text: &str) -> Result<ContextMenu, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let root = value.as_object().ok_or("the file must be an object")?;
    only(root, &["sections"], "the file")?;
    let list = root
        .get("sections")
        .and_then(Value::as_array)
        .ok_or("the file needs a list \"sections\"")?;
    let mut sections: Vec<Section> = Vec::new();
    let mut last = None;
    for value in list {
        let map = value.as_object().ok_or("a section must be an object")?;
        let id = map
            .get("id")
            .and_then(Value::as_str)
            .ok_or("a section needs a string \"id\"")?
            .to_owned();
        let rank = SECTION_ORDER
            .iter()
            .position(|s| *s == id)
            .ok_or_else(|| format!("{id:?} is not a context menu section"))?;
        if last.is_some_and(|l| rank <= l) {
            return Err(format!(
                "{id:?} is out of order; sections go {}",
                SECTION_ORDER.join(", ")
            ));
        }
        last = Some(rank);
        let allowed: &[&str] = if id == "object" {
            &["id", "title", "items", "kinds"]
        } else {
            &["id", "title", "items"]
        };
        only(map, allowed, &id)?;
        let title = match map.get("title") {
            None => None,
            Some(Value::String(t)) => Some(t.clone()),
            Some(_) => return Err(format!("{id}: \"title\" must be a string")),
        };
        let own = items(map.get("items").ok_or(format!("{id} has no items"))?, &id)?;
        let kinds = match map.get("kinds") {
            None => BTreeMap::new(),
            Some(kinds) => kinds
                .as_object()
                .ok_or(format!("{id}: \"kinds\" must be an object"))?
                .iter()
                .map(|(kind, v)| Ok((kind.clone(), items(v, kind)?)))
                .collect::<Result<_, String>>()?,
        };
        sections.push(Section {
            id,
            title,
            items: own,
            kinds,
        });
    }
    Ok(ContextMenu { sections })
}

fn file(ty: DocumentType) -> &'static str {
    match ty {
        DocumentType::Schematic => include_str!("../context_menus/schematic.json"),
        DocumentType::Pcb => include_str!("../context_menus/pcb.json"),
        DocumentType::Symbol => include_str!("../context_menus/symbol.json"),
        DocumentType::Footprint => include_str!("../context_menus/footprint.json"),
    }
}

/// The context menu of a Document type. The files are part of the binary, so one that
/// does not parse is a bug and panics.
pub fn for_type(ty: DocumentType) -> &'static ContextMenu {
    static MENUS: OnceLock<Vec<ContextMenu>> = OnceLock::new();
    let all = MENUS.get_or_init(|| {
        DocumentType::ALL
            .into_iter()
            .map(|ty| parse(file(ty)).unwrap_or_else(|e| panic!("context menu for {ty:?}: {e}")))
            .collect()
    });
    &all[DocumentType::ALL
        .iter()
        .position(|t| *t == ty)
        .expect("listed")]
}

/// What a right-click landed on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Target {
    /// The Violation whose marker it was on.
    pub violation: Option<LineKey>,
    /// The object under the pointer, or else the first selected one.
    pub object: Option<Uuid>,
}

/// One entry of the menu as shown: a submenu or a row of items, between separators.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// The items inline.
    Items(Vec<Item>),
    /// A submenu with this title.
    Submenu(String, Vec<Item>),
}

/// The menu `ty`'s canvas shows for `target`, in order, with what does not apply left
/// out: the Violation section off a marker, the object's submenu or Document properties,
/// and Align with fewer than two selected.
pub fn shown(shell: &Shell, document: Uuid, ty: DocumentType, target: &Target) -> Vec<Shown> {
    let menu = for_type(ty);
    let selected = shell.selection().len();
    let mut out = Vec::new();
    for section in &menu.sections {
        let items = match section.id.as_str() {
            "violation" if target.violation.is_none() => continue,
            "object" => {
                let Some(object) = target.object else {
                    continue;
                };
                let mut items = section.items.clone();
                let kind = crate::selection::rules::kind(shell, document, object);
                if let Some(more) = kind.and_then(|k| section.kinds.get(k.name())) {
                    if !items.is_empty() && !more.is_empty() {
                        items.push(Item::Separator);
                    }
                    items.extend(more.iter().cloned());
                }
                out.push(Shown::Submenu(object_name(shell, document, object), items));
                continue;
            }
            "document" if target.object.is_some() => continue,
            "align" if selected < 2 => continue,
            _ => section.items.clone(),
        };
        let items = match target
            .violation
            .and_then(|key| violation_params(shell, key))
        {
            Some((violation, check)) => items
                .into_iter()
                .map(|i| fill_item(i, &violation, &check))
                .collect(),
            None => items,
        };
        out.push(match &section.title {
            Some(title) => Shown::Submenu(crate::i18n::translate(title, &[]), items),
            None => Shown::Items(items),
        });
    }
    out
}

/// What names an object in the menu: a Component's or Footprint's Designator (R12), a
/// Zone with its Net (Zone GND), else its kind (Track).
pub fn object_name(shell: &Shell, document: Uuid, object: Uuid) -> String {
    let kind = crate::selection::rules::kind(shell, document, object)
        .map(crate::selection::filter_ui::kind_label)
        .unwrap_or_else(|| tr!("context-object"));
    let Some(project) = shell.engine().project() else {
        return kind;
    };
    match project.object(object) {
        Some(Object::Unit(unit)) if !unit.object.designator.is_empty() => {
            unit.object.designator.clone()
        }
        Some(Object::FootprintInstance(f)) if !f.object.designator.is_empty() => {
            f.object.designator.clone()
        }
        Some(Object::Zone(zone)) => {
            let net = zone
                .object
                .net
                .and_then(|n| project.pcb()?.net(n))
                .map(|n| n.name.clone());
            match net {
                Some(net) => format!("{kind} {net}"),
                None => kind,
            }
        }
        _ => kind,
    }
}

/// The Violation's `{check, objects}` and check id, for `$violation` and `$check`.
fn violation_params(shell: &Shell, key: LineKey) -> Option<(Value, Value)> {
    let entry = shell.messages().entry(key)?;
    let v = &entry.violation;
    Some((
        json!({ "check": v.check, "objects": v.objects }),
        json!(v.check),
    ))
}

fn fill_item(item: Item, violation: &Value, check: &Value) -> Item {
    match item {
        Item::Action(mut action) => {
            if let Binding::Raw { params, .. } = &mut action.binding {
                fill(params, violation, check);
            }
            Item::Action(action)
        }
        Item::Submenu(mut menu) => {
            menu.items = menu
                .items
                .into_iter()
                .map(|i| fill_item(i, violation, check))
                .collect();
            Item::Submenu(menu)
        }
        other => other,
    }
}

fn fill(params: &mut Value, violation: &Value, check: &Value) {
    match params {
        Value::String(s) if s == "$violation" => *params = violation.clone(),
        Value::String(s) if s == "$check" => *params = check.clone(),
        Value::Array(items) => items.iter_mut().for_each(|v| fill(v, violation, check)),
        Value::Object(map) => map.values_mut().for_each(|v| fill(v, violation, check)),
        _ => {}
    }
}

/// What a right-click at `screen` on `input`'s canvas of `document` lands on: a Violation
/// marker within its radius, else the object under the pointer, else the first selected
/// object.
pub fn target_at(
    shell: &Shell,
    document: Uuid,
    input: &CanvasInputs,
    screen: egui::Pos2,
) -> Target {
    let violation = shell.markers(document).iter().find_map(|m| {
        let at = input.screen_point((m.at.x.0 as f64, m.at.y.0 as f64));
        (at.distance(screen) <= crate::canvas::MARKER_RADIUS * 1.4).then_some(m.key)
    });
    let at = input.document_point(screen);
    let tolerance = 5.0 * input.camera.nm_per_point;
    let object = crate::selection::picking::hit(shell, document, at, tolerance);
    Target { violation, object }
}

/// The canvas context menu of `document`, a Document of type `ty`: a right-click without
/// dragging works out what it landed on (selecting an object under the pointer that is
/// not selected yet, or clearing the selection on an empty spot) and opens the menu, whose
/// clicked item runs through the Shell like any other Command.
pub fn canvas(ui: &Ui, shell: &mut Shell, document: Uuid, ty: DocumentType, input: &CanvasInputs) {
    let id = Id::NULL.with(("canvas-context-menu", document));
    if input.response.secondary_clicked()
        && let Some(screen) = input.response.interact_pointer_pos()
    {
        let target = target_at(shell, document, input, screen);
        match target.object {
            Some(object) if !selected(shell, object) => {
                let selection = crate::selection::picking::selected(shell, document, &[object]);
                let (cmd, params) = crate::selection::picking::set_command(&selection);
                let _ = shell.run_reporting(&cmd, params);
            }
            None if target.violation.is_none() && !shell.selection().is_empty() => {
                let _ = shell.run_reporting("selection.set", json!({}));
            }
            _ => {}
        }
        ui.data_mut(|d| d.insert_temp(id, target));
    }
    let target: Target = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let mut clicked: Option<Invocation> = None;
    input.response.context_menu(|ui| {
        let checked = BTreeMap::new();
        let cx = draw::Cx {
            shell,
            checked: &checked,
            document: Some(document),
        };
        for (i, entry) in shown(shell, document, ty, &target).iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            match entry {
                Shown::Items(items) => draw::menu_items(ui, &cx, items, &mut clicked),
                Shown::Submenu(title, items) => {
                    ui.menu_button(title.as_str(), |ui| {
                        draw::menu_items(ui, &cx, items, &mut clicked);
                    });
                }
            }
        }
    });
    if let Some((cmd, params)) = clicked {
        run(shell, &cmd, params);
    }
}

/// Whether `object` is selected, as an object or as the owner of a selected part.
fn selected(shell: &Shell, object: Uuid) -> bool {
    let s = shell.selection();
    s.objects.contains(&object)
        || s.pads.iter().any(|p| p.footprint == object)
        || s.texts.iter().any(|t| t.footprint == object)
        || s.fields.iter().any(|f| f.unit == object)
}

/// Runs a clicked item. A Waiver or severity change runs its check again, as the Messages
/// panel's menu does, so the markers and the list follow.
fn run(shell: &mut Shell, cmd: &str, params: Value) {
    if shell.run_reporting(cmd, params).is_err() {
        return;
    }
    let base = cmd.split(':').next().unwrap_or(cmd);
    let kind = match base.split_once('.') {
        Some(("erc", "waive" | "set_severity")) => CheckKind::Erc,
        Some(("drc", "waive" | "set_severity")) => CheckKind::Drc,
        _ => return,
    };
    let _ = shell.run_check(kind);
}
