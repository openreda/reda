//! A generator's form: one row per value of its params, nested objects opened up into
//! their leaves (`/goals/fillet/toe`), each in its section. A row left empty is left out
//! of the params, so the generator calculates it; the form never stores a calculated
//! value, so a change of density or goals recalculates every value not typed in.

use reda_engine::command::Registry;
use serde_json::{Map, Value};

use crate::command_dialog::form::{self as param, Field, Input};

/// Where a row sits in the form, in the spec's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    /// `library`, `name` and `description`.
    Target,
    /// The family's own fields: size, pins, dimensions and package options.
    Package,
    Goals,
    Tolerances,
    Pads,
    Outputs,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::Target,
        Section::Package,
        Section::Goals,
        Section::Tolerances,
        Section::Pads,
        Section::Outputs,
    ];

    fn of(pointer: &str) -> Section {
        match pointer.split('/').nth(1).unwrap_or("") {
            "library" | "name" | "description" => Section::Target,
            "goals" => Section::Goals,
            "tolerances" => Section::Tolerances,
            "pads" => Section::Pads,
            "outputs" => Section::Outputs,
            _ => Section::Package,
        }
    }
}

/// One value of the params. Its field's `name` is the value's JSON pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub section: Section,
    pub field: Field,
    /// The schema's default for the value, if it has one.
    pub default: Option<Value>,
}

impl Row {
    pub fn pointer(&self) -> &str {
        &self.field.name
    }
}

/// The form for one generator Command.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorForm {
    command: String,
    rows: Vec<Row>,
}

impl GeneratorForm {
    /// The form for generator `command`, filled in with the params `given`.
    pub fn new(registry: &Registry, command: &str, given: &Value) -> GeneratorForm {
        let empty = Value::Object(Map::new());
        let root = registry.params_schema(command).unwrap_or(&empty);
        let mut rows = Vec::new();
        walk(root, "", root, true, Some(given), &mut rows);
        rows.sort_by_key(|r| r.section);
        GeneratorForm {
            command: command.to_owned(),
            rows,
        }
    }

    /// The form for a Footprint's stored params, which hold every value resolved. A
    /// calculated value that the generator gives the same way again is cleared, so it is
    /// calculated anew when the density or another value changes; so are values equal to
    /// their default and the suggested description. Overrides stay.
    pub fn stored(registry: &Registry, command: &str, stored: &Value) -> GeneratorForm {
        let mut form = GeneratorForm::new(registry, command, stored);
        let overridden = match reda_engine::library::preview_footprint(command, stored) {
            Some(Ok(preview)) => overridden(&preview.warnings),
            _ => Vec::new(),
        };
        let calculated = reda_engine::library::calculated_values(command).unwrap_or(&[]);
        for pointer in calculated {
            if !overridden.iter().any(|o| o == pointer) {
                form.clear(pointer);
            }
        }
        let defaults: Vec<String> = form
            .rows
            .iter()
            .filter(|r| r.default.is_some() && stored.pointer(r.pointer()) == r.default.as_ref())
            .map(|r| r.pointer().to_owned())
            .collect();
        for pointer in defaults {
            form.clear(&pointer);
        }
        form
    }

    /// The generator Command the form is for.
    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub(crate) fn rows_mut(&mut self) -> &mut [Row] {
        &mut self.rows
    }

    pub fn row(&self, pointer: &str) -> Option<&Row> {
        self.rows.iter().find(|r| r.pointer() == pointer)
    }

    fn input_mut(&mut self, pointer: &str) -> Option<&mut Input> {
        self.rows
            .iter_mut()
            .find(|r| r.pointer() == pointer)
            .map(|r| &mut r.field.input)
    }

    /// Types `text` into a text, number, length or JSON row.
    pub fn set_text(&mut self, pointer: &str, text: &str) {
        if let Some(Input::Text(t) | Input::Integer(t) | Input::Measure(t) | Input::Json(t)) =
            self.input_mut(pointer)
        {
            *t = text.to_owned();
        }
    }

    /// Sets or clears a yes/no row.
    pub fn set_flag(&mut self, pointer: &str, value: Option<bool>) {
        if let Some(Input::Flag(flag)) = self.input_mut(pointer) {
            *flag = value;
        }
    }

    /// Picks `value` in a choice row, or clears it.
    pub fn choose(&mut self, pointer: &str, value: Option<&Value>) {
        let density = self.given("/goals/density");
        if let Some(Input::Choice { options, selected }) = self.input_mut(pointer) {
            *selected = value.and_then(|v| options.iter().position(|o| o == v));
        }
        self.follow_density(density.as_ref());
    }

    /// Keeps a suggested description in step with the density: one that ends in the
    /// density it was made at (`…, IPC-7351 Nominal`) takes the density picked now, since
    /// stored params hold the description written out. `before` is the density the form
    /// had.
    pub fn follow_density(&mut self, before: Option<&Value>) {
        let title = |d: Option<&Value>| {
            let name = d.and_then(Value::as_str).unwrap_or("nominal");
            format!("IPC-7351 {}", param::words(name))
        };
        let now = self.given("/goals/density");
        let (old, new) = (title(before), title(now.as_ref()));
        if old == new {
            return;
        }
        if let Some(Value::String(text)) = self.given("/description")
            && let Some(stem) = text.strip_suffix(&old)
        {
            let text = format!("{stem}{new}");
            self.set_text("/description", &text);
        }
    }

    /// Empties a row, so the generator calculates the value or takes its default.
    pub fn clear(&mut self, pointer: &str) {
        if let Some(input) = self.input_mut(pointer) {
            match input {
                Input::Text(t) | Input::Integer(t) | Input::Measure(t) | Input::Json(t) => {
                    t.clear();
                }
                Input::Flag(flag) => *flag = None,
                Input::Choice { selected, .. } => *selected = None,
            }
        }
    }

