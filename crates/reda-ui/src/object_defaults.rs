//! Object Defaults in the app (docs/spec/v0.3/input-and-defaults.md#object-defaults):
//! which record each placing Tool starts from, and what an edit of its Tool options does.
//!
//! The records are the `defaults.*` Preferences. While a Tool is active, Properties shows
//! its record as Tool options. With `defaults.remember_last_used` on, an edit there is an
//! `app.preferences.set`; off, it is a `tool.option.set`, kept in [`SessionEdits`] until
//! the Tool is left (`tool.activate` of another Tool clears them). The Tools read the
//! record through [`current`], so both kinds of edit reach the next placement. Commands
//! never read a record: the Tools put each value in the Command's params.

use std::collections::BTreeMap;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{FootprintLayer, LayerKind};
use reda_engine::preferences::{self, DrawingKind, ObjectDefaults, TypedArea, TypedLength, Units};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tools::Tool;

/// Tool-option edits made with Remember last used off, by `defaults.*` key: they last
/// until the Tool is left. View state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionEdits(pub BTreeMap<String, Value>);

/// The Tools whose options Tab (`tool.edit_properties`) shows in Properties, with the
/// keyboard in the first one.
pub const OPTION_TOOLS: &[Tool] = &[
    Tool::Line,
    Tool::Rect,
    Tool::Arc,
    Tool::Circle,
    Tool::Polygon,
    Tool::Text,
    Tool::Dimension,
    Tool::FreePad,
    Tool::Via,
    Tool::Zone,
    Tool::Keepout,
    Tool::Pad,
    Tool::Pin,
];

/// Counts Tab presses that asked for the Tool options, so Properties moves the keyboard
/// there once per press. View state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FocusRequest(pub u64);

/// The Object Defaults the active Tool places with: the user's records with this
/// session's [`SessionEdits`] over them.
pub fn current(shell: &Shell) -> ObjectDefaults {
    let prefs = shell.preferences();
    let edits = shell.engine().view().get::<SessionEdits>();
    match edits {
        Some(edits) if !edits.0.is_empty() => with_edits(prefs, &edits.0).defaults,
        _ => prefs.defaults.clone(),
    }
}

/// `prefs` with each edit applied; an edit that no longer fits (the file changed under
/// it) is left out.
fn with_edits(
    prefs: &preferences::Preferences,
    edits: &BTreeMap<String, Value>,
) -> preferences::Preferences {
    let mut out = prefs.clone();
    for (key, value) in edits {
        if let Ok(next) = preferences::applied(&out, key, value.clone()) {
            out = next;
        }
    }
    out
}

/// The Command a Tool-option edit of `key` (a `defaults.*` key) to `value` runs: written
/// to the record with Remember last used on, kept for the Tool's run with it off.
pub fn edit(shell: &Shell, key: &str, value: Value) -> (&'static str, Value) {
    let params = json!({"key": key, "value": value});
    if shell.preferences().defaults.remember_last_used {
        ("app.preferences.set", params)
    } else {
        (OPTION_SET, params)
    }
}

/// One group of a record's fields, under one key prefix such as
/// `defaults.pcb.line.silkscreen`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub prefix: String,
    pub fields: &'static [Field],
}

/// How a Tool option is edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// A length typed with or without a unit.
    Length,
    /// A length, or blank for unset.
    OptionalLength,
    /// A checkbox.
    Toggle,
    /// One of the schema's values.
    Choice,
    /// A whole number.
    Count,
    /// Text.
    Text,
    /// A share of half a Pad's shorter side, typed in percent.
    Percent,
}

/// One field of a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub kind: FieldKind,
}

const fn f(name: &'static str, kind: FieldKind) -> Field {
    Field { name, kind }
}

use FieldKind::{Choice, Count, Length, OptionalLength, Percent, Text, Toggle};

