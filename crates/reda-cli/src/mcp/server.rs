//! The hand-written rmcp `ServerHandler`, driven by the Command registry (ADR-0007).
//!
//! Tools: the generic `search_commands`, `describe_command` and `run_command`, plus each
//! of [`QUERY_TOOLS`] the registry has, named by its Command id. Every Command runs on the
//! single [`Writer`]. A Command error comes back as a tool result with `isError` and the
//! `{code, message, details?}` error as structured content, so the model can read it.

use std::sync::Arc;

use reda_engine::command::{CancelToken, CommandError, Progress, Signals};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, PaginatedRequestParams, ProgressNotificationParam,
    ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::search::{self, Query};
use super::writer::Writer;

/// The query Commands offered as tools of their own when the registry has them
/// (architecture.md#mcp). The others are reachable through `run_command`.
pub const QUERY_TOOLS: &[&str] = &[
    "project.overview",
    "objects.query",
    "object.get",
    "erc.run",
    "drc.run",
    "sync.preview",
    "document.render",
];

const INSTRUCTIONS: &str = "\
REDA is an electronics design tool. Every action is a Command with an id such as \
`project.save`. Find Commands with `search_commands`, read a Command's params and result \
schemas with `describe_command`, and run it with `run_command {id, params}`. An edit \
returns `{changes, result}`, where `result` fits the Command's result schema. Run several \
edits as one undo step with the `batch` Command. Nothing is written to disk until you run \
`project.save`; unsaved changes are discarded when the server exits.";

const BRIDGED_INSTRUCTIONS: &str = "\
REDA is an electronics design tool. Every action is a Command with an id such as \
`project.overview`. Find Commands with `search_commands`, read a Command's params and result \
schemas with `describe_command`, and run it with `run_command {id, params}`. An edit \
returns `{changes, result}`, where `result` fits the Command's result schema. Run several \
edits as one undo step with the `batch` Command. This session is bridged to the REDA app, \
which has the Project open: your edits land in its undo stacks and show there as unsaved \
changes. The user saves; `project.save`, `project.open` and `project.new` are refused.";

/// The MCP server: the catalogue, worked out once, and the writer that runs Commands.
#[derive(Clone)]
pub struct Server {
    tools: Arc<Vec<Tool>>,
    commands: Arc<Vec<Value>>,
    writer: Writer,
    instructions: &'static str,
}

impl Server {
    /// `catalogue` is the registry's [`reda_engine::command::Registry::catalogue`].
    pub fn new(writer: Writer, catalogue: &Value) -> Server {
        let commands = catalogue["commands"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let tools = tools(&commands);
        Server {
            tools: Arc::new(tools),
            commands: Arc::new(commands),
            writer,
            instructions: INSTRUCTIONS,
        }
    }

    /// [`Server::new`] for a writer bridged to a running app, which never saves.
    pub fn bridged(writer: Writer, catalogue: &Value) -> Server {
        Server {
            instructions: BRIDGED_INSTRUCTIONS,
            ..Server::new(writer, catalogue)
        }
    }

    fn command(&self, id: &str) -> Option<&Value> {
        let base = id.split_once(':').map_or(id, |(base, _)| base);
        self.commands.iter().find(|c| c["id"] == base)
    }

    fn describe(&self, args: &Value) -> CallToolResult {
        let Some(id) = args.get("id").and_then(Value::as_str) else {
            return failure(&CommandError::invalid_params("/id", "id must be a string"));
        };
        match self.command(id) {
            Some(command) => CallToolResult::structured(command.clone()),
            None => failure(&CommandError::not_found(format!(
                "no Command has the id {id}"
            ))),
        }
    }

    /// Runs a Command on the writer, forwarding its progress to the client while it runs
    /// and cancelling it when the client cancels the request.
    async fn run(
        &self,
        id: &str,
        params: Value,
        cx: &RequestContext<RoleServer>,
    ) -> CallToolResult {
        let cancel = CancelToken::new();
        let mut signals = Signals::new(cancel.clone());
        let mut forwarder = None;
        if let Some(token) = cx.meta.get_progress_token() {
            let (tx, mut rx) = mpsc::unbounded_channel::<(f64, Option<f64>, String)>();
            signals = signals.with_progress(Arc::new(ChannelProgress(tx)));
            let peer = cx.peer.clone();
            forwarder = Some(tokio::spawn(async move {
                while let Some((progress, total, message)) = rx.recv().await {
                    let mut param = ProgressNotificationParam::new(token.clone(), progress);
                    if let Some(total) = total {
                        param = param.with_total(total);
                    }
                    if !message.is_empty() {
                        param = param.with_message(message);
                    }
                    let _ = peer.notify_progress(param).await;
                }
            }));
        }
        let ct = cx.ct.clone();
        let watcher = tokio::spawn(async move {
            ct.cancelled().await;
            cancel.cancel();
        });
        let outcome = self.writer.run(id, params, signals).await;
        watcher.abort();
        if let Some(forwarder) = forwarder {
            let _ = forwarder.await;
        }
        match outcome {
            Ok(Value::Object(result)) => CallToolResult::structured(Value::Object(result)),
            Ok(result) => CallToolResult::success(vec![ContentBlock::text(result.to_string())]),
            Err(err) => failure(&err),
        }
    }
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("reda", env!("CARGO_PKG_VERSION")))
            .with_instructions(self.instructions)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _cx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.as_ref().clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        cx: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = match request.name.as_ref() {
            "search_commands" => match Query::parse(&args) {
                Ok(query) => CallToolResult::structured(search::search(&self.commands, &query)),
                Err(message) => failure(&CommandError::invalid_params("", message)),
            },
            "describe_command" => self.describe(&args),
            "run_command" => match run_args(args) {
                Ok((id, params)) => self.run(&id, params, &cx).await,
                Err(err) => failure(&err),
            },
            name if self.tools.iter().skip(3).any(|t| t.name == name) => {
                self.run(name, args, &cx).await
            }
            name => {
                return Err(ErrorData::invalid_params(
                    format!("no tool is named {name}"),
                    None,
                ));
            }
        };
        Ok(result.into())
    }
}

