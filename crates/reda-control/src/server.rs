//! The server side: listens, authenticates, answers `commands.list`/`describe` from the
//! catalogue and hands each `commands.run` to the host as a [`Call`].
//!
//! One thread accepts; one thread per connection reads its lines and waits for each reply,
//! so a connection's requests run in order. Every connection's Calls go to one channel,
//! which the host drains on its single writer.

mod instance;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use reda_engine::command::{CommandError, UserDirs};
use reda_engine::journal::Source;
use serde_json::{Value, json};

use crate::protocol::{self, InstanceFile};

/// The longest the first message may take to arrive, and the most bytes it may have.
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_LIMIT: u64 = 64 * 1024;

/// Commands a bridged session may not run: saving belongs to the host, and opening or
/// creating a Project would replace the one the host holds.
const REFUSED: &[&str] = &["project.save", "project.new", "project.open"];

type Outcome = Result<Value, CommandError>;

/// One `commands.run` from a client, for the host to run on its writer and answer.
pub struct Call {
    pub id: String,
    pub params: Value,
    /// Who sent it (the client's `auth {source}`), for the Journal.
    pub source: Source,
    reply: Sender<Outcome>,
}

impl Call {
    /// Sends the outcome back to the client. A client that has gone away is ignored.
    pub fn reply(self, outcome: Outcome) {
        let _ = self.reply.send(outcome);
    }
}

/// A running control port. Clones share it; [`ControlServer::stop`] (or dropping the last
/// clone) closes the port, drops every connection and removes the instance file.
#[derive(Clone)]
pub struct ControlServer {
    inner: Arc<Inner>,
}

struct Inner {
    port: u16,
    file: PathBuf,
    instance: Mutex<InstanceFile>,
    shared: Arc<Shared>,
    accept: Mutex<Option<JoinHandle<()>>>,
}

/// What the accept and connection threads share.
struct Shared {
    token: String,
    commands: Vec<Value>,
    project: Mutex<Option<PathBuf>>,
    stopped: AtomicBool,
    connections: Mutex<Vec<(TcpStream, JoinHandle<()>)>>,
}

impl ControlServer {
    /// Opens the port and writes the instance file. `catalogue` is the host registry's
    /// [`reda_engine::command::Registry::catalogue`]; `project` the open Project file
    /// ([`crate::project_file`]). Calls arrive on the returned receiver; it ends once the
    /// server has stopped and every connection is gone.
    pub fn start(
        dirs: &UserDirs,
        catalogue: &Value,
        project: Option<PathBuf>,
    ) -> std::io::Result<(ControlServer, Receiver<Call>)> {
        let dir = reda_engine::user_dirs::create_instances_dir(dirs)?;
        instance::remove_stale(&dir);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let token = instance::token()?;
        let record = InstanceFile {
            pid: std::process::id(),
            port,
            token: token.clone(),
            project: project.clone(),
        };
        let file = dir.join(instance::file_name());
        instance::write(&file, &record)?;

        let shared = Arc::new(Shared {
            token,
            commands: catalogue["commands"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            project: Mutex::new(project),
            stopped: AtomicBool::new(false),
            connections: Mutex::new(Vec::new()),
        });
        let (calls, receiver) = mpsc::channel();
        let accept = {
            let shared = shared.clone();
            std::thread::spawn(move || accept_loop(&listener, &shared, &calls))
        };
        let server = ControlServer {
            inner: Arc::new(Inner {
                port,
                file,
                instance: Mutex::new(record),
                shared,
                accept: Mutex::new(Some(accept)),
            }),
        };
        Ok((server, receiver))
    }

    pub fn port(&self) -> u16 {
        self.inner.port
    }

    /// The instance file this server wrote.
    pub fn instance_file(&self) -> &Path {
        &self.inner.file
    }

    /// Records a newly opened, created or closed Project in the instance file, so clients
    /// bridge by it. Does nothing when it is unchanged.
    pub fn set_project(&self, project: Option<PathBuf>) -> std::io::Result<()> {
        let mut record = self.inner.instance.lock().expect("not poisoned");
        if record.project == project {
            return Ok(());
        }
        record.project = project.clone();
        *self.inner.shared.project.lock().expect("not poisoned") = project;
        if self.inner.shared.stopped.load(Ordering::SeqCst) {
            return Ok(());
        }
        instance::write(&self.inner.file, &record)
    }

    /// Closes the port and every connection, waits for their threads, and removes the
    /// instance file. A connection waiting for a Call's reply ends once the host replies
    /// or drops the Call. Calling it again does nothing.
    pub fn stop(&self) {
        self.inner.stop();
    }
}

impl Inner {
    fn stop(&self) {
        if self.shared.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = std::fs::remove_file(&self.file);
        // Wake the accept loop, which sees the flag and returns.
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port));
        if let Some(accept) = self.accept.lock().expect("not poisoned").take() {
            let _ = accept.join();
        }
        let connections =
            std::mem::take(&mut *self.shared.connections.lock().expect("not poisoned"));
        for (stream, _) in &connections {
            let _ = stream.shutdown(Shutdown::Both);
        }
        for (_, thread) in connections {
            let _ = thread.join();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>, calls: &Sender<Call>) {
    for stream in listener.incoming() {
        if shared.stopped.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        let Ok(handle) = stream.try_clone() else {
            continue;
        };
        let thread = {
            let shared = shared.clone();
            let calls = calls.clone();
            std::thread::spawn(move || {
                serve(&shared, &stream, &calls);
                let _ = stream.shutdown(Shutdown::Both);
            })
        };
        shared
            .connections
            .lock()
            .expect("not poisoned")
            .push((handle, thread));
    }
}

/// Serves one connection until it closes, or its first message fails auth.
fn serve(shared: &Shared, stream: &TcpStream, calls: &Sender<Call>) {
    let Ok(read) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read);
    let _ = stream.set_read_timeout(Some(AUTH_TIMEOUT));
    let mut line = String::new();
    if (&mut reader).take(AUTH_LIMIT).read_line(&mut line).is_err() {
        return;
    }
    let Some(source) = authenticate(shared, stream, &line) else {
        return;
    };
    let _ = stream.set_read_timeout(None);
    for line in reader.lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle(shared, calls, source, &line)
            && send(stream, &response).is_err()
        {
            return;
        }
    }
}

