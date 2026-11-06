//! Schemas from the Rust types, and a check of params against them.
//!
//! The check covers the subset of JSON Schema 2020-12 that schemars generates for params:
//! `$ref` into `$defs`, `type`, `enum`, `const`, `properties`, `required`,
//! `additionalProperties`, `items`, `prefixItems`, `minItems`, `maxItems`, `oneOf`, `anyOf`,
//! `allOf`, `minimum`, `maximum` and the formats `uuid`, `length`, `copper_thickness`, `angle` and the integer
//! widths. It runs before serde so that a bad field is reported with a JSON pointer to it;
//! serde still has the last word on anything the subset does not express.

use reda_model::Uuid;
use reda_model::geom::{Millideg, Nm};
use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde_json::{Map, Value};

use crate::units::{ANGLE_FORMAT, COPPER_FORMAT, LENGTH_FORMAT};

/// The schema params of type `T` are checked against.
pub(crate) fn params_schema<T: JsonSchema>() -> Value {
    SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value()
}

/// The schema of a result of type `T`.
pub(crate) fn result_schema<T: JsonSchema>() -> Value {
    SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value()
}

/// Where and why params do not fit a schema.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Mismatch {
    /// A JSON pointer into the params, `""` for the whole.
    pub pointer: String,
    pub message: String,
}

/// Checks `value` against the root schema `root`.
pub(crate) fn check(root: &Value, value: &Value) -> Result<(), Mismatch> {
    Checker { root }.check(root, value, &mut Vec::new())
}

/// Checks `value` as the top-level property `name` of params fitting the root schema
/// `root`, without the rest of the params. Pointers start at `/<name>`.
pub(crate) fn check_property(root: &Value, name: &str, value: &Value) -> Result<(), Mismatch> {
    let checker = Checker { root };
    let mut path = vec![name.to_owned()];
    match root.get("properties").and_then(|p| p.get(name)) {
        Some(schema) => checker.check(schema, value, &mut path),
        None => Checker::fail(&path, format!("unknown field {name}")),
    }
}

/// Escapes one JSON pointer token (RFC 6901).
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn pointer(path: &[String]) -> String {
    path.iter().map(|t| format!("/{}", escape(t))).collect()
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "an integer",
        Value::Number(_) => "a fractional number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn has_type(value: &Value, ty: &str) -> bool {
    match ty {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    }
}

/// The inclusive range of a schemars integer `format`.
fn integer_range(format: &str) -> Option<(i128, i128)> {
    let range = match format {
        "int8" => (i8::MIN.into(), i8::MAX.into()),
        "int16" => (i16::MIN.into(), i16::MAX.into()),
        "int32" => (i32::MIN.into(), i32::MAX.into()),
        "int64" => (i64::MIN.into(), i64::MAX.into()),
        "uint8" => (0, u8::MAX.into()),
        "uint16" => (0, u16::MAX.into()),
        "uint32" => (0, u32::MAX.into()),
        "uint64" | "uint" => (0, u64::MAX.into()),
        _ => return None,
    };
    Some(range)
}

fn as_i128(value: &Value) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
}

struct Checker<'r> {
    root: &'r Value,
}

