//! Menus, toolbars and the Active Bar, as data (docs/spec/v0.1/app.md#window-shell).
//!
//! `bars/common.json` holds the menus and toolbar every context shows; `bars/<type>.json`
//! adds what one Document type shows when one of its Documents is the active Document, and
//! that type's Active Bar. An item has a stable id and binds only to a Command
//! (`"project.save"`), a Preset (`"panel.show:layers"`) or a raw `{command, params}`; the
//! rest is separators and submenus. There are no callbacks: clicking an item runs its
//! binding through the Shell like any other Command, so it reaches the Journal.
//!
//! [`resolve`] turns an item into what the window shows: its title (looked up by Command
//! id, see [`command_title`]), its icon key, and either the Command to run or the reason it
//! cannot run now, which greys it out. [`draw`] draws the bars; [`commands`] holds the view
//! Commands the menus need (dialogs, Quit, Zoom to Fit, Theme).

pub mod commands;
mod data;
pub mod draw;

use std::sync::OnceLock;

use reda_engine::model::{DocumentKind, Scope, Uuid};
use serde_json::{Map, Value, json};

use crate::shell::Shell;
use crate::{i18n, tr};

pub(crate) use commands::request;
pub use commands::{Request, install, register_all, take_requests};
pub use data::{MENU_ORDER, parse};

/// What a bar item binds to.
#[derive(Clone, Debug, PartialEq)]
pub enum Binding {
    /// A Command run with no params.
    Command(String),
    /// A Preset, `<command id>:<name>`.
    Preset(String),
    /// A Command with fixed params. A string param [`ACTIVE_DOCUMENT`] stands for the
    /// active Document's UUID.
    Raw { command: String, params: Value },
}

/// The raw-params placeholder for the active Document's UUID.
pub const ACTIVE_DOCUMENT: &str = "$active_document";

/// The raw-params placeholder for the undo stack focus picks ([`Shell::undo_scope`]): the
/// focused Document's UUID, or `"project"`. The Edit menu's and toolbar's Undo and Redo
/// use it, so they act on the same stack as Mod+Z.
pub const UNDO_STACK: &str = "$undo_stack";

/// The raw-params placeholder for the selected objects on the active Document, as an
/// array of UUIDs. An item using it is greyed while nothing there is selected.
pub const SELECTION: &str = "$selection";

/// The raw-params placeholder for what the last copy or cut returned (Paste). An item
/// using it is greyed while the clipboard is empty.
pub const CLIPBOARD: &str = "$clipboard";

/// The raw-params placeholder for the active Document's snap grid, in nm.
pub const GRID: &str = "$grid";

/// The raw-params placeholder for everything selectable on the active Document, as an
/// array of UUIDs (Select All).
pub const SELECTABLE: &str = "$selectable";

/// The raw-params placeholder for where the pointer last was over the active Document's
/// canvas, snapped to its grid, as `{x, y}` in nm (Paste puts things there). A field
/// holding it is left out while the pointer has not been over the canvas.
pub const CURSOR: &str = "$cursor";

/// The raw-params placeholder for the board's active Layer, by UUID (Paste Special). An
/// item using it is greyed while no board is open.
pub const ACTIVE_LAYER: &str = "$active_layer";

/// One clickable item.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// Stable id, such as `file.save_all`.
    pub id: String,
    pub binding: Binding,
    /// A catalogue key to show instead of the Command's title.
    pub title: Option<String>,
    /// An icon key to show instead of the Command's icon.
    pub icon: Option<String>,
}

/// A menu bar entry or a submenu.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub id: String,
    /// The catalogue key of its title.
    pub title: String,
    /// The icon key a submenu shows as a bar button, which opens the menu under it. With
    /// none, a bar shows the title.
    pub icon: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Separator,
    Submenu(Menu),
    Action(Action),
}

/// The menus, the toolbar and the Active Bar of one context.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bars {
    pub menus: Vec<Menu>,
    pub toolbar: Vec<Item>,
    pub active_bar: Vec<Item>,
}

/// The Document types the bars are listed for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocumentType {
    Schematic,
    Pcb,
    Symbol,
    Footprint,
}

impl DocumentType {
    pub const ALL: [DocumentType; 4] = [
        DocumentType::Schematic,
        DocumentType::Pcb,
        DocumentType::Symbol,
        DocumentType::Footprint,
    ];

