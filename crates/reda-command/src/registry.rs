//! The registry: every Command by id, and the one place Commands are run.

use std::collections::BTreeMap;

use reda_model::{ChangeSet, Project, Scope, Transaction};

use crate::History;
use serde_json::{Map, Value, json};

use crate::batch::{self, BatchParams};
use crate::command::{AvailabilityContext, NO_PROJECT, Preset, Unavailable};
use crate::context::{Access, Context, Env, Session, Step};
use crate::schema;
use crate::{Clock, Command, CommandError, IdSource, Kind, Meta, Signals};

/// The object-safe face of a [`Command`].
trait Erased: Send + Sync {
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable>;
    /// Deserialises params that already passed the schema check, and runs.
    fn invoke(&self, cx: &mut Context<'_>, params: Value) -> Result<Value, CommandError>;
}

impl<C: Command> Erased for C {
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        Command::available(self, cx)
    }

    fn invoke(&self, cx: &mut Context<'_>, params: Value) -> Result<Value, CommandError> {
        let params: C::Params = serde_json::from_value(params)
            .map_err(|e| CommandError::invalid_params("", e.to_string()))?;
        let output = self.run(cx, params)?;
        serde_json::to_value(output)
            .map_err(|e| CommandError::internal(format!("serialising the result: {e}")))
    }
}

/// How an entry runs: an ordinary Command, or the built-in `batch`.
enum Runner {
    Command(Box<dyn Erased>),
    Batch,
}

/// One registered Command with what is worked out once, at registration.
struct Registered {
    meta: Meta,
    runner: Runner,
    params_schema: Value,
    result_schema: Value,
    presets: Vec<Preset>,
}

/// Every registered Command, keyed by id. It always holds the built-in `batch`.
pub struct Registry {
    commands: BTreeMap<&'static str, Registered>,
}

impl Default for Registry {
    fn default() -> Registry {
        let mut commands = BTreeMap::new();
        commands.insert(
            batch::META.id,
            Registered {
                meta: batch::META,
                runner: Runner::Batch,
                params_schema: schema::params_schema::<BatchParams>(),
                result_schema: schema::result_schema::<Vec<Value>>(),
                presets: Vec::new(),
            },
        );
        Registry { commands }
    }
}

