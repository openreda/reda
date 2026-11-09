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

/// The built-in fields the Part region shows (library.md#part-editor).
pub const PART_FIELDS: [&str; 2] = ["description", "keywords"];
/// The built-in fields the Fields grid starts with, before the free-form ones.
pub const FIELD_GRID: [&str; 4] = ["value", "datasheet", "manufacturer", "mpn"];

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

/// Retypes the Part; the Command resets In BOM and In CPL to the new type's defaults.
pub fn set_component_type(part: &Part, component_type: ComponentType) -> Option<Invocation> {
    (part.component_type != component_type).then(|| {
        (
            "library.part.edit".to_owned(),
            json!({"part": part.uuid, "component_type": component_type}),
        )
    })
}

/// Sets the Part's In BOM default.
pub fn set_in_bom(part: &Part, in_bom: bool) -> Option<Invocation> {
    (part.in_bom != in_bom).then(|| {
        (
            "library.part.edit".to_owned(),
            json!({"part": part.uuid, "in_bom": in_bom}),
        )
    })
}

/// Sets the Part's In CPL default.
pub fn set_in_cpl(part: &Part, in_cpl: bool) -> Option<Invocation> {
    (part.in_cpl != in_cpl).then(|| {
        (
            "library.part.edit".to_owned(),
            json!({"part": part.uuid, "in_cpl": in_cpl}),
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

/// Why a pin-map cell is marked red in the Pins Panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellMark {
    /// The Pin reaches no Pad of this Footprint.
    Empty,
    /// A Pad number this Footprint doesn't have.
    Unknown(String),
    /// A Pad another Pin reaches too.
    Shared(String),
}

/// One alternate Footprint's cell of a Pin's row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapCell {
    /// The Pads the Pin reaches, comma-separated.
    pub text: String,
    pub marks: Vec<CellMark>,
}

/// One Pin's row of the Pins Panel on a Part: one cell per alternate Footprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapRow {
    pub number: String,
    pub name: String,
    pub overridden: bool,
    pub cells: Vec<MapCell>,
}

/// The Pins Panel's pin-pad map of `part`, one cell per Footprint of `footprints` (its
/// alternates, in order). The map is the Part's one map; a cell is red when the Pin
/// reaches no Pad of that Footprint, names a Pad it doesn't have, or shares a Pad with
/// another Pin.
pub fn map_rows(part: &Part, symbol: Option<&Symbol>, footprints: &[Footprint]) -> Vec<MapRow> {
    let rows = pin_rows(part, symbol);
    let mut uses: std::collections::BTreeMap<&str, usize> = Default::default();
    for row in &rows {
        for pad in &row.pads {
            *uses.entry(pad.as_str()).or_default() += 1;
        }
    }
    rows.iter()
        .map(|row| MapRow {
            number: row.number.clone(),
            name: row.name.clone(),
            overridden: row.overridden,
            cells: footprints
                .iter()
                .map(|f| {
                    let has = |n: &str| f.pads.iter().any(|p| p.number == n);
                    let mut marks = Vec::new();
                    if row.pads.is_empty() {
                        marks.push(CellMark::Empty);
                    }
                    for pad in &row.pads {
                        if !has(pad) {
                            marks.push(CellMark::Unknown(pad.clone()));
                        }
                        if uses.get(pad.as_str()).copied().unwrap_or(0) > 1 {
                            marks.push(CellMark::Shared(pad.clone()));
                        }
                    }
                    MapCell {
                        text: row.pads.join(", "),
                        marks,
                    }
                })
                .collect(),
        })
        .collect()
}

/// The Library holding `element`, and whether it can be written to: an open element's
/// own, else the registered Library that has it.
pub fn library_of(shell: &mut Shell, element: Uuid) -> Option<(Uuid, bool)> {
    let index = shell.library_index();
    let library = index.library_of(element)?;
    Some((
        library.info.uuid,
        library.origin != reda_engine::library::LibraryOrigin::Starter,
    ))
}

/// What the Pins Panel's rows and the Part editor's previews point at for each other
/// (library.md#pins-panel): the Pin whose row the pointer is on, or the Pad under it in a
/// preview. View state, one Part at a time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Probe {
    pub part: Option<Uuid>,
    /// A row hovered in the Pins Panel: that Pin's number.
    pub pin: Option<String>,
    /// A Pad hovered in a Footprint preview: its number.
    pub pad: Option<String>,
}