    fn file(self) -> &'static str {
        match self {
            DocumentType::Schematic => include_str!("../bars/schematic.json"),
            DocumentType::Pcb => include_str!("../bars/pcb.json"),
            DocumentType::Symbol => include_str!("../bars/symbol.json"),
            DocumentType::Footprint => include_str!("../bars/footprint.json"),
        }
    }
}

impl From<DocumentKind> for DocumentType {
    fn from(kind: DocumentKind) -> DocumentType {
        match kind {
            DocumentKind::Schematic => DocumentType::Schematic,
            DocumentKind::Pcb => DocumentType::Pcb,
        }
    }
}

const COMMON: &str = include_str!("../bars/common.json");

/// The bars for the active Document's type, or for no Document. The data files are part
/// of the binary, so a file that does not parse is a bug and panics.
pub fn for_document(document: Option<DocumentType>) -> &'static Bars {
    static BARS: OnceLock<Vec<Bars>> = OnceLock::new();
    let all = BARS.get_or_init(|| {
        let common = parse(COMMON).unwrap_or_else(|e| panic!("bars/common.json: {e}"));
        let mut all = vec![common.clone()];
        for ty in DocumentType::ALL {
            let own = parse(ty.file()).unwrap_or_else(|e| panic!("bars for {ty:?}: {e}"));
            all.push(data::merge(common.clone(), own));
        }
        all
    });
    let index = document.map_or(0, |ty| 1 + ty as usize);
    &all[index]
}

/// The type of the active Document, the one last interacted with in any window.
pub fn active_type(shell: &Shell) -> Option<DocumentType> {
    document_type(shell, shell.active_document()?.document)
}

/// The type of the Document, or open Library element, in the tab `document`.
pub fn document_type(shell: &Shell, document: Uuid) -> Option<DocumentType> {
    let entry = shell
        .engine()
        .project()
        .and_then(|p| p.documents().find(|d| d.uuid == document));
    match entry {
        Some(entry) => Some(entry.kind.into()),
        None => crate::element_editor::document_type(shell.engine(), document),
    }
}

/// An item as the window shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub title: String,
    pub icon: String,
    /// The Command's description, for the tooltip.
    pub description: String,
    /// The Command id without any Preset, and the params it would run with.
    pub command: String,
    pub params: Value,
    /// What to run, as `(id, params)` for [`Shell::run`], or why it cannot run now.
    pub run: Result<(String, Value), String>,
}

/// The title of a Command or Preset id: the catalogue's `command-<id>-title` (with `.` and
/// `:` written as `-`), or else the engine's English title.
pub fn command_title(id: &str, engine_title: &str) -> String {
    let key = format!("command-{}-title", id.replace(['.', ':'], "-"));
    if i18n::has_key(&key) {
        i18n::translate(&key, &[])
    } else {
        engine_title.to_owned()
    }
}

/// What `action` shows and runs on `shell` now.
pub fn resolve(shell: &Shell, action: &Action) -> Resolved {
    resolve_for(shell, action, shell.active_document().map(|d| d.document))
}

/// [`resolve`], with [`ACTIVE_DOCUMENT`] standing for `document`: an Active Bar acts on
/// its own canvas's Document.
pub fn resolve_for(shell: &Shell, action: &Action, document: Option<Uuid>) -> Resolved {
    resolve_in(shell, action, document, None)
}