/// The access an entry needs to run, worked out from its kind.
struct Resolved<'r> {
    entry: &'r Registered,
    params: Value,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Adds a Command. Panics if its id is taken, its id is not a valid MCP tool name, or
    /// one of its Presets does not fit its params schema: each is a bug in the Command.
    pub fn add<C: Command>(&mut self, command: C) {
        let meta = C::META;
        let id = meta.id;
        check_id(id);
        let params_schema = schema::params_schema::<C::Params>();
        let presets = command.presets();
        for preset in &presets {
            check_id(&preset.name);
            assert!(
                !preset.name.contains('.'),
                "Preset {id}:{} must not contain a dot",
                preset.name
            );
            if let Err(m) = schema::check(&params_schema, &preset.params) {
                panic!(
                    "Preset {id}:{} has params that do not fit: {} at {:?}",
                    preset.name, m.message, m.pointer
                );
            }
        }
        let entry = Registered {
            meta,
            runner: Runner::Command(Box::new(command)),
            params_schema,
            result_schema: schema::result_schema::<C::Output>(),
            presets,
        };
        if self.commands.insert(id, entry).is_some() {
            panic!("Command {id} is registered twice");
        }
    }

    pub fn get(&self, id: &str) -> Option<Meta> {
        self.commands.get(id).map(|e| e.meta)
    }

    /// Every Command's metadata, sorted by id.
    pub fn commands(&self) -> impl Iterator<Item = Meta> + '_ {
        self.commands.values().map(|e| e.meta)
    }

    /// One Command as the catalogue lists it: its metadata, its params and result schemas,
    /// and its Presets.
    pub fn describe(&self, id: &str) -> Option<Value> {
        self.commands.get(id).map(describe)
    }

    /// The whole registry, sorted by id: `{"commands": [describe(id)…]}`. `reda commands
    /// --json` prints it and `docs/commands.json` is a checked-in copy.
    pub fn catalogue(&self) -> Value {
        let commands: Vec<Value> = self.commands.values().map(describe).collect();
        json!({ "commands": commands })
    }

    /// Whether a Command or Preset id can run now, and if not, why. Menus grey out an
    /// unavailable Command and show the reason.
    pub fn available(
        &self,
        id: &str,
        project: Option<&Project>,
        headless: bool,
    ) -> Result<(), CommandError> {
        let resolved = self.resolve(id, Value::Null)?;
        check_available(resolved.entry, project, headless)
    }

    /// Runs one Command, or one Preset as `<id>:<preset>`, against the session.
    ///
    /// The params are checked against the Command's schema first; a misfit is
    /// `invalid_params` with a JSON pointer to the bad field. An edit runs in one transaction:
    /// on success it returns the envelope `{changes: {<scope>: {created, modified, deleted}},
    /// result}` and marks the touched scopes dirty; on failure it is rolled back fully. Other
    /// kinds return their result bare.
    pub fn run(
        &self,
        session: &mut Session,
        env: &mut Env<'_>,
        id: &str,
        params: Value,
    ) -> Result<Value, CommandError> {
        self.run_with(session, env, id, params, &Signals::default())
    }

    /// [`Registry::run`] with a cancellation token and progress reporter the Command can
    /// use while it runs (see [`Signals`]). Every step of a `batch` shares them.
    pub fn run_with(
        &self,
        session: &mut Session,
        env: &mut Env<'_>,
        id: &str,
        params: Value,
        signals: &Signals,
    ) -> Result<Value, CommandError> {
        let Resolved { entry, params } = self.resolve(id, params)?;
        check_available(entry, session.project.as_ref(), env.headless)?;
        check_params(entry, &params)?;

        let command = match &entry.runner {
            Runner::Batch => return self.run_batch(session, env, params, signals),
            Runner::Command(command) => command,
        };
        match entry.meta.kind {
            Kind::Edit => {
                let project = session
                    .project
                    .as_mut()
                    .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
                // The Context reads the stacks while the transaction holds the Project; they
                // move only once it has committed.
                let (result, changes, step) = {
                    let run = invoke_in(
                        command.as_ref(),
                        Kind::Edit,
                        (project.transaction(), &session.history),
                        &mut *env.ids,
                        (env.clock, env.parts),
                        signals,
                        params,
                    );
                    let result = run.result?;
                    (result, run.tx.commit()?, run.step)
                };
                let history = &mut session.history;
                match step {
                    None => history.record(entry.meta.title, &changes),
                    Some(Step::Undo(scope)) => history.undone(scope),
                    Some(Step::Redo(scope)) => history.redone(scope),
                }
                Ok(envelope(&changes, result))
            }
            Kind::Query | Kind::View => {
                let mut cx = Context {
                    access: Access::Read(session.project.as_ref()),
                    ids: &mut *env.ids,
                    clock: env.clock,
                    parts: env.parts,
                    signals,
                    step: None,
                };
                command.invoke(&mut cx, params)
            }
            Kind::File => {
                let mut cx = Context {
                    access: Access::File(session),
                    ids: &mut *env.ids,
                    clock: env.clock,
                    parts: env.parts,
                    signals,
                    step: None,
                };
                command.invoke(&mut cx, params)
            }
        }
    }

    /// Runs `batch {commands}`: every step in one transaction, so a failing step rolls back
    /// the steps before it too.
    fn run_batch(
        &self,
        session: &mut Session,
        env: &mut Env<'_>,
        params: Value,
        signals: &Signals,
    ) -> Result<Value, CommandError> {
        let params: BatchParams = serde_json::from_value(params)
            .map_err(|e| CommandError::invalid_params("", e.to_string()))?;
        let project = session
            .project
            .as_mut()
            .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let history = &session.history;
        let mut tx = project.transaction();
        let mut ids: &mut dyn IdSource = &mut *env.ids;
        let mut results = Vec::with_capacity(params.commands.len());
        for (i, step) in params.commands.into_iter().enumerate() {
            let at = |err: CommandError| batch::step_error(i, &step.id, err);
            let Resolved { entry, params } =
                self.resolve(&step.id, step.params.clone()).map_err(at)?;
            let command = match (&entry.runner, entry.meta.kind) {
                (Runner::Command(command), Kind::Edit | Kind::Query) => command,
                (_, kind) => return Err(at(batch::refused_step(entry.meta.id, kind))),
            };
            check_available(entry, Some(tx.project()), env.headless).map_err(at)?;
            check_params(entry, &params).map_err(at)?;
            let run = invoke_in(
                command.as_ref(),
                entry.meta.kind,
                (tx, history),
                ids,
                (env.clock, env.parts),
                signals,
                params,
            );
            tx = run.tx;
            ids = run.ids;
            let result = run.result.map_err(at)?;
            if run.step.is_some() {
                return Err(at(CommandError::rejected(
                    "undo and redo cannot be batch steps",
                )));
            }
            results.push(result);
        }
        let changes = tx.commit()?;
        session.history.record(batch::META.title, &changes);
        Ok(envelope(&changes, Value::Array(results)))
    }

    /// Finds the entry for a Command id or a `<id>:<preset>` id. A Preset brings its own
    /// params, so the caller may pass none.
    fn resolve(&self, id: &str, params: Value) -> Result<Resolved<'_>, CommandError> {
        let (command_id, preset) = match id.split_once(':') {
            Some((command_id, preset)) => (command_id, Some(preset)),
            None => (id, None),
        };
        let entry = self.commands.get(command_id).ok_or_else(|| {
            CommandError::not_found(format!("no Command has the id {command_id}"))
        })?;
        let params = match params {
            Value::Null => json!({}),
            params => params,
        };
        let Some(preset) = preset else {
            return Ok(Resolved { entry, params });
        };
        let preset = entry
            .presets
            .iter()
            .find(|p| p.name == preset)
            .ok_or_else(|| {
                CommandError::not_found(format!("{command_id} has no Preset {preset}"))
            })?;
        if params != json!({}) {
            return Err(CommandError::invalid_params(
                "",
                format!("the Preset {id} has fixed params; pass none"),
            ));
        }
        Ok(Resolved {
            entry,
            params: preset.params.clone(),
        })
    }
}