/// The cross-probe now.
pub fn probe(shell: &Shell) -> Probe {
    shell
        .engine()
        .view()
        .get::<Probe>()
        .cloned()
        .unwrap_or_default()
}

/// Sets the cross-probe: `pin` from the Pins Panel's rows or `pad` from a preview, each
/// kept until the other side says otherwise.
pub fn set_probe(
    shell: &mut Shell,
    part: Uuid,
    pin: Option<Option<String>>,
    pad: Option<Option<String>>,
) {
    let view = shell.engine.view_mut();
    if view.get::<Probe>().is_none() {
        view.set(Probe::default());
    }
    let probe = view.get_mut::<Probe>().expect("just set");
    if probe.part != Some(part) {
        *probe = Probe {
            part: Some(part),
            ..Probe::default()
        };
    }
    if let Some(pin) = pin {
        probe.pin = pin;
    }
    if let Some(pad) = pad {
        probe.pad = pad;
    }
}

/// The Pin numbers the cross-probe highlights on `part`: the hovered row's Pin, or the
/// Pins that reach the hovered Pad.
pub fn probed_pins(shell: &Shell, part: &Part, symbol: Option<&Symbol>) -> Vec<String> {
    let probe = probe(shell);
    if probe.part != Some(part.uuid) {
        return Vec::new();
    }
    match (&probe.pin, &probe.pad) {
        (Some(pin), _) => vec![pin.clone()],
        (None, Some(pad)) => pin_rows(part, symbol)
            .into_iter()
            .filter(|r| r.pads.contains(pad))
            .map(|r| r.number)
            .collect(),
        (None, None) => Vec::new(),
    }
}

/// The Pad numbers the cross-probe highlights on `part`: those of the hovered row's Pin,
/// or the hovered Pad.
pub fn probed_pads(shell: &Shell, part: &Part, symbol: Option<&Symbol>) -> Vec<String> {
    let probe = probe(shell);
    if probe.part != Some(part.uuid) {
        return Vec::new();
    }
    match (&probe.pin, &probe.pad) {
        (Some(pin), _) => pin_rows(part, symbol)
            .into_iter()
            .find(|r| r.number == *pin)
            .map(|r| r.pads)
            .unwrap_or_default(),
        (None, Some(pad)) => vec![pad.clone()],
        (None, None) => Vec::new(),
    }
}

/// New for the Part's Symbol, or Add ▸ New for a Footprint: makes an empty element named
/// after the Part in the Part's Library (or the first writable one), points the Part at it
/// and opens it. Returns the new element.
pub fn new_element(
    shell: &mut Shell,
    part: Uuid,
    kind: &str,
) -> Result<Uuid, reda_engine::command::CommandError> {
    use reda_engine::command::CommandError;
    let Some(LibraryElement::Part(p)) = shell.engine().element(part).cloned() else {
        return Err(CommandError::not_found("the Part is not open"));
    };
    let library = match library_of(shell, part) {
        Some((library, true)) => library,
        _ => crate::library_dialogs::writable_libraries(shell)
            .first()
            .map(|(l, _)| *l)
            .ok_or_else(|| CommandError::rejected("no Library can be written to"))?,
    };
    let out = shell.run_reporting(
        "library.element.new",
        json!({"library": library, "kind": kind, "name": p.name}),
    )?;
    let element: Uuid = out["element"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| CommandError::internal("the new element has no UUID"))?;
    let (id, params) = match kind {
        "symbol" => set_symbol(&p, Some(element)),
        _ => add_footprint(&p, element),
    }
    .ok_or_else(|| CommandError::internal("the Part already has it"))?;
    shell.run_reporting(&id, params)?;
    crate::shared_element::opened_from_part(shell, element, part);
    shell.run_reporting("library.element.open", json!({"element": element}))?;
    Ok(element)
}