    /// The value typed or picked in a row; `None` when it is empty or cannot be read.
    pub fn given(&self, pointer: &str) -> Option<Value> {
        param::value(&self.row(pointer)?.field.input).ok().flatten()
    }

    /// The params the rows give, unchecked: each filled-in row at its pointer.
    pub fn raw_params(&self) -> Value {
        let mut params = Value::Object(Map::new());
        for row in &self.rows {
            if let Ok(Some(v)) = param::value(&row.field.input) {
                set(&mut params, row.pointer(), v);
            }
        }
        params
    }

    /// What is wrong, as `(pointer, message)`; `""` names the params as a whole.
    pub fn problems(&self, registry: &Registry) -> Vec<(String, String)> {
        let mut problems: Vec<(String, String)> = self
            .rows
            .iter()
            .filter_map(|r| match param::value(&r.field.input) {
                Err(message) => Some((r.pointer().to_owned(), message)),
                Ok(None) if r.field.required => {
                    Some((r.pointer().to_owned(), crate::tr!("form-required")))
                }
                Ok(_) => None,
            })
            .collect();
        if problems.is_empty()
            && let Err(e) = registry.check_params(&self.command, &self.raw_params())
        {
            let pointer = e
                .details
                .as_ref()
                .and_then(|d| d["pointer"].as_str())
                .unwrap_or("")
                .to_owned();
            let row = self.rows.iter().map(Row::pointer).find(|p| {
                !pointer.is_empty() && (*p == pointer || p.starts_with(&format!("{pointer}/")))
            });
            let message = e
                .message
                .strip_prefix(&pointer)
                .and_then(|m| m.strip_prefix(": "))
                .unwrap_or(&e.message)
                .to_owned();
            problems.push((row.unwrap_or("").to_owned(), message));
        }
        problems
    }

    /// The params to run with, or `None` while a row is missing or wrong.
    pub fn params(&self, registry: &Registry) -> Option<Value> {
        self.problems(registry)
            .is_empty()
            .then(|| self.raw_params())
    }
}

/// The params a [`reda_engine::library::GeneratorWarning`] of the not-IPC-compliant kind
/// names: the values that override IPC-7351's.
pub fn overridden(warnings: &[reda_engine::library::GeneratorWarning]) -> Vec<String> {
    warnings
        .iter()
        .filter(|w| w.code == reda_engine::library::GeneratorWarningCode::NotIpcCompliant)
        .flat_map(|w| w.params.iter().cloned())
        .collect()
}

/// Adds a row per leaf under `schema` at `pointer`: an object with `properties` opens up,
/// anything else is one row.
fn walk(
    root: &Value,
    pointer: &str,
    schema: &Value,
    required: bool,
    given: Option<&Value>,
    rows: &mut Vec<Row>,
) {
    let resolved = param::unwrap(root, schema);
    if let Some(properties) = resolved["properties"].as_object() {
        let needed: Vec<&str> = resolved["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut names: Vec<&String> = properties.keys().collect();
        // Required ones first, as the generated form has them, then single values
        // before the objects that open up into several (a size before dimensions).
        let opens = |n: &str| param::unwrap(root, &properties[n])["properties"].is_object();
        names.sort_by_key(|n| (!needed.contains(&n.as_str()), opens(n)));
        for name in names {
            walk(
                root,
                &format!("{pointer}/{name}"),
                &properties[name],
                required && needed.contains(&name.as_str()),
                given.and_then(|g| g.get(name)),
                rows,
            );
        }
        return;
    }
    let mut field = param::field(root, pointer, schema, required, given);
    if let (Input::Measure(text), Some(nm)) = (&mut field.input, given.and_then(Value::as_i64)) {
        *text = mm(nm);
    }
    field.label = label(pointer);
    let default = schema
        .get("default")
        .or_else(|| resolved.get("default"))
        .filter(|d| !d.is_null())
        .cloned();
    rows.push(Row {
        section: Section::of(pointer),
        field,
        default,
    });
}

/// A row's label from its pointer: the words after the section, `/goals/fillet/toe`
/// reading "Fillet toe" and `/dimensions/length/min` "Length min".
/// A package's `dimensions` read as the package's own: `/dimensions/height` is "Height".
fn label(pointer: &str) -> String {
    let parts: Vec<&str> = pointer.split('/').skip(1).collect();
    let words: Vec<&str> = match Section::of(pointer) {
        Section::Target => parts,
        Section::Package if parts.len() > 2 || parts[0] == "dimensions" => parts[1..].to_vec(),
        Section::Package => parts,
        _ => parts[1..].to_vec(),
    };
    param::words(&words.join(" "))
}

/// `nm` as a length to type: millimetres with at least two decimals, such as `0.35mm`.
pub fn mm(nm: i64) -> String {
    let sign = if nm < 0 { "-" } else { "" };
    let n = nm.unsigned_abs();
    let mut text = format!("{}.{:06}", n / 1_000_000, n % 1_000_000);
    while text.ends_with('0') && text.len() - text.find('.').unwrap_or(0) > 3 {
        text.pop();
    }
    format!("{sign}{text}mm")
}

/// Sets `value` at `pointer` in `params`, making the objects on the way.
fn set(params: &mut Value, pointer: &str, value: Value) {
    let mut at = params;
    let parts: Vec<&str> = pointer.split('/').skip(1).collect();
    for (i, part) in parts.iter().enumerate() {
        let Some(object) = at.as_object_mut() else {
            return;
        };
        if i + 1 == parts.len() {
            object.insert((*part).to_owned(), value);
            return;
        }
        at = object
            .entry((*part).to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}
