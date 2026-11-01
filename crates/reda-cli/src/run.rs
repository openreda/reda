//! `reda run <id> [--params json | --key value…] [--project p] [--save]`.
//!
//! Runs in-process (bridging to a running app comes with the control port). Prints the
//! result as JSON on stdout, or the `{code, message}` error as JSON on stderr. Writes
//! nothing to disk unless `--save` is given or the Command itself is `project.save`.

use std::process::ExitCode;

use reda_engine::Engine;
use reda_engine::command::CommandError;
use serde_json::{Map, Value, json};

/// The parsed arguments of `reda run`.
#[derive(Debug, PartialEq)]
pub struct RunArgs {
    pub id: String,
    pub params: Value,
    pub project: Option<String>,
    pub save: bool,
}

/// Parses everything after `run`. Errors are usage messages.
pub fn parse(args: &[&str]) -> Result<RunArgs, String> {
    let (id, mut rest) = match args {
        [id, rest @ ..] if !id.starts_with("--") => (id.to_string(), rest),
        _ => return Err("reda run needs a Command id".into()),
    };
    let mut json_params: Option<Value> = None;
    let mut flags = Map::new();
    let mut project = None;
    let mut save = false;
    while let [first, tail @ ..] = rest {
        rest = tail;
        let Some(key) = first.strip_prefix("--") else {
            return Err(format!("unexpected argument '{first}'"));
        };
        if key == "save" {
            save = true;
            continue;
        }
        let [value, tail @ ..] = rest else {
            return Err(format!("--{key} needs a value"));
        };
        rest = tail;
        match key {
            "params" => {
                let parsed = serde_json::from_str(value)
                    .map_err(|e| format!("--params is not valid JSON: {e}"))?;
                json_params = Some(parsed);
            }
            "project" => project = Some(value.to_string()),
            _ => {
                flags.insert(key.to_owned(), flag_value(value));
            }
        }
    }
    let params = match (json_params, flags.is_empty()) {
        (Some(_), false) => return Err("give either --params or --key value flags".into()),
        (Some(params), true) => params,
        (None, _) => Value::Object(flags),
    };
    Ok(RunArgs {
        id,
        params,
        project,
        save,
    })
}

/// A `--key value` value: JSON if it parses as JSON (numbers, booleans, objects, quoted
/// strings), otherwise the plain string.
fn flag_value(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()))
}

/// Runs the parsed command on a fresh in-process engine.
pub fn execute(args: RunArgs) -> ExitCode {
    let mut engine = Engine::new();
    let result = (|| -> Result<Value, CommandError> {
        if let Some(project) = &args.project {
            engine.run("project.open", json!({ "path": project }))?;
        }
        let result = engine.run(&args.id, args.params)?;
        if args.save {
            engine.run("project.save", json!({}))?;
        }
        Ok(result)
    })();
    match result {
        Ok(value) => {
            println!("{}", pretty(&value));
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!(
                "{}",
                pretty(&serde_json::to_value(&err).expect("errors serialise"))
            );
            ExitCode::from(1)
        }
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("JSON values serialise")
}
