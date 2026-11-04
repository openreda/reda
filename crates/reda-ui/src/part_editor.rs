//! The Part editor: an open Part's tab (see [`element_editor`]), all form and no canvas.
//! It picks the Symbol and the ordered alternate Footprints (from every registered
//! Library, with previews and links into their own editors), edits the pin-map overrides,
//! the default fields and the Component Type.
//!
//! The functions here turn one edit of the form into the `library.part.*` Command that
//! makes it, on the Part's own undo stack; `None` when the edit changes nothing. The form
//! itself is drawn by [`view`].
//!
//! [`element_editor`]: crate::element_editor

pub(crate) mod view;

use std::cmp::Ordering;

use reda_engine::model::{
    ComponentType, Field, Footprint, LibraryElement, Part, PinPads, Symbol, Uuid,
};
use serde_json::json;

use crate::element_editor::Invocation;
use crate::shell::Shell;

/// A Symbol or Footprint the pickers offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub uuid: Uuid,
    /// `Library / Name`.
    pub label: String,
}

/// One Pin of the Part's Symbol in the pin-pad map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinRow {
    pub number: String,
    pub name: String,
    /// The Pads it reaches: its override's, else the Pad with its own number.
    pub pads: Vec<String>,
    /// Whether the Part overrides the implicit map for this Pin.
    pub overridden: bool,
}

/// How a listed alternate Footprint moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootprintMove {
    /// One place earlier; to the top, it becomes the default.
    Up,
    Down,
    Remove,
}

/// The built-in fields, by their `library.part.fields.set` param name.
pub const BUILT_IN: [&str; 6] = [
    "value",
    "description",
    "datasheet",
    "manufacturer",
    "mpn",
    "keywords",
];

/// Every Symbol in the registered Libraries, by label.
pub fn symbol_choices(shell: &mut Shell) -> Vec<Choice> {
    let index = shell.library_index();
    let mut choices: Vec<Choice> = index
        .libraries()
        .flat_map(|l| {
            l.symbols.values().map(|s| Choice {
                uuid: s.uuid,
                label: format!("{} / {}", l.info.name, s.name),
            })
        })
        .collect();
    choices.sort_by(|a, b| a.label.cmp(&b.label));
    choices
}

/// Every Footprint in the registered Libraries, by label.
pub fn footprint_choices(shell: &mut Shell) -> Vec<Choice> {
    let index = shell.library_index();
    let mut choices: Vec<Choice> = index
        .libraries()
        .flat_map(|l| {
            l.footprints.values().map(|f| Choice {
                uuid: f.uuid,
                label: format!("{} / {}", l.info.name, f.name),
            })
        })
        .collect();
    choices.sort_by(|a, b| a.label.cmp(&b.label));
    choices
}

/// The Symbol `uuid`: open (with its unsaved edits) or in a registered Library.
pub fn symbol(shell: &mut Shell, uuid: Uuid) -> Option<Symbol> {
    if let Some(LibraryElement::Symbol(s)) = shell.engine().element(uuid) {
        return Some(s.clone());
    }
    shell.library_index().symbol(uuid).cloned()
}

/// The Footprint `uuid`: open (with its unsaved edits) or in a registered Library.
pub fn footprint(shell: &mut Shell, uuid: Uuid) -> Option<Footprint> {
    if let Some(LibraryElement::Footprint(f)) = shell.engine().element(uuid) {
        return Some(f.clone());
    }
    shell.library_index().footprint(uuid).cloned()
}