/// Checks the first message. Answers it either way; `None` means close the connection.
fn authenticate(shared: &Shared, stream: &TcpStream, line: &str) -> Option<Source> {
    let request: Value = serde_json::from_str(line).unwrap_or(Value::Null);
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let params = &request["params"];
    let presented = params["token"].as_str().unwrap_or_default();
    let source = match params.get("source") {
        None | Some(Value::Null) => Some(Source::Cli),
        Some(source) => serde_json::from_value(source.clone()).ok(),
    };
    let authorised = request["method"] == protocol::AUTH
        && instance::same_token(presented, &shared.token)
        && source.is_some();
    if !authorised {
        let _ = send(
            stream,
            &error(
                id,
                protocol::UNAUTHORIZED,
                "the first message must be auth {token} with this instance's token",
                None,
            ),
        );
        return None;
    }
    send(stream, &success(id, project_result(shared))).ok()?;
    source
}

fn project_result(shared: &Shared) -> Value {
    json!({ "project": *shared.project.lock().expect("not poisoned") })
}

/// Why a request failed: the request itself, or the Command it ran.
enum Failure {
    Protocol(i64, String),
    Command(CommandError),
}

impl From<CommandError> for Failure {
    fn from(err: CommandError) -> Failure {
        Failure::Command(err)
    }
}

/// Answers one request line; `None` for a notification.
fn handle(shared: &Shared, calls: &Sender<Call>, source: Source, line: &str) -> Option<Value> {
    let Ok(request) = serde_json::from_str::<Value>(line) else {
        return Some(error(Value::Null, protocol::PARSE_ERROR, "not JSON", None));
    };
    let id = request.get("id").cloned();
    let method = request["method"].as_str();
    let (Some(method), true) = (method, request["jsonrpc"] == "2.0") else {
        let id = id.unwrap_or(Value::Null);
        return Some(error(
            id,
            protocol::INVALID_REQUEST,
            "not a JSON-RPC 2.0 request",
            None,
        ));
    };
    let id = id?;
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let outcome = match method {
        protocol::AUTH => Ok(project_result(shared)),
        protocol::LIST => Ok(json!({ "commands": shared.commands })),
        protocol::DESCRIBE => {
            command_id(&params, &["id"]).and_then(|command| describe(shared, &command))
        }
        protocol::RUN => {
            run_params(params).and_then(|(command, params)| run(calls, source, command, params))
        }
        _ => Err(Failure::Protocol(
            protocol::METHOD_NOT_FOUND,
            format!("no method is named {method}"),
        )),
    };
    Some(match outcome {
        Ok(result) => success(id, result),
        Err(Failure::Protocol(code, message)) => error(id, code, &message, None),
        Err(Failure::Command(err)) => {
            let data = serde_json::to_value(&err).expect("errors serialise");
            error(id, protocol::COMMAND_FAILED, &err.message, Some(data))
        }
    })
}

fn describe(shared: &Shared, id: &str) -> Result<Value, Failure> {
    let base = id.split_once(':').map_or(id, |(base, _)| base);
    match shared.commands.iter().find(|c| c["id"] == base) {
        Some(command) => Ok(command.clone()),
        None => Err(CommandError::not_found(format!("no Command has the id {id}")).into()),
    }
}

fn run(calls: &Sender<Call>, source: Source, id: String, params: Value) -> Result<Value, Failure> {
    let base = id.split_once(':').map_or(id.as_str(), |(base, _)| base);
    if REFUSED.contains(&base) {
        return Err(CommandError::rejected(format!(
            "{base} is not available over the bridge: the REDA app holds the Project, and \
             a bridged session never saves. Its edits show as unsaved changes in the app"
        ))
        .into());
    }
    let (reply, answer) = mpsc::channel();
    let call = Call {
        id,
        params,
        source,
        reply,
    };
    let stopped = || CommandError::internal("the REDA app has stopped serving Commands");
    if calls.send(call).is_err() {
        return Err(stopped().into());
    }
    Ok(answer.recv().unwrap_or_else(|_| Err(stopped()))?)
}

/// `{id}` with no other keys.
fn command_id(params: &Value, keys: &[&str]) -> Result<String, Failure> {
    let invalid = |message: String| Failure::Protocol(protocol::INVALID_PARAMS, message);
    let Value::Object(map) = params else {
        return Err(invalid("params must be an object".into()));
    };
    if let Some(key) = map.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(invalid(format!("unknown field {key}")));
    }
    match map.get("id") {
        Some(Value::String(id)) => Ok(id.clone()),
        _ => Err(invalid("id must be a string".into())),
    }
}

/// `{id, params?}`.
fn run_params(params: Value) -> Result<(String, Value), Failure> {
    let id = command_id(&params, &["id", "params"])?;
    let params = params.get("params").cloned().unwrap_or(Value::Null);
    Ok((id, params))
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

fn send(mut stream: &TcpStream, message: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message).expect("JSON values serialise");
    line.push(b'\n');
    stream.write_all(&line)?;
    stream.flush()
}