/// What [`invoke_in`] hands back: the transaction and id source, so a batch can thread
/// them through its steps, the undo or redo the Command carried out, and its result.
struct InTx<'a> {
    tx: Transaction<'a>,
    ids: &'a mut dyn IdSource,
    step: Option<Step>,
    result: Result<Value, CommandError>,
}

/// Runs a Command inside an open transaction. A query step sees the transaction's Project
/// but cannot change it.
fn invoke_in<'a>(
    command: &dyn Erased,
    kind: Kind,
    (tx, history): (Transaction<'a>, &'a History),
    ids: &'a mut dyn IdSource,
    (clock, parts): (&'a dyn Clock, &'a dyn crate::PartSource),
    signals: &'a Signals,
    params: Value,
) -> InTx<'a> {
    let access = match kind {
        Kind::Edit => Access::Edit(tx, history),
        _ => Access::ReadInEdit(tx),
    };
    let mut cx = Context {
        access,
        ids,
        clock,
        parts,
        signals,
        step: None,
    };
    let result = command.invoke(&mut cx, params);
    let Context {
        access, ids, step, ..
    } = cx;
    let (Access::Edit(tx, _) | Access::ReadInEdit(tx)) = access else {
        unreachable!("an edit context holds its transaction")
    };
    InTx {
        tx,
        ids,
        step,
        result,
    }
}

fn check_available(
    entry: &Registered,
    project: Option<&Project>,
    headless: bool,
) -> Result<(), CommandError> {
    if entry.meta.kind == Kind::View && headless {
        return Err(CommandError::unavailable("requires the REDA app"));
    }
    let cx = AvailabilityContext { project };
    let available = match &entry.runner {
        Runner::Command(command) => command.available(&cx),
        Runner::Batch => match project {
            Some(_) => Ok(()),
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
        },
    };
    available.map_err(|u| CommandError::unavailable(u.reason))
}

fn check_params(entry: &Registered, params: &Value) -> Result<(), CommandError> {
    schema::check(&entry.params_schema, params).map_err(|m| {
        let at = if m.pointer.is_empty() {
            "params"
        } else {
            &m.pointer
        };
        CommandError::invalid_params(&m.pointer, format!("{at}: {}", m.message))
    })
}

/// Command ids and Preset names must be valid MCP tool names: 1–128 characters from
/// `[a-z0-9_.]`, lowercase being REDA's own rule.
fn check_id(id: &str) {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.');
    assert!(valid, "{id:?} is not a valid Command or Preset id");
}

fn describe(entry: &Registered) -> Value {
    let meta = entry.meta;
    let presets: Vec<Value> = entry
        .presets
        .iter()
        .map(|p| {
            json!({
                "id": format!("{}:{}", meta.id, p.name),
                "title": p.title,
                "icon": p.icon,
                "params": p.params,
            })
        })
        .collect();
    json!({
        "id": meta.id,
        "title": meta.title,
        "description": meta.description,
        "category": meta.category,
        "icon": meta.icon,
        "kind": meta.kind,
        "params": entry.params_schema,
        "result": entry.result_schema,
        "presets": presets,
    })
}

/// The key of a scope in the edit envelope: `"project"`, or the Document's or open Library
/// element's UUID.
pub fn scope_key(scope: Scope) -> String {
    match scope {
        Scope::Project => "project".to_owned(),
        Scope::Document(uuid) | Scope::Element(uuid) => uuid.to_string(),
    }
}

/// `{changes: {<scope>: {created, modified, deleted}}, result}`.
fn envelope(changes: &ChangeSet, result: Value) -> Value {
    let mut out = Map::new();
    for (scope, summary) in changes.summary() {
        let summary = serde_json::to_value(summary).expect("UUID lists serialise");
        out.insert(scope_key(scope), summary);
    }
    json!({ "changes": Value::Object(out), "result": result })
}