/// [`resolve_for`], with a dialog open whose changes go on the `dialog` stack:
/// [`UNDO_STACK`] then stands for that stack, as [`Shell::undo_scope`] says.
pub fn resolve_in(
    shell: &Shell,
    action: &Action,
    document: Option<Uuid>,
    dialog: Option<Scope>,
) -> Resolved {
    let registry = shell.engine().registry();
    let (mut run_id, mut command, mut params) = match &action.binding {
        Binding::Command(id) => (id.clone(), id.clone(), json!({})),
        Binding::Preset(id) => {
            let command = id.split(':').next().unwrap_or(id).to_owned();
            (id.clone(), command, json!({}))
        }
        Binding::Raw { command, params } => (command.clone(), command.clone(), params.clone()),
    };
    if deletes_text_alone(shell, &command, &params) {
        run_id = "selection.delete".into();
        command = run_id.clone();
        params = json!({});
    }
    let Some(meta) = registry.get(&command) else {
        return Resolved {
            title: run_id.clone(),
            icon: String::new(),
            description: String::new(),
            command,
            params,
            run: Err(tr!("bars-unknown-command", command = run_id)),
        };
    };
    let mut title = command_title(&command, meta.title);
    let mut icon = meta.icon.to_owned();
    let mut shown_params = params.clone();
    match &action.binding {
        Binding::Preset(id) => {
            let presets = registry.describe(&command).unwrap_or_default();
            let name = id.split_once(':').map_or("", |(_, n)| n);
            if let Some(preset) = presets["presets"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["id"] == id.as_str() || p["id"] == name)
            {
                title = command_title(id, preset["title"].as_str().unwrap_or(meta.title));
                icon = preset["icon"].as_str().unwrap_or(meta.icon).to_owned();
                shown_params = preset["params"].clone();
            }
        }
        Binding::Raw { .. } => {
            let summary = summary(&params);
            if !summary.is_empty() {
                title = format!("{title} ({summary})");
            }
        }
        Binding::Command(_) => {}
    }
    if let Some(key) = &action.title {
        title = i18n::translate(key, &[]);
    }
    if let Some(key) = &action.icon {
        icon = key.clone();
    }
    let active = document.map(|d| d.to_string());
    let undo = shell.undo_scope(dialog).map(|scope| match scope {
        Scope::Document(uuid) => uuid.to_string(),
        _ => "project".to_owned(),
    });
    let undo = undo.map(Value::from);
    let active = active.map(Value::from);
    let grid = document.map(|d| json!(shell.grid(d).0));
    let selectable = document.map(|d| json!(selectable_on(shell, d)));
    let cursor = document.and_then(|d| {
        let at = shell.document_tab(d)?.cursor?;
        let at = shell.snap_to_grid(d, at);
        Some(json!({ "x": at.x.0, "y": at.y.0 }))
    });
    if cursor.is_none() {
        drop_placeholder(&mut params, CURSOR);
    }
    let active_layer = shell
        .engine()
        .project()
        .and_then(|p| shell.board_view().active_layer(p))
        .map(|l| json!(l));
    let selected = document
        .map(|d| selected_on(shell, d))
        .filter(|s| !s.is_empty())
        .map(|s| json!(s));
    let select_all = command == "selection.set" && params["objects"].as_str() == Some(SELECTABLE);
    let filled = fill(&mut params, UNDO_STACK, undo.as_ref())
        .and_then(|()| fill(&mut params, ACTIVE_DOCUMENT, active.as_ref()))
        .and_then(|()| fill(&mut params, GRID, grid.as_ref()))
        .and_then(|()| fill(&mut params, SELECTABLE, selectable.as_ref()))
        .and_then(|()| fill(&mut params, CURSOR, cursor.as_ref()))
        .and_then(|()| fill(&mut params, ACTIVE_LAYER, active_layer.as_ref()))
        .map_err(|()| tr!("bars-no-active-document"))
        .and_then(|()| {
            fill(&mut params, SELECTION, selected.as_ref())
                .map_err(|()| tr!("bars-nothing-selected"))
        })
        .and_then(|()| {
            fill(&mut params, CLIPBOARD, shell.clipboard())
                .map_err(|()| tr!("bars-clipboard-empty"))
        });
    let run = match filled {
        Err(reason) => Err(reason),
        Ok(()) => {
            if select_all && let Some(document) = document {
                params = serde_json::to_value(crate::selection::picking::all(shell, document))
                    .expect("selection serializes");
            }
            // The window is never headless.
            match shell.engine().available(&run_id, false) {
                Ok(()) => Ok((run_id, params.clone())),
                Err(e) => Err(e.message),
            }
        }
    };
    if matches!(action.binding, Binding::Raw { .. }) {
        shown_params = params.clone();
    }
    Resolved {
        title,
        icon,
        description: meta.description.to_owned(),
        command,
        params: shown_params,
        run,
    }
}

/// The selected objects on `document`, in selection order.
fn selected_on(shell: &Shell, document: Uuid) -> Vec<Uuid> {
    if crate::element_editor::is_element(shell.engine(), document) {
        return crate::element_editor::selected_on(shell, document);
    }
    let Some(project) = shell.engine().project() else {
        return Vec::new();
    };
    shell
        .selection()
        .objects
        .iter()
        .copied()
        .filter(|o| {
            project
                .object(*o)
                .is_some_and(|o| o.scope() == Scope::Document(document))
        })
        .collect()
}