impl<'r> Checker<'r> {
    fn resolve(&self, reference: &str) -> Option<&'r Value> {
        let path = reference.strip_prefix('#')?;
        self.root.pointer(path)
    }

    fn fail<T>(path: &[String], message: impl Into<String>) -> Result<T, Mismatch> {
        Err(Mismatch {
            pointer: pointer(path),
            message: message.into(),
        })
    }

    fn check(&self, schema: &Value, value: &Value, path: &mut Vec<String>) -> Result<(), Mismatch> {
        let schema = match schema {
            Value::Bool(true) => return Ok(()),
            Value::Bool(false) => return Self::fail(path, "no value is allowed here"),
            Value::Object(schema) => schema,
            _ => return Ok(()),
        };
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            match self.resolve(reference) {
                Some(target) => self.check(target, value, path)?,
                None => return Self::fail(path, format!("unresolved schema {reference}")),
            }
        }
        self.check_type(schema, value, path)?;
        if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
            && !allowed.contains(value)
        {
            let list: Vec<String> = allowed.iter().map(Value::to_string).collect();
            return Self::fail(path, format!("expected one of {}", list.join(", ")));
        }
        if let Some(constant) = schema.get("const")
            && constant != value
        {
            return Self::fail(path, format!("expected {constant}"));
        }
        self.check_format(schema, value, path)?;
        self.check_bounds(schema, value, path)?;
        match value {
            Value::Object(map) => self.check_object(schema, map, path)?,
            Value::Array(items) => self.check_array(schema, items, path)?,
            _ => {}
        }
        if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
            for sub in all {
                self.check(sub, value, path)?;
            }
        }
        for keyword in ["oneOf", "anyOf"] {
            if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
                self.check_branches(branches, value, path)?;
            }
        }
        Ok(())
    }

    fn check_type(
        &self,
        schema: &Map<String, Value>,
        value: &Value,
        path: &[String],
    ) -> Result<(), Mismatch> {
        let types: Vec<&str> = match schema.get("type") {
            Some(Value::String(t)) => vec![t],
            Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
            _ => return Ok(()),
        };
        if types.iter().any(|t| has_type(value, t)) {
            return Ok(());
        }
        Self::fail(
            path,
            format!(
                "expected {}, found {}",
                types.join(" or "),
                type_name(value)
            ),
        )
    }

    fn check_format(
        &self,
        schema: &Map<String, Value>,
        value: &Value,
        path: &[String],
    ) -> Result<(), Mismatch> {
        let Some(format) = schema.get("format").and_then(Value::as_str) else {
            return Ok(());
        };
        if let Some(text) = value.as_str() {
            let refused = match format {
                "uuid" => text.parse::<Uuid>().err().map(|e| e.to_string()),
                LENGTH_FORMAT => Nm::parse(text).err().map(|e| e.to_string()),
                COPPER_FORMAT => Nm::parse_copper(text).err().map(|e| e.to_string()),
                ANGLE_FORMAT => Millideg::parse(text).err().map(|e| e.to_string()),
                _ => None,
            };
            if let Some(message) = refused {
                return Self::fail(path, message);
            }
        }
        if let (Some(n), Some((min, max))) = (as_i128(value), integer_range(format))
            && !(min..=max).contains(&n)
        {
            return Self::fail(path, format!("{n} is outside {min}..={max}"));
        }
        Ok(())
    }

    fn check_bounds(
        &self,
        schema: &Map<String, Value>,
        value: &Value,
        path: &[String],
    ) -> Result<(), Mismatch> {
        let Some(n) = value.as_f64() else {
            return Ok(());
        };
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64)
            && n < min
        {
            return Self::fail(path, format!("must be at least {min}"));
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64)
            && n > max
        {
            return Self::fail(path, format!("must be at most {max}"));
        }
        Ok(())
    }

    fn check_object(
        &self,
        schema: &Map<String, Value>,
        map: &Map<String, Value>,
        path: &mut Vec<String>,
    ) -> Result<(), Mismatch> {
        let empty = Map::new();
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !map.contains_key(key) {
                    path.push(key.to_owned());
                    let result = Self::fail(path, format!("the field `{key}` is missing"));
                    path.pop();
                    return result;
                }
            }
        }
        for (key, value) in map {
            path.push(key.clone());
            let result = match (properties.get(key), schema.get("additionalProperties")) {
                (Some(sub), _) => self.check(sub, value, path),
                (None, Some(Value::Bool(false))) => {
                    let known: Vec<&str> = properties.keys().map(String::as_str).collect();
                    let hint = if known.is_empty() {
                        "this Command takes no params".to_owned()
                    } else {
                        format!("expected one of {}", known.join(", "))
                    };
                    Self::fail(path, format!("unknown field `{key}`; {hint}"))
                }
                (None, Some(extra)) => self.check(extra, value, path),
                (None, None) => Ok(()),
            };
            path.pop();
            result?;
        }
        Ok(())
    }

    fn check_array(
        &self,
        schema: &Map<String, Value>,
        items: &[Value],
        path: &mut Vec<String>,
    ) -> Result<(), Mismatch> {
        let len = items.len() as u64;
        if let Some(min) = schema.get("minItems").and_then(Value::as_u64)
            && len < min
        {
            return Self::fail(path, format!("expected at least {min} items"));
        }
        if let Some(max) = schema.get("maxItems").and_then(Value::as_u64)
            && len > max
        {
            return Self::fail(path, format!("expected at most {max} items"));
        }
        let prefix = schema
            .get("prefixItems")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for (i, item) in items.iter().enumerate() {
            let sub = prefix.get(i).or_else(|| schema.get("items"));
            if let Some(sub) = sub {
                path.push(i.to_string());
                let result = self.check(sub, item, path);
                path.pop();
                result?;
            }
        }
        Ok(())
    }

    /// `oneOf`/`anyOf`: at least one branch must fit (serde settles any overlap). When none
    /// fits and exactly one branch has the value's JSON type, that branch's complaint is the
    /// most useful one, so it is reported.
    fn check_branches(
        &self,
        branches: &[Value],
        value: &Value,
        path: &mut Vec<String>,
    ) -> Result<(), Mismatch> {
        let mut candidates = Vec::new();
        for branch in branches {
            match self.check(branch, value, path) {
                Ok(()) => return Ok(()),
                Err(mismatch) if self.shape_fits(branch, value) => candidates.push(mismatch),
                Err(_) => {}
            }
        }
        if candidates.len() == 1 {
            return Err(candidates.remove(0));
        }
        Self::fail(
            path,
            format!("{} does not fit any allowed form", type_name(value)),
        )
    }

    /// Whether `value` has the JSON type `schema` expects, ignoring everything else.
    fn shape_fits(&self, schema: &Value, value: &Value) -> bool {
        let Some(schema) = schema.as_object() else {
            return true;
        };
        if let Some(target) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| self.resolve(r))
        {
            return self.shape_fits(target, value);
        }
        if let Some(constant) = schema.get("const") {
            return constant == value;
        }
        if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
            return allowed.iter().any(|a| has_type(value, json_type(a)));
        }
        match schema.get("type") {
            Some(Value::String(t)) => has_type(value, t),
            Some(Value::Array(ts)) => ts
                .iter()
                .filter_map(Value::as_str)
                .any(|t| has_type(value, t)),
            _ => ["oneOf", "anyOf"].iter().all(|k| {
                schema
                    .get(*k)
                    .and_then(Value::as_array)
                    .is_none_or(|bs| bs.iter().any(|b| self.shape_fits(b, value)))
            }),
        }
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
