//! Reading the bar data files. The format allows only what the spec allows: items that bind
//! to a Command, a Preset or a raw `{command, params}`, separators and submenus. Anything
//! else, such as a callback field, is refused.
//!
//! ```json
//! { "menus": [{ "id": "file", "title": "menu-file", "items": [
//!       { "id": "file.save_all", "command": "project.save" },
//!       { "id": "view.panels.layers", "command": "panel.show:layers" },
//!       { "id": "edit.undo", "command": "edit.undo", "params": { "document": "$active_document" } },
//!       "separator",
//!       { "id": "view.theme", "title": "menu-view-theme", "items": [ … ] } ] }],
//!   "toolbar": [ … ],
//!   "active_bar": [ … ] }
//! ```

use serde_json::{Map, Value};

use super::{Action, Bars, Binding, Item, Menu};

/// Reads one data file.
pub fn parse(text: &str) -> Result<Bars, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let root = object(&value, "the file")?;
    only(root, &["menus", "toolbar", "active_bar"], "the file")?;
    let menus = match root.get("menus") {
        None => Vec::new(),
        Some(v) => array(v, "menus")?
            .iter()
            .map(|m| {
                let map = object(m, "a menu")?;
                only(map, &["id", "title", "items"], "a menu")?;
                menu(map)
            })
            .collect::<Result<_, _>>()?,
    };
    let bar = |key: &str| match root.get(key) {
        None => Ok(Vec::new()),
        Some(v) => items(v, key),
    };
    Ok(Bars {
        menus,
        toolbar: bar("toolbar")?,
        active_bar: bar("active_bar")?,
    })
}

/// The common bars with one Document type's: a menu with the id of a common menu adds its
/// items to that menu, other menus follow the common ones, and the toolbar's own items
/// follow the common toolbar after a separator.
pub fn merge(mut common: Bars, own: Bars) -> Bars {
    for menu in own.menus {
        match common.menus.iter_mut().find(|m| m.id == menu.id) {
            Some(existing) => {
                existing.items.push(Item::Separator);
                existing.items.extend(menu.items);
            }
            // Help stays last, as on every platform.
            None => {
                let at = common
                    .menus
                    .iter()
                    .position(|m| m.id == "help")
                    .unwrap_or(common.menus.len());
                common.menus.insert(at, menu);
            }
        }
    }
    if !own.toolbar.is_empty() {
        common.toolbar.push(Item::Separator);
        common.toolbar.extend(own.toolbar);
    }
    common.active_bar.extend(own.active_bar);
    common
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

fn items(value: &Value, what: &str) -> Result<Vec<Item>, String> {
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
    only(map, &["id", "command", "params", "title"], &id)?;
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
    Ok(Item::Action(Action { id, binding, title }))
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
fn only(map: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    match map.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(format!("{what}: unknown field {k:?}")),
        None => Ok(()),
    }
}
