//! The form generated from a Command's params schema: one field per top-level param,
//! typed from the schema, each checked against its own part of the schema as it is filled
//! in.

use reda_engine::command::Registry;
use serde_json::{Map, Value};

use crate::bars;
use crate::tr;

/// What a field takes, with what has been typed or picked so far.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// A string (including UUIDs). Empty leaves it out.
    Text(String),
    /// A whole number, as typed.
    Integer(String),
    /// A length or angle: a whole number of nm or millidegrees, or a number and a unit
    /// (`"1.27mm"`, `"45deg"`), as typed.
    Measure(String),
    /// A boolean; `None` leaves it out.
    Flag(Option<bool>),
    /// One of a fixed set of values; `None` leaves it out.
    Choice {
        options: Vec<Value>,
        selected: Option<usize>,
    },
    /// Anything else (objects, lists), typed as JSON.
    Json(String),
}

/// One top-level param.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// The param's name in the params object.
    pub name: String,
    /// The schema's title, else the name in words: `fab_preset` reads "Fab preset".
    pub label: String,
    /// The schema's description, for the tooltip.
    pub description: String,
    pub required: bool,
    pub input: Input,
}

/// A form for the params of one Command.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamForm {
    command: String,
    title: String,
    description: String,
    fields: Vec<Field>,
    /// Params the call gave that no field shows; they are passed on unchanged.
    rest: Map<String, Value>,
    /// Why the last run failed.
    pub(super) error: Option<String>,
    /// Warnings the form shows above its buttons, such as what a package leaves out.
    notes: Vec<String>,
}

impl ParamForm {
    /// The form for `id`, filled in with the params the call did give.
    pub fn new(registry: &Registry, id: &str, given: &Value) -> ParamForm {
        let empty = Value::Object(Map::new());
        let root = registry.params_schema(id).unwrap_or(&empty);
        let (title, description) = match registry.get(id) {
            Some(meta) => (
                bars::command_title(id, meta.title),
                meta.description.to_owned(),
            ),
            None => (id.to_owned(), String::new()),
        };
        let mut rest = given.as_object().cloned().unwrap_or_default();
        let properties = root["properties"].as_object();
        let required: Vec<&str> = root["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let optional = properties
            .into_iter()
            .flatten()
            .map(|(name, _)| name.as_str())
            .filter(|name| !required.contains(name));
        let fields = required
            .iter()
            .copied()
            .chain(optional)
            .filter_map(|name| {
                let schema = properties?.get(name)?;
                let given = rest.remove(name);
                Some(field(
                    root,
                    name,
                    schema,
                    required.contains(&name),
                    given.as_ref(),
                ))
            })
            .collect();
        ParamForm {
            command: id.to_owned(),
            title,
            description,
            fields,
            rest,
            error: None,
            notes: Vec::new(),
        }
    }

    /// The form with `notes` shown as warnings above its buttons.
    pub fn with_notes(mut self, notes: Vec<String>) -> ParamForm {
        self.notes = notes;
        self
    }

    /// The warnings the form shows.
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// The Command id the form is for.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// The Command's title, which heads the form.
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub(crate) fn fields_mut(&mut self) -> &mut [Field] {
        &mut self.fields
    }

    /// Types `text` into a text, number, length or JSON field.
    pub fn set_text(&mut self, name: &str, text: &str) {
        if let Some(field) = self.fields.iter_mut().find(|f| f.name == name) {
            match &mut field.input {
                Input::Text(t) | Input::Integer(t) | Input::Measure(t) | Input::Json(t) => {
                    *t = text.to_owned();
                }
                Input::Flag(_) | Input::Choice { .. } => {}
            }
        }
    }

    /// Sets or clears a boolean field.
    pub fn set_flag(&mut self, name: &str, value: Option<bool>) {
        if let Some(Field {
            input: Input::Flag(flag),
            ..
        }) = self.fields.iter_mut().find(|f| f.name == name)
        {
            *flag = value;
        }
    }

    /// Picks option `index` of a choice field, or clears it.
    pub fn choose(&mut self, name: &str, index: Option<usize>) {
        if let Some(Field {
            input: Input::Choice { options, selected },
            ..
        }) = self.fields.iter_mut().find(|f| f.name == name)
        {
            *selected = index.filter(|i| *i < options.len());
        }
    }

    /// What is wrong, as `(field name, message)` in field order; `""` names the params as a
    /// whole. Empty when the params fit the Command's schema.
    pub fn problems(&self, registry: &Registry) -> Vec<(String, String)> {
        let mut problems = Vec::new();
        let mut params = self.rest.clone();
        for field in &self.fields {
            match value(&field.input) {
                Err(message) => problems.push((field.name.clone(), message)),
                Ok(None) if field.required => {
                    problems.push((field.name.clone(), tr!("form-required")));
                }
                Ok(None) => {}
                Ok(Some(v)) => {
                    if let Err(e) = registry.check_param(&self.command, &field.name, &v) {
                        problems.push((field.name.clone(), field_message(&field.name, &e)));
                    }
                    params.insert(field.name.clone(), v);
                }
            }
        }
        if problems.is_empty()
            && let Err(e) = registry.check_params(&self.command, &Value::Object(params))
        {
            problems.push((String::new(), e.message));
        }
        problems
    }

    /// The params to run with, or `None` while a field is missing or wrong.
    pub fn params(&self, registry: &Registry) -> Option<Value> {
        if !self.problems(registry).is_empty() {
            return None;
        }
        let mut params = self.rest.clone();
        for field in &self.fields {
            if let Ok(Some(v)) = value(&field.input) {
                params.insert(field.name.clone(), v);
            }
        }
        Some(Value::Object(params))
    }
}

/// A param check's message for the field `name`: the pointer is left out when it is the
/// field itself, else given from inside the field (`height: expected …`).
fn field_message(name: &str, error: &reda_engine::command::CommandError) -> String {
    let pointer = error
        .details
        .as_ref()
        .and_then(|d| d["pointer"].as_str())
        .unwrap_or("");
    let message = error
        .message
        .strip_prefix(pointer)
        .and_then(|m| m.strip_prefix(": "))
        .unwrap_or(&error.message);
    match pointer
        .strip_prefix('/')
        .and_then(|p| p.strip_prefix(name))
        .and_then(|p| p.strip_prefix('/'))
    {
        Some(inside) => format!("{inside}: {message}"),
        None => message.to_owned(),
    }
}

/// The value a field gives: `Ok(None)` when it is left empty, `Err` when what was typed
/// cannot be read.
fn value(input: &Input) -> Result<Option<Value>, String> {
    Ok(match input {
        Input::Text(t) if t.is_empty() => None,
        Input::Text(t) => Some(Value::String(t.clone())),
        Input::Integer(t) => {
            let t = t.trim();
            if t.is_empty() {
                None
            } else if let Ok(n) = t.parse::<i64>() {
                Some(n.into())
            } else if let Ok(n) = t.parse::<u64>() {
                Some(n.into())
            } else {
                return Err(tr!("form-whole-number"));
            }
        }
        Input::Measure(t) => {
            let t = t.trim();
            if t.is_empty() {
                None
            } else if let Ok(n) = t.parse::<i64>() {
                Some(n.into())
            } else {
                Some(Value::String(t.to_owned()))
            }
        }
        Input::Flag(flag) => flag.map(Value::Bool),
        Input::Choice { options, selected } => selected.and_then(|i| options.get(i).cloned()),
        Input::Json(t) if t.trim().is_empty() => None,
        Input::Json(t) => match serde_json::from_str(t) {
            Ok(v) => Some(v),
            Err(e) => return Err(tr!("form-bad-json", error = e.to_string())),
        },
    })
}

fn field(root: &Value, name: &str, schema: &Value, required: bool, given: Option<&Value>) -> Field {
    let resolved = unwrap(root, schema);
    let text = |v: Option<&Value>| match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    let input = if let Some(options) = choices(resolved) {
        let selected = given.and_then(|g| options.iter().position(|o| o == g));
        Input::Choice { options, selected }
    } else if is_measure(root, resolved) {
        Input::Measure(text(given))
    } else {
        let types = types(resolved);
        match types.as_slice() {
            ["boolean"] => Input::Flag(
                given
                    .and_then(Value::as_bool)
                    .or((required).then_some(false)),
            ),
            ["integer"] => Input::Integer(text(given)),
            ["string"] if given.is_none_or(Value::is_string) => Input::Text(text(given)),
            _ => Input::Json(
                given
                    .filter(|v| !v.is_null())
                    .map_or_else(String::new, |v| v.to_string()),
            ),
        }
    };
    let description = [schema, resolved]
        .iter()
        .find_map(|s| s["description"].as_str())
        .unwrap_or("")
        .to_owned();
    let label = schema["title"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| words(name));
    Field {
        name: name.to_owned(),
        label,
        description,
        required,
        input,
    }
}

/// `fab_preset` as "Fab preset".
fn words(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => spaced,
    }
}

