//! The client side: find the running instances and talk to one.
//!
//! [`bridge`] picks the instance the CLI and MCP server must bridge to (ADR-0007): the one
//! with the target Project open, or, with no Project named, the only instance that has a
//! Project open. Otherwise they run in-process.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use reda_engine::command::{CommandError, ErrorCode, Signals, UserDirs};
use reda_engine::journal::Source;
use reda_engine::store;
use serde_json::{Value, json};

use crate::normalize;
use crate::protocol::{self, InstanceFile};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// The app may hold the answer to `auth` while the user decides whether to allow the agent.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How often a run waiting for its answer checks whether its caller cancelled it.
const CANCEL_POLL: Duration = Duration::from_millis(20);

/// The environment variable that names the agent in the app's "Allow this agent?" prompt.
pub const AGENT_VAR: &str = "REDA_AGENT";

/// Every readable instance file, by file name. Some may be stale (their instance died).
pub fn instances(dirs: &UserDirs) -> Vec<InstanceFile> {
    let Ok(entries) = std::fs::read_dir(dirs.instances_dir()) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|p| serde_json::from_slice(&std::fs::read(p).ok()?).ok())
        .collect()
}

/// The agent name this process gives: [`AGENT_VAR`] when set, else a name for `source`.
pub fn agent_name(source: Source) -> String {
    match std::env::var(AGENT_VAR) {
        Ok(name) if !name.trim().is_empty() => name.trim().to_owned(),
        _ => protocol::default_agent_name(source).to_owned(),
    }
}

/// Connects to the instance to bridge to, if there is one: with `project` (a `.reda` file
/// or its folder), the live instance that has it open; without, the only live instance
/// with any Project open. `Ok(None)` means run in-process. An instance whose user did not
/// allow this agent is a `rejected` error: the Project is open there, so running
/// in-process instead would edit it behind the app's back.
pub fn bridge(
    dirs: &UserDirs,
    project: Option<&Path>,
    source: Source,
) -> Result<Option<Connection>, CommandError> {
    let agent = agent_name(source);
    let all = instances(dirs);
    // Only live instances, and only the one picked is asked to admit this agent.
    let live: Vec<&InstanceFile> = match project {
        Some(project) => {
            let wanted = normalize(&store::resolve_project_file(project));
            all.iter()
                .filter(|i| i.project.as_deref() == Some(wanted.as_path()))
                .filter(|i| alive(i))
                .take(1)
                .collect()
        }
        None => all
            .iter()
            .filter(|i| i.project.is_some())
            .filter(|i| alive(i))
            .collect(),
    };
    let [instance] = live[..] else {
        return Ok(None);
    };
    match Connection::connect_as(instance, source, &agent) {
        Ok(connection) => Ok(Some(connection)),
        Err(e) if e.kind() == ErrorKind::PermissionDenied => Err(CommandError::rejected(format!(
            "a running REDA app has the Project open but did not allow this agent \
             ({agent}): {e}. Allow it when REDA asks, or under Agents"
        ))),
        Err(_) => Ok(None),
    }
}

/// Whether `instance`'s port answers.
fn alive(instance: &InstanceFile) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, instance.port));
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok()
}

/// An authenticated connection to one instance. Requests go one at a time.
pub struct Connection {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    next_id: u64,
    project: Option<PathBuf>,
}

impl Connection {
    /// Connects and presents the instance's token, naming `source` for its Journal and
    /// [`agent_name`] for its prompt.
    pub fn connect(instance: &InstanceFile, source: Source) -> std::io::Result<Connection> {
        Connection::connect_as(instance, source, &agent_name(source))
    }