/// Orders Pin and Pad numbers as people count them: `2` before `10`, numbers before
/// names.
pub fn by_number(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

/// The pin-pad map of `part`: one row per Pin of `symbol`, by number.
pub fn pin_rows(part: &Part, symbol: Option<&Symbol>) -> Vec<PinRow> {
    let Some(symbol) = symbol else {
        return Vec::new();
    };
    let mut rows: Vec<PinRow> = symbol
        .pins
        .iter()
        .map(|pin| PinRow {
            number: pin.number.clone(),
            name: pin.name.clone(),
            pads: part
                .pin_map
                .pads_of(&pin.number)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            overridden: part.pin_map.pins.iter().any(|o| o.pin == pin.number),
        })
        .collect();
    rows.sort_by(|a, b| by_number(&a.number, &b.number));
    rows.dedup_by(|a, b| a.number == b.number);
    rows
}

/// The Pads of `footprints` that no Pin of `symbol` reaches, plus those already marked
/// unconnected, by number: what may be marked or unmarked unconnected.
pub fn unmapped_pads(
    part: &Part,
    symbol: Option<&Symbol>,
    footprints: &[Footprint],
) -> Vec<String> {
    let reached: Vec<String> = pin_rows(part, symbol)
        .into_iter()
        .flat_map(|r| r.pads)
        .collect();
    let mut pads: Vec<String> = footprints
        .iter()
        .flat_map(|f| f.pads.iter().map(|p| p.number.clone()))
        .filter(|n| !n.is_empty() && !reached.contains(n))
        .chain(part.pin_map.unconnected.iter().cloned())
        .collect();
    pads.sort_by(|a, b| by_number(a, b));
    pads.dedup();
    pads
}

fn pin_map_set(part: &Part, pins: Vec<PinPads>, unconnected: Vec<String>) -> Invocation {
    (
        "library.part.pin_map.set".to_owned(),
        json!({"part": part.uuid, "pins": pins, "unconnected": unconnected}),
    )
}

/// Maps Pin `pin` to the Pads typed in `text` (separated by commas or spaces). Nothing,
/// or just the Pin's own number, restores the implicit map. Pads it now reaches are no
/// longer unconnected.
pub fn set_pin_pads(part: &Part, pin: &str, text: &str) -> Option<Invocation> {
    let mut pads: Vec<String> = Vec::new();
    for pad in text
        .split([',', ' '])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        if !pads.iter().any(|p| p == pad) {
            pads.push(pad.to_owned());
        }
    }
    let wanted = (!pads.is_empty() && pads != [pin]).then_some(pads);
    let current = part.pin_map.pins.iter().find(|o| o.pin == pin);
    if current.map(|o| &o.pads) == wanted.as_ref() {
        return None;
    }
    let mut pins: Vec<PinPads> = part
        .pin_map
        .pins
        .iter()
        .filter(|o| o.pin != pin)
        .cloned()
        .collect();
    let mut unconnected = part.pin_map.unconnected.clone();
    if let Some(pads) = wanted {
        unconnected.retain(|u| !pads.contains(u));
        pins.push(PinPads {
            pin: pin.to_owned(),
            pads,
        });
    } else {
        unconnected.retain(|u| u != pin);
    }
    Some(pin_map_set(part, pins, unconnected))
}

/// Marks Pad `pad` deliberately unconnected, or not.
pub fn set_unconnected(part: &Part, pad: &str, unconnected: bool) -> Option<Invocation> {
    let listed = part.pin_map.unconnected.iter().any(|u| u == pad);
    if listed == unconnected {
        return None;
    }
    let mut list: Vec<String> = part
        .pin_map
        .unconnected
        .iter()
        .filter(|u| *u != pad)
        .cloned()
        .collect();
    if unconnected {
        list.push(pad.to_owned());
    }
    Some(pin_map_set(part, part.pin_map.pins.clone(), list))
}

fn footprints_set(part: &Part, footprints: Vec<Uuid>) -> Invocation {
    (
        "library.part.footprints.set".to_owned(),
        json!({"part": part.uuid, "footprints": footprints}),
    )
}

/// Adds `footprint` as the last alternate (the default, if it is the first).
pub fn add_footprint(part: &Part, footprint: Uuid) -> Option<Invocation> {
    if part.footprints.contains(&footprint) {
        return None;
    }
    let mut list = part.footprints.clone();
    list.push(footprint);
    Some(footprints_set(part, list))
}

