//! Reading the bar data files. The format allows only what the spec allows: items that bind
//! to a Command, a Preset or a raw `{command, params}`, separators and submenus. An item
//! may name a `title` (a catalogue key) or an `icon` to show instead of its Command's.
//! Anything else, such as a callback field, is refused.
//!
//! ```json
//! { "menus": [{ "id": "file", "title": "menu-file", "items": [
//!       { "id": "file.save_all", "command": "project.save" },
//!       { "id": "window.panel.layers", "command": "panel.show:layers" },
//!       { "id": "edit.undo", "command": "edit.undo", "params": { "document": "$active_document" } },
//!       "separator",
//!       { "id": "view.theme", "title": "menu-view-theme", "items": [ … ] } ] }],
//!   "toolbar": [ … ],
//!   "active_bar": [ … ],
//!   "tool_bars": { "route": [ … ] } }
//! ```

use serde_json::{Map, Value};

use super::{Action, Bars, Binding, Item, Menu};

/// Reads one data file.
pub fn parse(text: &str) -> Result<Bars, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let root = object(&value, "the file")?;
    only(
        root,
        &["menus", "toolbar", "active_bar", "tool_bars"],
        "the file",
    )?;
    let menus = match root.get("menus") {
        None => Vec::new(),
        Some(v) => array(v, "menus")?
            .iter()
            .map(|m| {
                let map = object(m, "a menu")?;
                only(map, &["id", "title", "items"], "a menu")?;
                let menu = menu(map)?;
                if menu_rank(&menu.id) == MENU_ORDER.len() {
                    return Err(format!("{}: not a menu of the menu bar", menu.id));
                }
                Ok(menu)
            })
            .collect::<Result<_, _>>()?,
    };
    let bar = |key: &str| match root.get(key) {
        None => Ok(Vec::new()),
        Some(v) => items(v, key),
    };
    let tool_bars = match root.get("tool_bars") {
        None => Default::default(),
        Some(v) => object(v, "tool_bars")?
            .iter()
            .map(|(tool, items)| Ok((tool.clone(), self::items(items, tool)?)))
            .collect::<Result<_, String>>()?,
    };
    Ok(Bars {
        menus,
        toolbar: bar("toolbar")?,
        active_bar: bar("active_bar")?,
        tool_bars,
    })
}

/// The menus in the order the menu bar shows them, the leading suite's. A data file may
/// name no other menu.
pub const MENU_ORDER: [&str; 10] = [
    "file", "edit", "view", "project", "place", "design", "route", "tools", "window", "help",
];

/// The common bars with one Document type's. A menu with the id of a common menu adds its
/// items to that menu, after the common menu's first group; the menus then stand in
/// [`MENU_ORDER`]. A toolbar item with the id of a common one takes its place (a Library
/// editor's Save saves the element); the type's other toolbar items follow the common ones.
pub fn merge(mut common: Bars, own: Bars) -> Bars {
    for menu in own.menus {
        match common.menus.iter_mut().find(|m| m.id == menu.id) {
            Some(existing) => {
                let at = existing
                    .items
                    .iter()
                    .position(|i| matches!(i, Item::Separator))
                    .unwrap_or(existing.items.len());
                let mut added = vec![Item::Separator];
                added.extend(menu.items);
                existing.items.splice(at..at, added);
            }
            None => common.menus.push(menu),
        }
    }
    common.menus.sort_by_key(|m| menu_rank(&m.id));
    for item in own.toolbar {
        let id = |i: &Item| match i {
            Item::Action(a) => Some(a.id.clone()),
            _ => None,
        };
        let mine = id(&item);
        match common
            .toolbar
            .iter_mut()
            .find(|i| mine.is_some() && id(i) == mine)
        {
            Some(slot) => *slot = item,
            None => common.toolbar.push(item),
        }
    }
    common.active_bar.extend(own.active_bar);
    common.tool_bars.extend(own.tool_bars);
    common
}

fn menu_rank(id: &str) -> usize {
    MENU_ORDER
        .iter()
        .position(|m| *m == id)
        .unwrap_or(MENU_ORDER.len())
}

fn menu(map: &Map<String, Value>) -> Result<Menu, String> {
    let id = string(map, "id", "a menu")?;
    Ok(Menu {
        title: string(map, "title", &id)?,
        icon: match map.get("icon") {
            Some(_) => Some(string(map, "icon", &id)?),
            None => None,
        },
        items: items(map.get("items").ok_or(format!("{id} has no items"))?, &id)?,
        id,
    })
}

pub(crate) fn items(value: &Value, what: &str) -> Result<Vec<Item>, String> {
    array(value, what)?.iter().map(item).collect()
}

fn item(value: &Value) -> Result<Item, String> {
    if let Value::String(s) = value {
        return match s.as_str() {
            "separator" => Ok(Item::Separator),
            other => Err(format!(
                "{other:?} is not an item; the only bare item is \"separator\""
            )),
        };
    }
    let map = object(value, "an item")?;
    if map.contains_key("items") {
        only(map, &["id", "title", "icon", "items"], "a submenu")?;
        return Ok(Item::Submenu(menu(map)?));
    }
    let id = string(map, "id", "an item")?;
    only(map, &["id", "command", "params", "title", "icon"], &id)?;
    let command = string(map, "command", &id)?;
    let binding = match (map.get("params"), command.contains(':')) {
        (Some(_), true) => return Err(format!("{id}: a Preset has fixed params; give none")),
        (Some(params), false) => Binding::Raw {
            command,
            params: params.clone(),
        },
        (None, true) => Binding::Preset(command),
        (None, false) => Binding::Command(command),
    };
    let title = map
        .get("title")
        .map(|_| string(map, "title", &id))
        .transpose()?;
    let icon = map
        .get("icon")
        .map(|_| string(map, "icon", &id))
        .transpose()?;
    Ok(Item::Action(Action {
        id,
        binding,
        title,
        icon,
    }))
}

fn object<'v>(value: &'v Value, what: &str) -> Result<&'v Map<String, Value>, String> {
    value.as_object().ok_or(format!("{what} must be an object"))
}

fn array<'v>(value: &'v Value, what: &str) -> Result<&'v Vec<Value>, String> {
    value.as_array().ok_or(format!("{what} must be a list"))
}

fn string(map: &Map<String, Value>, key: &str, what: &str) -> Result<String, String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(format!("{what} needs a string {key:?}"))
}

/// Refuses fields other than `allowed`.
pub(crate) fn only(map: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    match map.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(format!("{what}: unknown field {k:?}")),
        None => Ok(()),
    }
}