/// `run_command {id, params?}`.
fn run_args(args: Value) -> Result<(String, Value), CommandError> {
    let Value::Object(mut args) = args else {
        unreachable!("tool arguments are an object")
    };
    let id = match args.remove("id") {
        Some(Value::String(id)) => id,
        _ => return Err(CommandError::invalid_params("/id", "id must be a string")),
    };
    let params = args.remove("params").unwrap_or(Value::Null);
    if let Some(key) = args.keys().next() {
        return Err(CommandError::invalid_params(
            &format!("/{key}"),
            format!("unknown field {key}"),
        ));
    }
    Ok((id, params))
}

/// A failed Command as a tool result the model reads: `isError` with the error JSON.
fn failure(err: &CommandError) -> CallToolResult {
    CallToolResult::structured_error(serde_json::to_value(err).expect("errors serialise"))
}

/// Sends a running Command's progress to the task that notifies the client.
struct ChannelProgress(mpsc::UnboundedSender<(f64, Option<f64>, String)>);

impl Progress for ChannelProgress {
    fn report(&self, progress: f64, total: Option<f64>, message: &str) {
        let _ = self.0.send((progress, total, message.to_owned()));
    }
}

/// Every tool: the three generic ones first, then the query Commands in [`QUERY_TOOLS`]
/// order.
fn tools(commands: &[Value]) -> Vec<Tool> {
    let read_only = ToolAnnotations::new().read_only(true);
    let mut tools = vec![
        Tool::new(
            "search_commands",
            "Finds REDA Commands by words in their id, title or description, optionally only \
             of one kind or category. Returns each hit's id, title, description, kind and \
             Preset ids.",
            object(search::params_schema()),
        )
        .with_title("Search Commands")
        .with_raw_output_schema(object(search::result_schema()))
        .with_annotations(read_only.clone()),
        Tool::new(
            "describe_command",
            "Describes one REDA Command by id (or `<id>:<preset>`): its title, description, \
             kind, category, JSON Schemas for its params and its result, and its Presets.",
            object(json!({
                "type": "object",
                "properties": {"id": {"type": "string", "description": "The Command id."}},
                "required": ["id"],
                "additionalProperties": false
            })),
        )
        .with_title("Describe Command")
        .with_raw_output_schema(object(json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "title": {"type": "string"},
                "description": {"type": "string"},
                "category": {"type": "string"},
                "icon": {"type": "string"},
                "kind": {"type": "string", "enum": ["edit", "query", "view", "file"]},
                "params": {"type": "object", "description": "The params JSON Schema."},
                "result": {"description": "The result JSON Schema. An edit returns it wrapped as `{changes, result}`."},
                "presets": {"type": "array"}
            },
            "required": ["id", "title", "description", "category", "icon", "kind", "params", "result", "presets"]
        })))
        .with_annotations(read_only.clone()),
        Tool::new(
            "run_command",
            "Runs one REDA Command (or a Preset as `<id>:<preset>`, with no params) and \
             returns its result. The result fits the schema `describe_command` gives; an \
             edit wraps it as `{changes: {<document>: {created, modified, deleted}}, \
             result}`. A failure returns `{code, message, details?}` with isError.",
            object(json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "The Command or Preset id."},
                    "params": {"type": "object", "description": "The Command's params, as its params schema describes. Leave out for none."}
                },
                "required": ["id"],
                "additionalProperties": false
            })),
        )
        .with_title("Run Command"),
    ];
    for &id in QUERY_TOOLS {
        let Some(command) = commands.iter().find(|c| c["id"] == id) else {
            continue;
        };
        let mut tool = Tool::new(
            id,
            command["description"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            object(command["params"].clone()),
        )
        .with_title(command["title"].as_str().unwrap_or_default())
        .with_annotations(read_only.clone());
        if command["result"]["type"] == "object" {
            tool = tool.with_raw_output_schema(object(command["result"].clone()));
        }
        tools.push(tool);
    }
    tools
}

fn object(schema: Value) -> Arc<JsonObject> {
    match schema {
        Value::Object(map) => Arc::new(map),
        _ => Arc::new(JsonObject::new()),
    }
}