/// `schema` with `$ref`s followed and an optional's `anyOf [X, null]` reduced to X.
fn unwrap<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let mut schema = schema;
    for _ in 0..16 {
        if let Some(target) = schema["$ref"]
            .as_str()
            .and_then(|r| r.strip_prefix('#'))
            .and_then(|p| root.pointer(p))
        {
            schema = target;
            continue;
        }
        let branches = schema["anyOf"].as_array().or(schema["oneOf"].as_array());
        if let Some(branches) = branches {
            let other: Vec<&Value> = branches.iter().filter(|b| b["type"] != "null").collect();
            if other.len() == 1 && branches.len() == 2 {
                schema = other[0];
                continue;
            }
        }
        break;
    }
    schema
}

/// The schema's types other than `null`.
fn types(schema: &Value) -> Vec<&str> {
    match &schema["type"] {
        Value::String(t) => vec![t.as_str()],
        Value::Array(ts) => ts
            .iter()
            .filter_map(Value::as_str)
            .filter(|t| *t != "null")
            .collect(),
        _ => Vec::new(),
    }
}

/// The values of an `enum`, or of a `oneOf` whose every branch is a `const`.
fn choices(schema: &Value) -> Option<Vec<Value>> {
    if let Some(values) = schema["enum"].as_array() {
        return Some(values.iter().filter(|v| !v.is_null()).cloned().collect());
    }
    let branches = schema["oneOf"].as_array()?;
    branches
        .iter()
        .map(|b| b.get("const").cloned())
        .collect::<Option<Vec<_>>>()
}

/// Whether the schema is a length or an angle: integer or a unit string.
fn is_measure(root: &Value, schema: &Value) -> bool {
    schema["oneOf"].as_array().is_some_and(|branches| {
        branches
            .iter()
            .any(|b| matches!(unwrap(root, b)["format"].as_str(), Some("length" | "angle")))
    })
}
