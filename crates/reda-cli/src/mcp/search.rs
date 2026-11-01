//! `search_commands`: finds Commands in the catalogue by words, kind and category.

use serde_json::{Value, json};

/// The default and largest number of hits returned.
pub const DEFAULT_LIMIT: usize = 20;

/// The parsed `search_commands` arguments.
#[derive(Debug, Default)]
pub struct Query {
    /// Lowercase words that must each appear in the id, title, description or category.
    pub words: Vec<String>,
    pub kind: Option<String>,
    pub category: Option<String>,
    pub limit: usize,
}

impl Query {
    /// Reads `{query?, kind?, category?, limit?}`. Errors name the bad field.
    pub fn parse(args: &Value) -> Result<Query, String> {
        let Some(args) = args.as_object() else {
            return Err("arguments must be an object".into());
        };
        let mut query = Query {
            limit: DEFAULT_LIMIT,
            ..Query::default()
        };
        for (key, value) in args {
            match (key.as_str(), value) {
                ("query", Value::String(text)) => {
                    query.words = text.split_whitespace().map(str::to_lowercase).collect();
                }
                ("kind", Value::String(kind)) => query.kind = Some(kind.clone()),
                ("category", Value::String(category)) => query.category = Some(category.clone()),
                ("limit", Value::Number(n)) => match n.as_u64() {
                    Some(n) if n >= 1 => query.limit = usize::try_from(n).unwrap_or(usize::MAX),
                    _ => return Err("limit must be a positive integer".into()),
                },
                (_, Value::Null) => {}
                ("query" | "kind" | "category", _) => {
                    return Err(format!("{key} must be a string"));
                }
                ("limit", _) => return Err("limit must be a positive integer".into()),
                _ => return Err(format!("unknown field {key}")),
            }
        }
        Ok(query)
    }
}

/// The `search_commands` input schema.
pub fn params_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Words to look for in each Command's id, title, description and category. Every word must match; hits in the id rank first. Leave out to list every Command."
            },
            "kind": {
                "type": "string",
                "enum": ["edit", "query", "view", "file"],
                "description": "Only Commands of this kind."
            },
            "category": {
                "type": "string",
                "description": "Only Commands in this category, such as `project` or `schematic`."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "description": "The most hits to return (default 20)."
            }
        },
        "additionalProperties": false
    })
}

/// The `search_commands` output schema.
pub fn result_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "commands": {
                "type": "array",
                "description": "The best hits first. Call `describe_command` for a hit's params and result schemas.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "title": {"type": "string"},
                        "description": {"type": "string"},
                        "kind": {"type": "string"},
                        "category": {"type": "string"},
                        "presets": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Preset ids (`<id>:<preset>`), run with no params."
                        }
                    },
                    "required": ["id", "title", "description", "kind", "category", "presets"]
                }
            }
        },
        "required": ["commands"]
    })
}

/// Runs the query over the catalogue's `commands`: `{commands: [hit…]}`, best first, then
/// by id.
pub fn search(commands: &[Value], query: &Query) -> Value {
    let mut hits: Vec<(u32, &Value)> = commands
        .iter()
        .filter(|c| query.kind.as_deref().is_none_or(|k| c["kind"] == k))
        .filter(|c| query.category.as_deref().is_none_or(|k| c["category"] == k))
        .filter_map(|c| score(c, &query.words).map(|s| (s, c)))
        .collect();
    hits.sort_by(|(a, ca), (b, cb)| b.cmp(a).then_with(|| text(ca, "id").cmp(text(cb, "id"))));
    let hits: Vec<Value> = hits
        .into_iter()
        .take(query.limit)
        .map(|(_, c)| hit(c))
        .collect();
    json!({ "commands": hits })
}

/// How well a Command matches every word, or `None` when a word is missing. A word in
/// the id counts most, then the title, then the category or description.
fn score(command: &Value, words: &[String]) -> Option<u32> {
    let id = text(command, "id").to_lowercase();
    let title = text(command, "title").to_lowercase();
    let rest = format!(
        "{} {}",
        text(command, "category").to_lowercase(),
        text(command, "description").to_lowercase()
    );
    words.iter().try_fold(0, |total, word| {
        let points = if id.contains(word.as_str()) {
            4
        } else if title.contains(word.as_str()) {
            2
        } else if rest.contains(word.as_str()) {
            1
        } else {
            return None;
        };
        Some(total + points)
    })
}

fn hit(command: &Value) -> Value {
    let presets: Vec<&Value> = command["presets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| &p["id"])
        .collect();
    json!({
        "id": command["id"],
        "title": command["title"],
        "description": command["description"],
        "kind": command["kind"],
        "category": command["category"],
        "presets": presets,
    })
}

fn text<'v>(command: &'v Value, key: &str) -> &'v str {
    command[key].as_str().unwrap_or_default()
}