/// Everything selectable on `document`, sorted.
fn selectable_on(shell: &Shell, document: Uuid) -> Vec<Uuid> {
    if crate::element_editor::is_element(shell.engine(), document) {
        return crate::element_editor::parts(shell.engine(), document)
            .into_iter()
            .filter(|o| crate::selection::rules::allowed(shell, document, *o))
            .collect();
    }
    let Some(project) = shell.engine().project() else {
        return Vec::new();
    };
    if let Some(tab) = shell
        .document_tab(document)
        .filter(|_| project.pcb_document(document).is_some())
    {
        let mut out = Vec::new();
        for item in &tab.scene.scene.items {
            if crate::selection::rules::allowed(shell, document, item.object)
                && !out.contains(&item.object)
            {
                out.push(item.object);
            }
        }
        return out;
    }
    let objects = project
        .schematic_sheet(document)
        .map(crate::schematic_editor::selectable)
        .unwrap_or_default();
    objects
        .into_iter()
        .filter(|o| crate::selection::rules::allowed(shell, document, *o))
        .collect()
}

/// Removes every object field whose value is `placeholder`, for an optional one.
/// A delete of the selection (`$selection`) when only Field or Designator text is
/// selected: that text can't be deleted alone, so `selection.delete` refuses instead. With
/// objects selected too, the delete goes ahead on them.
fn deletes_text_alone(shell: &Shell, command: &str, params: &Value) -> bool {
    let selection = shell.selection();
    command.ends_with(".delete")
        && mentions(params, SELECTION)
        && selection.objects.is_empty()
        && (!selection.fields.is_empty() || !selection.texts.is_empty())
}

/// Whether `placeholder` is one of `params`' values, at any depth.
fn mentions(params: &Value, placeholder: &str) -> bool {
    match params {
        Value::String(s) => s == placeholder,
        Value::Array(items) => items.iter().any(|v| mentions(v, placeholder)),
        Value::Object(map) => map.values().any(|v| mentions(v, placeholder)),
        _ => false,
    }
}

fn drop_placeholder(params: &mut Value, placeholder: &str) {
    match params {
        Value::Array(items) => items
            .iter_mut()
            .for_each(|v| drop_placeholder(v, placeholder)),
        Value::Object(map) => {
            map.retain(|_, v| v.as_str() != Some(placeholder));
            map.values_mut()
                .for_each(|v| drop_placeholder(v, placeholder));
        }
        _ => {}
    }
}

/// Replaces every `placeholder` string in `params` with `value`; `Err` when one is there
/// but there is no value (no Document is active, nothing is selected, the clipboard is
/// empty).
fn fill(params: &mut Value, placeholder: &str, value: Option<&Value>) -> Result<(), ()> {
    match params {
        Value::String(s) if s == placeholder => {
            *params = value.ok_or(())?.clone();
            Ok(())
        }
        Value::Array(items) => items
            .iter_mut()
            .try_for_each(|v| fill(v, placeholder, value)),
        Value::Object(map) => map
            .values_mut()
            .try_for_each(|v| fill(v, placeholder, value)),
        _ => Ok(()),
    }
}

/// A raw binding's params in short: `key: value, …`, strings unquoted. Params that are
/// [`ACTIVE_DOCUMENT`] or [`UNDO_STACK`] are context, not a choice, so they are left out.
fn summary(params: &Value) -> String {
    fn value(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
    match params {
        Value::Object(map) => summary_map(map, value),
        other => value(other),
    }
}

fn summary_map(map: &Map<String, Value>, value: fn(&Value) -> String) -> String {
    map.iter()
        .filter(|(_, v)| {
            !matches!(
                v.as_str(),
                Some(
                    ACTIVE_DOCUMENT
                        | UNDO_STACK
                        | SELECTION
                        | CLIPBOARD
                        | GRID
                        | SELECTABLE
                        | CURSOR
                        | ACTIVE_LAYER
                )
            )
        })
        .map(|(k, v)| format!("{k}: {}", value(v)))
        .collect::<Vec<_>>()
        .join(", ")
}