const STROKE: &[Field] = &[f("width", Length)];
const OUTLINE: &[Field] = &[f("width", Length), f("fill", Toggle)];
const TEXT_LOOK: &[Field] = &[f("height", Length), f("thickness", Length), f("font", Text)];
const JUSTIFY: &[Field] = &[f("h", Choice), f("v", Choice)];
const DIMENSION: &[Field] = &[
    f("width", Length),
    f("extension_width", Length),
    f("offset", Length),
    f("gap", Length),
    f("text_gap", Length),
    f("arrow_size", Length),
    f("arrow_length", Length),
    f("arrow_position", Choice),
    f("precision", Count),
    f("units", Choice),
    f("format", Choice),
    f("text_height", Length),
];
const FREE_PAD: &[Field] = &[
    f("kind", Choice),
    f("shape", Choice),
    f("width", Length),
    f("height", Length),
    f("hole", Length),
];
const PAD: &[Field] = &[
    f("kind", Choice),
    f("shape", Choice),
    f("width", Length),
    f("height", Length),
    f("hole_width", Length),
    f("hole_height", Length),
    f("chamfer_ratio", Percent),
    f("mask_margin", OptionalLength),
    f("paste_margin", OptionalLength),
];
const VIA: &[Field] = &[f("teardrop", Toggle)];
const PIN: &[Field] = &[
    f("length", Length),
    f("electrical_type", Choice),
    f("name_visible", Toggle),
    f("number_visible", Toggle),
];
const SHEET_TEXT: &[Field] = &[f("height", Length)];
const SHEET_RECTANGLE: &[Field] = &[f("fill", Toggle)];
const PORT: &[Field] = &[f("direction", Choice)];

fn group(prefix: String, fields: &'static [Field]) -> Group {
    Group { prefix, fields }
}

/// Where the active Tool places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    Board,
    Footprint,
    Symbol,
    Sheet,
}

/// The editor of the active Document.
pub fn editor(shell: &Shell) -> Option<Editor> {
    if shell.board_is_active() {
        Some(Editor::Board)
    } else if crate::element_editor::active_footprint(shell).is_some() {
        Some(Editor::Footprint)
    } else if crate::element_editor::active_symbol(shell).is_some() {
        Some(Editor::Symbol)
    } else if shell.active_sheet().is_some() {
        Some(Editor::Sheet)
    } else {
        None
    }
}

/// The Layer kind a board drawing Tool draws on now: the active Layer's.
pub fn board_kind(shell: &Shell) -> Option<DrawingKind> {
    let project = shell.engine().project()?;
    let layer = shell.board_view().active_layer(project)?;
    let kind: LayerKind = project.pcb()?.layers().layer(layer)?.kind;
    DrawingKind::of_layer(kind)
}

/// The Layer kind a Footprint drawing Tool draws on now: the drawing Layer's.
pub fn footprint_kind(shell: &Shell) -> DrawingKind {
    DrawingKind::of_footprint(crate::footprint_editor::tool_settings(shell).layer)
}

/// The Layer Footprint text goes on: the drawing Layer when it can hold text, else the
/// Silkscreen on its side.
pub fn footprint_text_layer(shell: &Shell) -> FootprintLayer {
    crate::footprint_editor::inspector::text_layer(
        crate::footprint_editor::tool_settings(shell).layer,
    )
}

