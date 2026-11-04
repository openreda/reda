//! The registry: every Command by id, and the one place Commands are run.

use std::collections::BTreeMap;

use reda_model::{ChangeSet, Project, Scope, Transaction};

use crate::History;
use serde_json::{Map, Value, json};

use crate::batch::{self, BatchParams};
use crate::command::{AvailabilityContext, NO_PROJECT, Preset, Unavailable};
use crate::context::{Access, Context, Env, Session, Step};
use crate::schema;
use crate::{Clock, Command, CommandError, Fonts, IdSource, Kind, Meta, Signals, UserDirs};

/// The object-safe face of a [`Command`].
trait Erased: Send + Sync {
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable>;
    fn presets(&self) -> Vec<Preset>;
    /// Deserialises params that already passed the schema check, and runs.
    fn invoke(&self, cx: &mut Context<'_>, params: Value) -> Result<Value, CommandError>;
}

impl<C: Command> Erased for C {
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        Command::available(self, cx)
    }

    fn presets(&self) -> Vec<Preset> {
        Command::presets(self)
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
}

impl Registered {
    /// The Command's Presets, asked of it each time: some, such as Saved Layouts, change
    /// while REDA runs.
    fn presets(&self) -> Vec<Preset> {
        match &self.runner {
            Runner::Command(command) => command.presets(),
            Runner::Batch => Vec::new(),
        }
    }
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
        // The Presets the Command has now are checked here; later ones are checked against
        // the schema when they run, like any params.
        for preset in &command.presets() {
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
        self.available_with(id, project, None, headless)
    }

    /// [`Registry::available`] with the workbench of Library elements open while no
    /// Project is ([`Session::workbench`]).
    pub fn available_with(
        &self,
        id: &str,
        project: Option<&Project>,
        workbench: Option<&Project>,
        headless: bool,
    ) -> Result<(), CommandError> {
        let resolved = self.resolve(id, Value::Null)?;
        check_available(resolved.entry, (project, workbench), headless)
    }

    /// Checks params for a Command or Preset id against its schema without running it:
    /// the `invalid_params` error [`Registry::run`] would give, or `Ok`. Forms use it to
    /// tell whether the params are complete.
    pub fn check_params(&self, id: &str, params: &Value) -> Result<(), CommandError> {
        let resolved = self.resolve(id, params.clone())?;
        check_params(resolved.entry, &resolved.params)
    }

    /// Checks one top-level param of the Command `id` on its own, so a form can flag each
    /// field as it is typed. The error's pointer starts at `/<name>`.
    pub fn check_param(&self, id: &str, name: &str, value: &Value) -> Result<(), CommandError> {
        let entry = self.resolve(id, Value::Null)?.entry;
        schema::check_property(&entry.params_schema, name, value).map_err(|m| {
            CommandError::invalid_params(&m.pointer, format!("{}: {}", m.pointer, m.message))
        })
    }

    /// The JSON Schema of the params of the Command `id` (a Preset id names its Command).
    pub fn params_schema(&self, id: &str) -> Option<&Value> {
        let command = id.split_once(':').map_or(id, |(c, _)| c);
        self.commands.get(command).map(|e| &e.params_schema)
    }

    /// Whether `id` could run now with `params`: everything [`Registry::run`] checks
    /// before the Command starts (known id, availability, params against the schema).
    pub fn check(
        &self,
        id: &str,
        params: &Value,
        project: Option<&Project>,
        headless: bool,
    ) -> Result<(), CommandError> {
        let Resolved { entry, params } = self.resolve(id, params.clone())?;
        check_available(entry, (project, None), headless)?;
        check_params(entry, &params)
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
        let open = (session.project.as_ref(), session.workbench.as_ref());
        check_available(entry, open, env.headless)?;
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
                    .or(session.workbench.as_mut())
                    .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
                // The Context reads the stacks while the transaction holds the Project; they
                // move only once it has committed.
                let (result, changes, step) = {
                    let run = invoke_in(
                        command.as_ref(),
                        Kind::Edit,
                        (
                            project.transaction(),
                            &session.history,
                            session.user_dirs.as_ref(),
                        ),
                        &mut *env.ids,
                        (env.clock, env.parts, env.fonts),
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
                let project = session.project.as_ref().or(session.workbench.as_ref());
                let access = match entry.meta.kind {
                    Kind::View => Access::View(project, &mut session.view),
                    _ => Access::Read(project),
                };
                let mut cx = Context {
                    access,
                    ids: &mut *env.ids,
                    clock: env.clock,
                    parts: env.parts,
                    fonts: env.fonts,
                    signals,
                    user_dirs: session.user_dirs.as_ref(),
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
                    fonts: env.fonts,
                    signals,
                    user_dirs: None,
                    step: None,
                };
                command.invoke(&mut cx, params)
            }
        }
    }

    /// Runs the edit Command `id` on a copy of `project` and returns that copy with the
    /// Command's result: what the Project would be if it ran. Nothing else changes, so a
    /// Tool can draw an edit before the user commits it. Only edit Commands preview.
    pub fn preview(
        &self,
        project: &Project,
        history: &History,
        env: &mut Env<'_>,
        id: &str,
        params: Value,
    ) -> Result<(Project, Value), CommandError> {
        let Resolved { entry, params } = self.resolve(id, params)?;
        check_available(entry, (Some(project), None), env.headless)?;
        check_params(entry, &params)?;
        let command = match (&entry.runner, entry.meta.kind) {
            (Runner::Command(command), Kind::Edit) => command,
            _ => {
                return Err(CommandError::rejected(format!(
                    "{} is not an edit Command, so it has nothing to preview",
                    entry.meta.id
                )));
            }
        };
        let mut copy = project.clone();
        let signals = Signals::default();
        let result = {
            let run = invoke_in(
                command.as_ref(),
                Kind::Edit,
                (copy.transaction(), history, None),
                &mut *env.ids,
                (env.clock, env.parts, env.fonts),
                &signals,
                params,
            );
            let result = run.result?;
            run.tx.commit()?;
            result
        };
        Ok((copy, result))
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
        let on_workbench = session.project.is_none();
        let project = session
            .project
            .as_mut()
            .or(session.workbench.as_mut())
            .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let history = &session.history;
        let user_dirs = session.user_dirs.as_ref();
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
            let open = match on_workbench {
                true => (None, Some(tx.project())),
                false => (Some(tx.project()), None),
            };
            check_available(entry, open, env.headless).map_err(at)?;
            check_params(entry, &params).map_err(at)?;
            let run = invoke_in(
                command.as_ref(),
                entry.meta.kind,
                (tx, history, user_dirs),
                ids,
                (env.clock, env.parts, env.fonts),
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
            .presets()
            .into_iter()
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
            params: preset.params,
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
    (tx, history, user_dirs): (Transaction<'a>, &'a History, Option<&'a UserDirs>),
    ids: &'a mut dyn IdSource,
    (clock, parts, fonts): (&'a dyn Clock, &'a dyn crate::PartSource, &'a Fonts),
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
        fonts,
        signals,
        user_dirs,
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
    (project, workbench): (Option<&Project>, Option<&Project>),
    headless: bool,
) -> Result<(), CommandError> {
    if entry.meta.kind == Kind::View && headless {
        return Err(CommandError::unavailable("requires the REDA app"));
    }
    let cx = AvailabilityContext { project, workbench };
    let available = match &entry.runner {
        Runner::Command(command) => command.available(&cx),
        Runner::Batch => match project.or(workbench) {
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
        .presets()
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