/// Moves or removes the alternate Footprint at `index`.
pub fn move_footprint(part: &Part, index: usize, how: FootprintMove) -> Option<Invocation> {
    let mut list = part.footprints.clone();
    if index >= list.len() {
        return None;
    }
    match how {
        FootprintMove::Up if index > 0 => list.swap(index, index - 1),
        FootprintMove::Down if index + 1 < list.len() => list.swap(index, index + 1),
        FootprintMove::Remove => {
            list.remove(index);
        }
        _ => return None,
    }
    Some(footprints_set(part, list))
}

/// Picks the Part's Symbol, or none.
pub fn set_symbol(part: &Part, symbol: Option<Uuid>) -> Option<Invocation> {
    (part.symbol != symbol).then(|| {
        (
            "library.part.symbol.set".to_owned(),
            json!({"part": part.uuid, "symbol": symbol}),
        )
    })
}

/// Renames the Part.
pub fn rename(part: &Part, name: &str) -> Option<Invocation> {
    let name = name.trim();
    (!name.is_empty() && name != part.name).then(|| {
        (
            "library.part.edit".to_owned(),
            json!({"part": part.uuid, "name": name}),
        )
    })
}

pub fn set_component_type(part: &Part, component_type: ComponentType) -> Option<Invocation> {
    (part.component_type != component_type).then(|| {
        (
            "library.part.edit".to_owned(),
            json!({"part": part.uuid, "component_type": component_type}),
        )
    })
}

/// The value of the built-in field `key` (one of [`BUILT_IN`]).
pub fn builtin(part: &Part, key: &str) -> String {
    let f = &part.fields;
    match key {
        "value" => &f.value,
        "description" => &f.description,
        "datasheet" => &f.datasheet,
        "manufacturer" => &f.manufacturer,
        "mpn" => &f.mpn,
        "keywords" => &f.keywords,
        _ => "",
    }
    .to_owned()
}

/// Sets the built-in field `key` (one of [`BUILT_IN`]); empty clears it.
pub fn set_builtin(part: &Part, key: &str, value: &str) -> Option<Invocation> {
    let value = value.trim();
    (BUILT_IN.contains(&key) && builtin(part, key) != value).then(|| {
        let mut params = json!({"part": part.uuid});
        params[key] = json!(value);
        ("library.part.fields.set".to_owned(), params)
    })
}

fn custom_set(part: &Part, custom: Vec<Field>) -> Invocation {
    (
        "library.part.fields.set".to_owned(),
        json!({"part": part.uuid, "custom": custom}),
    )
}

/// Adds an empty free-form field, called `Field 1` (or the next free number).
pub fn add_field(part: &Part) -> Invocation {
    let taken = |name: &str| {
        part.fields
            .custom
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case(name))
    };
    let name = (1..)
        .map(|n| format!("Field {n}"))
        .find(|n| !taken(n))
        .expect("some number is free");
    let mut custom = part.fields.custom.clone();
    custom.push(Field {
        name,
        value: String::new(),
    });
    custom_set(part, custom)
}

/// Renames the free-form field at `index` and/or sets its value.
pub fn set_field(
    part: &Part,
    index: usize,
    name: Option<&str>,
    value: Option<&str>,
) -> Option<Invocation> {
    let mut custom = part.fields.custom.clone();
    let field = custom.get_mut(index)?;
    let before = field.clone();
    if let Some(name) = name {
        field.name = name.trim().to_owned();
    }
    if let Some(value) = value {
        field.value = value.trim().to_owned();
    }
    (*field != before).then(|| custom_set(part, custom))
}

/// Removes the free-form field at `index`.
pub fn remove_field(part: &Part, index: usize) -> Invocation {
    let mut custom = part.fields.custom.clone();
    if index < custom.len() {
        custom.remove(index);
    }
    custom_set(part, custom)
}