    /// [`Connection::connect`] as the agent `agent`. An instance that does not allow it is
    /// an error of kind `PermissionDenied`.
    pub fn connect_as(
        instance: &InstanceFile,
        source: Source,
        agent: &str,
    ) -> std::io::Result<Connection> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, instance.port));
        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        stream.set_read_timeout(Some(AUTH_TIMEOUT))?;
        let mut connection = Connection {
            reader: BufReader::new(stream.try_clone()?),
            stream,
            next_id: 0,
            project: None,
        };
        let params = json!({ "token": instance.token, "source": source, "agent": agent });
        let reply = connection
            .request(protocol::AUTH, params, &Signals::default())
            .map_err(|e| {
                let kind = match e.code {
                    ErrorCode::Rejected => ErrorKind::PermissionDenied,
                    _ => ErrorKind::ConnectionRefused,
                };
                std::io::Error::new(kind, e.message)
            })?;
        connection.project = serde_json::from_value(reply["project"].clone())
            .ok()
            .flatten();
        Ok(connection)
    }

    /// The Project file the instance had open when this connected.
    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    /// The instance's catalogue, `{commands: [...]}`.
    pub fn list(&mut self) -> Result<Value, CommandError> {
        self.request(protocol::LIST, json!({}), &Signals::default())
    }

    /// One catalogue entry.
    pub fn describe(&mut self, id: &str) -> Result<Value, CommandError> {
        self.request(protocol::DESCRIBE, json!({ "id": id }), &Signals::default())
    }

    /// Runs a Command on the instance's writer.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        self.run_with(id, params, &Signals::default())
    }

    /// [`Connection::run`], relaying the Command's progress to `signals` and cancelling it
    /// on the instance when `signals` is cancelled. A run cancelled before it starts is
    /// not sent.
    pub fn run_with(
        &mut self,
        id: &str,
        params: Value,
        signals: &Signals,
    ) -> Result<Value, CommandError> {
        if signals.cancel.is_cancelled() {
            return Err(CommandError::cancelled());
        }
        let params = json!({ "id": id, "params": params, "progress": true });
        self.request(protocol::RUN, params, signals)
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        signals: &Signals,
    ) -> Result<Value, CommandError> {
        self.next_id += 1;
        let id = self.next_id;
        let lost = |e: std::io::Error| {
            CommandError::internal(format!("lost the connection to the REDA app: {e}"))
        };
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.send(&request).map_err(lost)?;
        let previous = self.stream.read_timeout().map_err(lost)?;
        self.stream
            .set_read_timeout(Some(CANCEL_POLL))
            .map_err(lost)?;
        let mut waited = Duration::ZERO;
        let mut cancel_sent = false;
        let mut line = Vec::new();
        let response = loop {
            match self.reader.read_until(b'\n', &mut line) {
                Ok(0) => return Err(lost(ErrorKind::UnexpectedEof.into())),
                Ok(_) if line.ends_with(b"\n") => {
                    let message: Value = serde_json::from_slice(&line).map_err(|e| {
                        CommandError::internal(format!("the REDA app sent a malformed reply: {e}"))
                    })?;
                    line.clear();
                    if message["method"] == protocol::PROGRESS && message["params"]["id"] == id {
                        let p = &message["params"];
                        signals.report(
                            p["progress"].as_f64().unwrap_or_default(),
                            p["total"].as_f64(),
                            p["message"].as_str().unwrap_or_default(),
                        );
                    } else if message.get("method").is_none() {
                        break message;
                    }
                }
                Ok(_) => {}
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    waited += CANCEL_POLL;
                    if previous.is_some_and(|limit| waited >= limit) {
                        return Err(lost(e));
                    }
                    if !cancel_sent && signals.cancel.is_cancelled() {
                        let cancel = json!({
                            "jsonrpc": "2.0", "method": protocol::CANCEL, "params": { "id": id }
                        });
                        self.send(&cancel).map_err(lost)?;
                        cancel_sent = true;
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(lost(e)),
            }
        };
        self.stream.set_read_timeout(None).map_err(lost)?;
        answer(id, response)
    }

    fn send(&mut self, message: &Value) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(message).expect("JSON values serialise");
        line.push(b'\n');
        self.stream.write_all(&line)?;
        self.stream.flush()
    }
}

/// The result of response `response` to request `id`, or its error.
fn answer(id: u64, mut response: Value) -> Result<Value, CommandError> {
    if response["id"] != id {
        return Err(CommandError::internal(
            "the REDA app answered another request",
        ));
    }
    if let Some(result) = response.get_mut("result") {
        return Ok(result.take());
    }
    let error = &response["error"];
    if error["code"] == protocol::DENIED {
        let message = error["message"]
            .as_str()
            .unwrap_or("the agent was not allowed");
        return Err(CommandError::rejected(message));
    }
    if error["code"] == protocol::COMMAND_FAILED
        && let Ok(err) = serde_json::from_value::<CommandError>(error["data"].clone())
    {
        return Err(err);
    }
    Err(CommandError::internal(format!(
        "the REDA app refused the request: {}",
        error["message"].as_str().unwrap_or("no message")
    )))
}
