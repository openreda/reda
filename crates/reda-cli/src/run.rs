//! `reda run <id> [--params json | --key value…] [--project p] [--save]` and
//! `reda run --script journal.jsonl [--project p] [--save]`.
//!
//! Runs in-process (bridging to a running app comes with the control port). Prints the
//! result as JSON on stdout, or the `{code, message}` error as JSON on stderr. Edit
//! Commands change only the in-memory Project, so they reach disk only with `--save`; file
//! Commands write what they always write (`project.new` writes the new Project,
//! `project.save` saves).
//!
//! `--script` replays an exported Journal (see [`reda_engine::journal::replay`]) and prints
//! the replay's own Journal as JSON lines. A replay mismatch prints
//! `{line, id, pointer?, message}` on stderr.

use std::process::ExitCode;

use reda_engine::Engine;
use reda_engine::command::CommandError;
use reda_engine::journal;
use serde_json::{Map, Value, json};

/// The parsed arguments of `reda run`.
#[derive(Debug, PartialEq)]
pub struct RunArgs {
    pub target: Target,
    pub params: Value,
    pub project: Option<String>,
    pub save: bool,
}

/// What `reda run` runs.
#[derive(Debug, PartialEq)]
pub enum Target {
    /// One Command, by id.
    Command(String),
    /// A Journal script file to replay.
    Script(String),
}

/// Parses everything after `run`. Errors are usage messages.
pub fn parse(args: &[&str]) -> Result<RunArgs, String> {
    let (target, mut rest) = match args {
        ["--script", script, rest @ ..] => (Target::Script(script.to_string()), rest),
        ["--script"] => return Err("--script needs a file".into()),
        [id, rest @ ..] if !id.starts_with("--") => (Target::Command(id.to_string()), rest),
        _ => return Err("reda run needs a Command id or --script".into()),
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
    if matches!(target, Target::Script(_)) && (json_params.is_some() || !flags.is_empty()) {
        return Err("--script takes no params".into());
    }
    let params = match (json_params, flags.is_empty()) {
        (Some(_), false) => return Err("give either --params or --key value flags".into()),
        (Some(params), true) => params,
        (None, _) => Value::Object(flags),
    };
    Ok(RunArgs {
        target,
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
    match &args.target {
        Target::Command(id) => execute_command(&mut engine, id, &args),
        Target::Script(script) => execute_script(&mut engine, script, &args),
    }
}

fn execute_command(engine: &mut Engine, id: &str, args: &RunArgs) -> ExitCode {
    let result = (|| -> Result<Value, CommandError> {
        open(engine, args)?;
        let result = engine.run(id, args.params.clone())?;
        save(engine, args)?;
        Ok(result)
    })();
    match result {
        Ok(value) => {
            println!("{}", pretty(&value));
            ExitCode::SUCCESS
        }
        Err(err) => fail(serde_json::to_value(&err)),
    }
}

/// Replays the script, then prints the replay's Journal as JSON lines.
fn execute_script(engine: &mut Engine, script: &str, args: &RunArgs) -> ExitCode {
    let text = match std::fs::read_to_string(script) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return fail(serde_json::to_value(CommandError::not_found(format!(
                "no script at {script}"
            ))));
        }
        Err(e) => {
            return fail(serde_json::to_value(CommandError::internal(format!(
                "reading {script}: {e}"
            ))));
        }
    };
    let steps = match journal::parse(&text) {
        Ok(steps) => steps,
        Err(err) => return fail(serde_json::to_value(&err)),
    };
    if let Err(err) = open(engine, args) {
        return fail(serde_json::to_value(&err));
    }
    if let Err(err) = journal::replay(engine, &steps) {
        return fail(serde_json::to_value(&err));
    }
    if let Err(err) = save(engine, args) {
        return fail(serde_json::to_value(&err));
    }
    print!("{}", journal::export(engine.journal()));
    ExitCode::SUCCESS
}

/// `--project`: opens the Project first.
fn open(engine: &mut Engine, args: &RunArgs) -> Result<(), CommandError> {
    if let Some(project) = &args.project {
        engine.run("project.open", json!({ "path": project }))?;
    }
    Ok(())
}

/// `--save`: saves every file last.
fn save(engine: &mut Engine, args: &RunArgs) -> Result<(), CommandError> {
    if args.save {
        engine.run("project.save", json!({}))?;
    }
    Ok(())
}

/// Prints an error, already turned into JSON with `serde_json::to_value`, on stderr and
/// exits 1.
fn fail(err: serde_json::Result<Value>) -> ExitCode {
    eprintln!("{}", pretty(&err.expect("errors serialise")));
    ExitCode::from(1)
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("JSON values serialise")
}