/// The record the active Tool places from, as groups of fields; `None` when the active
/// Tool places nothing with Object Defaults (Select, Route, Wire, Net Label, ...). The
/// Zone and Keepout Tools have their own rows ([`crate::panels`]), so they are left out.
pub fn record(shell: &Shell) -> Option<Vec<Group>> {
    let tool = shell.tool();
    let pcb = |kind: &str, layer: DrawingKind| format!("defaults.pcb.{kind}.{}", layer.key());
    Some(match (editor(shell)?, tool) {
        (Editor::Board, Tool::Line) => vec![group(pcb("line", board_kind(shell)?), STROKE)],
        (Editor::Board, Tool::Rect) => {
            vec![group(pcb("rectangle", board_kind(shell)?), OUTLINE)]
        }
        (Editor::Board, Tool::Arc) => vec![group(pcb("arc", board_kind(shell)?), STROKE)],
        (Editor::Board, Tool::Text) => vec![
            group(pcb("text", board_kind(shell)?), TEXT_LOOK),
            group("defaults.pcb.text.justify".into(), JUSTIFY),
        ],
        (Editor::Board, Tool::Dimension) => {
            vec![group("defaults.pcb.dimension".into(), DIMENSION)]
        }
        (Editor::Board, Tool::FreePad) => vec![group("defaults.pcb.free_pad".into(), FREE_PAD)],
        (Editor::Board, Tool::Via) => vec![group("defaults.pcb.via".into(), VIA)],
        (Editor::Footprint, Tool::Pad) => vec![group("defaults.footprint.pad".into(), PAD)],
        (Editor::Footprint, Tool::Line) => vec![group(pcb("line", footprint_kind(shell)), STROKE)],
        (Editor::Footprint, Tool::Rect) => {
            vec![group(pcb("rectangle", footprint_kind(shell)), OUTLINE)]
        }
        (Editor::Footprint, Tool::Arc) => vec![group(pcb("arc", footprint_kind(shell)), STROKE)],
        (Editor::Footprint, Tool::Circle) => {
            vec![group(pcb("circle", footprint_kind(shell)), OUTLINE)]
        }
        (Editor::Footprint, Tool::Polygon) => {
            vec![group(pcb("polygon", footprint_kind(shell)), OUTLINE)]
        }
        (Editor::Footprint, Tool::Text) => vec![
            group(
                pcb(
                    "text",
                    DrawingKind::of_footprint(footprint_text_layer(shell)),
                ),
                TEXT_LOOK,
            ),
            group("defaults.footprint.text.justify".into(), JUSTIFY),
        ],
        (Editor::Symbol, Tool::Pin) => vec![group("defaults.symbol.pin".into(), PIN)],
        (Editor::Symbol, Tool::Line) => vec![group("defaults.symbol.line".into(), STROKE)],
        (Editor::Symbol, Tool::Rect) => vec![group("defaults.symbol.rectangle".into(), OUTLINE)],
        (Editor::Symbol, Tool::Arc) => vec![group("defaults.symbol.arc".into(), STROKE)],
        (Editor::Symbol, Tool::Circle) => vec![group("defaults.symbol.circle".into(), OUTLINE)],
        (Editor::Symbol, Tool::Polygon) => vec![group("defaults.symbol.polygon".into(), OUTLINE)],
        (Editor::Symbol | Editor::Sheet, Tool::Text) => {
            vec![group("defaults.schematic.text".into(), SHEET_TEXT)]
        }
        (Editor::Sheet, Tool::Rect) => {
            vec![group(
                "defaults.schematic.rectangle".into(),
                SHEET_RECTANGLE,
            )]
        }
        (Editor::Sheet, Tool::Port) => vec![group("defaults.schematic.port".into(), PORT)],
        _ => return None,
    })
}

/// Whether `field` of the record at `prefix` applies with the values `record` holds: a
/// Pad's hole only with a hole, its chamfer only when chamfered, its paste only on SMD.
pub fn applies(prefix: &str, field: &str, record: &Value) -> bool {
    let kind = record["kind"].as_str().unwrap_or_default();
    let shape = record["shape"].as_str().unwrap_or_default();
    match (prefix.rsplit('.').next().unwrap_or_default(), field) {
        ("pad" | "free_pad", "hole" | "hole_width" | "hole_height") => kind != "smd",
        ("pad", "chamfer_ratio") => shape == "chamfered_rect",
        ("pad", "paste_margin") => kind == "smd",
        _ => true,
    }
}

/// The value at the dotted `key` (a `defaults.*` key) of `defaults`.
pub fn value(defaults: &ObjectDefaults, key: &str) -> Value {
    let mut node = serde_json::to_value(defaults).expect("Object Defaults serialise");
    for segment in key.strip_prefix("defaults.").unwrap_or(key).split('.') {
        node = node.get(segment).cloned().unwrap_or(Value::Null);
    }
    node
}

