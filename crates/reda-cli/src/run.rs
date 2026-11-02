//! `reda run <id> [--params json | --key value…] [--project p] [--save]` and
//! `reda run --script journal.jsonl [--project p] [--save]`.
//!
//! Bridges to a running instance that holds the Project ([`crate::bridge`]), else runs
//! in-process. Prints the result as JSON on stdout, or the `{code, message}` error as JSON
//! on stderr. In-process, edit Commands change only the in-memory Project, so they reach
//! disk only with `--save`; file Commands write what they always write (`project.new`
//! writes the new Project, `project.save` saves). Bridged, never saves (`--save` prints a
//! note on stderr).
//!
//! `--script` replays an exported Journal (see [`reda_engine::journal::replay`]) and prints
//! the replay's own Journal as JSON lines. A replay mismatch prints
//! `{line, id, pointer?, message}` on stderr.

use std::process::ExitCode;

use reda_control::client::Connection;
use reda_engine::Engine;
use reda_engine::command::CommandError;
use reda_engine::journal::{self, JournalEntry, Source};

use crate::bridge;
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

/// Runs the parsed command on the instance holding the Project, or on a fresh in-process
/// engine.
pub fn execute(args: RunArgs) -> ExitCode {
    let may_bridge = match &args.target {
        Target::Command(id) => bridge::may_bridge(id),
        Target::Script(_) => true,
    };
    let bridged = match may_bridge {
        true => bridge::connect(args.project.as_deref(), Source::Cli),
        false => Ok(None),
    };
    let mut runner = match bridged {
        Err(err) => return fail(serde_json::to_value(&err)),
        Ok(Some(connection)) => Runner::Bridged(connection, Vec::new()),
        Ok(None) => Runner::Local(Box::new(
            Engine::builder().trash(crate::os_trash::OsTrash).build(),
        )),
    };
    match &args.target {
        Target::Command(id) => execute_command(&mut runner, id, &args),
        Target::Script(script) => execute_script(&mut runner, script, &args),
    }
}

/// Where Commands run.
enum Runner {
    Local(Box<Engine>),
    /// A connection to the instance holding the Project, and the Journal of this run.
    Bridged(Connection, Vec<JournalEntry>),
}

impl Runner {
    fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        match self {
            Runner::Local(engine) => engine.run(id, params),
            Runner::Bridged(connection, journal) => {
                let outcome = connection.run(id, params.clone());
                journal.push(JournalEntry {
                    id: id.to_owned(),
                    params,
                    source: Source::Cli,
                    outcome: outcome.clone(),
                });
                outcome
            }
        }
    }

    fn journal(&self) -> &[JournalEntry] {
        match self {
            Runner::Local(engine) => engine.journal(),
            Runner::Bridged(_, journal) => journal,
        }
    }
}

fn execute_command(runner: &mut Runner, id: &str, args: &RunArgs) -> ExitCode {
    let result = (|| -> Result<Value, CommandError> {
        open(runner, args)?;
        let result = runner.run(id, args.params.clone())?;
        save(runner, args)?;
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
fn execute_script(runner: &mut Runner, script: &str, args: &RunArgs) -> ExitCode {
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
    if let Err(err) = open(runner, args) {
        return fail(serde_json::to_value(&err));
    }
    if let Err(err) = journal::replay_with(|id, params| runner.run(id, params), &steps) {
        return fail(serde_json::to_value(&err));
    }
    if let Err(err) = save(runner, args) {
        return fail(serde_json::to_value(&err));
    }
    print!("{}", journal::export(runner.journal()));
    ExitCode::SUCCESS
}

/// `--project`: opens the Project first, unless the bridged instance already has it.
fn open(runner: &mut Runner, args: &RunArgs) -> Result<(), CommandError> {
    if let (Runner::Local(engine), Some(project)) = (runner, &args.project) {
        engine.run("project.open", json!({ "path": project }))?;
    }
    Ok(())
}

/// `--save`: saves every file last. A bridged run never saves.
fn save(runner: &mut Runner, args: &RunArgs) -> Result<(), CommandError> {
    match runner {
        _ if !args.save => {}
        Runner::Local(engine) => {
            engine.run("project.save", json!({}))?;
        }
        Runner::Bridged(connection, _) => eprintln!("{}", bridge::not_saving(connection)),
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
