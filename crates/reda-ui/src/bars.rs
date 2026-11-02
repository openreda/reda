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

use reda_engine::model::{DocumentKind, Uuid};
use serde_json::{Map, Value, json};

use crate::shell::Shell;
use crate::{i18n, tr};

pub use commands::{Request, install, register_all, take_requests};
pub use data::parse;

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

/// One clickable item.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// Stable id, such as `file.save_all`.
    pub id: String,
    pub binding: Binding,
    /// A catalogue key to show instead of the Command's title.
    pub title: Option<String>,
}

/// A menu bar entry or a submenu.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub id: String,
    /// The catalogue key of its title.
    pub title: String,
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
    let document = shell.active_document()?.document;
    let entry = shell
        .engine()
        .project()?
        .documents()
        .find(|d| d.uuid == document)?;
    Some(entry.kind.into())
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
    let registry = shell.engine().registry();
    let (run_id, command, mut params) = match &action.binding {
        Binding::Command(id) => (id.clone(), id.clone(), json!({})),
        Binding::Preset(id) => {
            let command = id.split(':').next().unwrap_or(id).to_owned();
            (id.clone(), command, json!({}))
        }
        Binding::Raw { command, params } => (command.clone(), command.clone(), params.clone()),
    };
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
    let active = document.map(|d| d.to_string());
    let run = match fill_active_document(&mut params, active.as_deref()) {
        Err(()) => Err(tr!("bars-no-active-document")),
        Ok(()) => {
            // The window is never headless.
            match registry.available(&run_id, shell.engine().project(), false) {
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

/// Replaces every [`ACTIVE_DOCUMENT`] in `params`; `Err` when one is there but no Document
/// is active.
fn fill_active_document(params: &mut Value, active: Option<&str>) -> Result<(), ()> {
    match params {
        Value::String(s) if s == ACTIVE_DOCUMENT => {
            *s = active.ok_or(())?.to_owned();
            Ok(())
        }
        Value::Array(items) => items
            .iter_mut()
            .try_for_each(|v| fill_active_document(v, active)),
        Value::Object(map) => map
            .values_mut()
            .try_for_each(|v| fill_active_document(v, active)),
        _ => Ok(()),
    }
}

/// A raw binding's params in short: `key: value, …`, strings unquoted. Params that are
/// [`ACTIVE_DOCUMENT`] are context, not a choice, so they are left out.
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
        .filter(|(_, v)| v.as_str() != Some(ACTIVE_DOCUMENT))
        .map(|(k, v)| format!("{k}: {}", value(v)))
        .collect::<Vec<_>>()
        .join(", ")
}
