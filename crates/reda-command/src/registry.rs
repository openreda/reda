//! The registry: every Command by id, and the one place Commands are run.

use std::collections::BTreeMap;

use reda_model::{ChangeSet, Scope};
use serde_json::{Map, Value, json};

use crate::command::{AvailabilityContext, Unavailable};
use crate::context::{Access, Context, Env, Session};
use crate::{Command, CommandError, Kind, Meta};

/// The object-safe face of a [`Command`].
trait Erased: Send + Sync {
    fn meta(&self) -> Meta;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable>;
    fn invoke(&self, cx: &mut Context<'_>, params: Value) -> Result<Value, CommandError>;
}

impl<C: Command> Erased for C {
    fn meta(&self) -> Meta {
        C::META
    }

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        Command::available(self, cx)
    }

    fn invoke(&self, cx: &mut Context<'_>, params: Value) -> Result<Value, CommandError> {
        let params = if params.is_null() { json!({}) } else { params };
        let params: C::Params = serde_json::from_value(params)
            .map_err(|e| CommandError::invalid_params("", e.to_string()))?;
        let output = self.run(cx, params)?;
        serde_json::to_value(output)
            .map_err(|e| CommandError::internal(format!("serialising the result: {e}")))
    }
}

/// Every registered Command, keyed by id.
#[derive(Default)]
pub struct Registry {
    commands: BTreeMap<&'static str, Box<dyn Erased>>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Adds a Command. Panics if its id is taken: two crates claiming one id is a bug.
    pub fn add<C: Command>(&mut self, command: C) {
        let id = C::META.id;
        if self.commands.insert(id, Box::new(command)).is_some() {
            panic!("Command {id} is registered twice");
        }
    }

    pub fn get(&self, id: &str) -> Option<Meta> {
        self.commands.get(id).map(|c| c.meta())
    }

    /// Every Command's metadata, sorted by id.
    pub fn commands(&self) -> impl Iterator<Item = Meta> + '_ {
        self.commands.values().map(|c| c.meta())
    }

    /// Runs one Command against the session.
    ///
    /// An edit runs in one transaction: on success it returns the envelope
    /// `{changes: {<scope>: {created, modified, deleted}}, result}` and marks the touched
    /// scopes dirty; on failure it is rolled back fully. Other kinds return their result
    /// bare.
    pub fn run(
        &self,
        session: &mut Session,
        env: &mut Env<'_>,
        id: &str,
        params: Value,
    ) -> Result<Value, CommandError> {
        let command = self
            .commands
            .get(id)
            .ok_or_else(|| CommandError::not_found(format!("no Command has the id {id}")))?;
        let meta = command.meta();
        if meta.kind == Kind::View && env.headless {
            return Err(CommandError::unavailable("requires the REDA app"));
        }
        let availability = AvailabilityContext {
            project: session.project.as_ref(),
        };
        command
            .available(&availability)
            .map_err(|u| CommandError::unavailable(u.reason))?;

        match meta.kind {
            Kind::Edit => {
                let project = session
                    .project
                    .as_mut()
                    .ok_or_else(|| CommandError::unavailable(crate::command::NO_PROJECT))?;
                let mut cx = Context {
                    access: Access::Edit(project.transaction()),
                    ids: &mut *env.ids,
                    clock: env.clock,
                };
                let result = command.invoke(&mut cx, params);
                let Access::Edit(tx) = cx.access else {
                    unreachable!("an edit context holds its transaction")
                };
                let result = result?;
                let changes = tx.commit()?;
                session.dirty.extend(changes.scopes());
                Ok(json!({ "changes": changes_json(&changes), "result": result }))
            }
            Kind::Query | Kind::View => {
                let mut cx = Context {
                    access: Access::Read(session.project.as_ref()),
                    ids: &mut *env.ids,
                    clock: env.clock,
                };
                command.invoke(&mut cx, params)
            }
            Kind::File => {
                let mut cx = Context {
                    access: Access::File(session),
                    ids: &mut *env.ids,
                    clock: env.clock,
                };
                command.invoke(&mut cx, params)
            }
        }
    }
}

/// The key of a scope in the edit envelope: `"project"` or the Document's UUID.
pub fn scope_key(scope: Scope) -> String {
    match scope {
        Scope::Project => "project".to_owned(),
        Scope::Document(uuid) => uuid.to_string(),
    }
}

fn changes_json(changes: &ChangeSet) -> Value {
    let mut out = Map::new();
    for (scope, summary) in changes.summary() {
        let summary = serde_json::to_value(summary).expect("UUID lists serialise");
        out.insert(scope_key(scope), summary);
    }
    Value::Object(out)
}