/// A record's length as a field shows it: in the unit it was typed in, else (a bare
/// number of nanometres) in `units`.
pub fn shown_length(length: TypedLength, units: Units) -> String {
    length
        .shown()
        .unwrap_or_else(|| crate::pcb::view::format_length(length.nm(), units))
}

/// A record's area as a field shows it: in the unit it was typed in, else in `units`.
pub fn shown_area(area: TypedArea, units: Units) -> String {
    area.shown()
        .unwrap_or_else(|| crate::panels::navigator::format_area(area.nm2(), units))
}

/// A length or area field's value as JSON (`"0.15mm"`, `150000` or `null`) as the field
/// shows it.
pub fn shown_value(value: &Value, units: Units, area: bool) -> String {
    let typed = |text: &str| {
        if area {
            TypedArea::parse(text).ok().map(|a| shown_area(a, units))
        } else {
            TypedLength::parse(text)
                .ok()
                .map(|l| shown_length(l, units))
        }
    };
    match value {
        Value::String(s) => typed(s).unwrap_or_else(|| s.clone()),
        Value::Number(n) => typed(&n.to_string()).unwrap_or_default(),
        _ => String::new(),
    }
}

/// What a length (or `area`) typed into a field sets, keeping its unit: `"0.2 mm"` is
/// `"0.2mm"`, and a bare number is in `units`. Text that is no value is passed on as it
/// is, for the Preference to refuse.
pub fn typed_value(text: &str, units: Units, area: bool) -> Value {
    let t = text.trim().replace(' ', "").replace('²', "2");
    if t.parse::<f64>().is_err() {
        return json!(t);
    }
    let unit = match units {
        Units::Mm => "mm",
        Units::Mil => "mil",
    };
    json!(format!("{t}{unit}{}", if area { "2" } else { "" }))
}

/// Puts `kind` into the session's Pad record, as the SMD and through-hole Pad Presets of
/// `tool.activate` ask: it lasts while the Pad Tool runs.
pub(crate) fn preset_pad_kind(edits: &mut SessionEdits, kind: reda_engine::model::PadKind) {
    edits
        .0
        .insert("defaults.footprint.pad.kind".to_owned(), json!(kind));
}

/// The id of the session edit Command.
pub const OPTION_SET: &str = "tool.option.set";

/// Adds `tool.option.set` to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(OptionSet);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OptionSetParams {
    /// An Object Default, such as `defaults.pcb.line.silkscreen.width`.
    key: String,
    /// Its value for the active Tool's run, as `app.preferences.set` takes it.
    value: Value,
}

/// `tool.option.set`: a Tool-option edit with Remember last used off.
struct OptionSet;

impl Command for OptionSet {
    const META: Meta = Meta {
        id: OPTION_SET,
        title: "Set Tool Option",
        description: "Sets one of the active Tool's options (an Object Default such as \
                      `defaults.pcb.line.silkscreen.width`) until the Tool is left, without \
                      writing the Preference. With Remember last used on, the Properties \
                      panel writes the Preference instead.",
        category: "tool",
        icon: "properties",
        kind: Kind::View,
    };
    type Params = OptionSetParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: OptionSetParams) -> Result<Value, CommandError> {
        if !p.key.starts_with("defaults.") || p.key == "defaults.remember_last_used" {
            return Err(CommandError::invalid_params(
                "/key",
                format!("{} is not an Object Default", p.key),
            ));
        }
        let prefs = preferences::current(cx.user_dirs());
        let edits = cx.view::<SessionEdits>()?;
        // Checked as the Preference would be, over the edits already made.
        let next = preferences::applied(&with_edits(&prefs, &edits.0), &p.key, p.value)
            .map_err(|why| CommandError::invalid_params("/value", why))?;
        let value = value(&next.defaults, &p.key);
        edits.0.insert(p.key, value.clone());
        Ok(json!({ "value": value }))
    }
}
