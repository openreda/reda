//! One connection: auth (and admission), then one request per line until it closes.
//!
//! `commands.run` does not hold up the connection's reader: the Call goes to the host and
//! a waiter thread writes its response, so a `cancel` notification sent meanwhile reaches
//! the running Command. Writes share one lock, so lines never interleave.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use reda_engine::command::{CancelToken, CommandError, Progress, Signals};
use reda_engine::journal::Source;
use serde_json::{Value, json};

use super::{Admission, Agent, Call, Shared, instance};
use crate::protocol;

/// The longest the first message may take to arrive, and the most bytes it may have.
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_LIMIT: u64 = 64 * 1024;
/// How often a connection waiting for its next request checks whether the server stopped.
const STOP_POLL: Duration = Duration::from_millis(100);

/// Commands a bridged session may not run: saving belongs to the host, and opening or
/// creating a Project would replace the one the host holds.
const REFUSED: &[&str] = &["project.save", "project.new", "project.open"];

/// The connection's writing half, shared by the reader and the waiters.
type Writer = Arc<Mutex<TcpStream>>;

/// The runs still in flight, by request id (as JSON text), to cancel.
type InFlight = Arc<Mutex<HashMap<String, CancelToken>>>;

/// Serves one connection until it closes, or its first message fails auth or admission.
pub(super) fn serve(shared: &Shared, stream: &TcpStream, calls: &Sender<Call>) {
    let (Ok(read), Ok(write)) = (stream.try_clone(), stream.try_clone()) else {
        return;
    };
    let writer: Writer = Arc::new(Mutex::new(write));
    let mut reader = BufReader::new(read);
    // On the handle read from: on Windows a `try_clone`d socket does not share the
    // original's read timeout.
    let _ = reader.get_ref().set_read_timeout(Some(AUTH_TIMEOUT));
    let mut line = String::new();
    if (&mut reader).take(AUTH_LIMIT).read_line(&mut line).is_err() {
        return;
    }
    let Some((agent, auth_id)) = authenticate(shared, &writer, &line) else {
        return;
    };
    // From here the reader wakes now and then to see whether the server stopped: shutting
    // the socket down from another handle does not wake a blocked read on Windows.
    if reader.get_ref().set_read_timeout(Some(STOP_POLL)).is_err() {
        return;
    }
    // Listed before the client hears it is in, so the host's indicator never lags.
    let number = shared.next_connection.fetch_add(1, Ordering::SeqCst);
    shared
        .agents
        .lock()
        .expect("not poisoned")
        .push((number, agent.clone()));
    let forget = || {
        let mut agents = shared.agents.lock().expect("not poisoned");
        agents.retain(|(n, _)| *n != number);
    };
    if send(&writer, &success(auth_id, project_result(shared))).is_err() {
        forget();
        return;
    }

    let in_flight = InFlight::default();
    let mut waiters: Vec<JoinHandle<()>> = Vec::new();
    while let Some(line) = next_line(&mut reader, shared) {
        if line.trim().is_empty() {
            continue;
        }
        let request = Request {
            shared,
            calls,
            agent: &agent,
            writer: &writer,
            in_flight: &in_flight,
        };
        match request.handle(&line) {
            Handled::Respond(response) => {
                if send(&writer, &response).is_err() {
                    break;
                }
            }
            Handled::Waiting(waiter) => waiters.push(waiter),
            Handled::Nothing => {}
        }
        waiters.retain(|w| !w.is_finished());
    }
    // Cancel what is still queued or running: nobody is left to read the answers.
    for token in in_flight.lock().expect("not poisoned").values() {
        token.cancel();
    }
    for waiter in waiters {
        let _ = waiter.join();
    }
    forget();
}

/// The next line from the client, without its line ending, or `None` once it closes, sends
/// something that is not UTF-8, fails, or the server stops. A line split across read
/// timeouts is kept whole.
fn next_line(reader: &mut BufReader<TcpStream>, shared: &Shared) -> Option<String> {
    let mut bytes = Vec::new();
    loop {
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) if bytes.is_empty() => return None,
            // A full line, or the last one before the client closed.
            Ok(_) => break,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if shared.stopped.load(Ordering::SeqCst) {
                    return None;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    let mut line = String::from_utf8(bytes).ok()?;
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    Some(line)
}

/// Checks the first message and, when the host gates agents, waits for it to admit this
/// one. Answers a refusal; `None` means close the connection. Otherwise returns the agent
/// and the request id to answer.
fn authenticate(shared: &Shared, writer: &Writer, line: &str) -> Option<(Agent, Value)> {
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
    let (true, Some(source)) = (authorised, source) else {
        let _ = send(
            writer,
            &error(
                id,
                protocol::UNAUTHORIZED,
                "the first message must be auth {token} with this instance's token",
                None,
            ),
        );
        return None;
    };
    let name = match params["agent"].as_str().map(str::trim) {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => protocol::default_agent_name(source).to_owned(),
    };
    let agent = Agent { name, source };
    if !admitted(shared, &agent) {
        let _ = send(
            writer,
            &error(
                id,
                protocol::DENIED,
                &format!("the REDA app did not allow the agent {}", agent.name),
                None,
            ),
        );
        return None;
    }
    Some((agent, id))
}

/// Asks the host to admit `agent`, when it gates agents. The app's own hand-off is let in.
fn admitted(shared: &Shared, agent: &Agent) -> bool {
    let Some(gate) = &shared.gate else {
        return true;
    };
    if agent.source == Source::Gui {
        return true;
    }
    let (reply, answer) = mpsc::channel();
    let admission = Admission {
        agent: agent.clone(),
        reply,
    };
    if gate.lock().expect("not poisoned").send(admission).is_err() {
        return false;
    }
    answer.recv().unwrap_or(false)
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

enum Handled {
    Respond(Value),
    /// A run went to the host; the thread writes its response.
    Waiting(JoinHandle<()>),
    /// A notification.
    Nothing,
}

/// One request line and what answering it needs.
struct Request<'a> {
    shared: &'a Shared,
    calls: &'a Sender<Call>,
    agent: &'a Agent,
    writer: &'a Writer,
    in_flight: &'a InFlight,
}

impl Request<'_> {
    fn handle(&self, line: &str) -> Handled {
        let Ok(request) = serde_json::from_str::<Value>(line) else {
            return Handled::Respond(error(Value::Null, protocol::PARSE_ERROR, "not JSON", None));
        };
        let id = request.get("id").cloned();
        let method = request["method"].as_str();
        let (Some(method), true) = (method, request["jsonrpc"] == "2.0") else {
            let id = id.unwrap_or(Value::Null);
            return Handled::Respond(error(
                id,
                protocol::INVALID_REQUEST,
                "not a JSON-RPC 2.0 request",
                None,
            ));
        };
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = id else {
            if method == protocol::CANCEL {
                self.cancel(&params["id"]);
            }
            return Handled::Nothing;
        };
        let outcome = match method {
            protocol::AUTH => Ok(project_result(self.shared)),
            protocol::LIST => Ok(json!({ "commands": self.shared.commands })),
            protocol::DESCRIBE => {
                command_id(&params, &["id"]).and_then(|command| self.describe(&command))
            }
            protocol::RUN => match run_params(params).and_then(|run| self.check(run)) {
                Ok(run) => return Handled::Waiting(self.run(id, run)),
                Err(failure) => Err(failure),
            },
            _ => Err(Failure::Protocol(
                protocol::METHOD_NOT_FOUND,
                format!("no method is named {method}"),
            )),
        };
        Handled::Respond(respond(id, outcome))
    }

    fn describe(&self, id: &str) -> Result<Value, Failure> {
        let base = id.split_once(':').map_or(id, |(base, _)| base);
        match self.shared.commands.iter().find(|c| c["id"] == base) {
            Some(command) => Ok(command.clone()),
            None => Err(CommandError::not_found(format!("no Command has the id {id}")).into()),
        }
    }

    /// Refuses the runs a bridged session may not make.
    fn check(&self, run: Run) -> Result<Run, Failure> {
        let base = run.id.split_once(':').map_or(run.id.as_str(), |(b, _)| b);
        if self.agent.source == Source::Gui && base != protocol::OPEN_FILE {
            return Err(CommandError::rejected(format!(
                "another REDA app may only run {} over the bridge",
                protocol::OPEN_FILE
            ))
            .into());
        }
        if REFUSED.contains(&base) {
            return Err(CommandError::rejected(format!(
                "{base} is not available over the bridge: the REDA app holds the Project, \
                 and a bridged session never saves. Its edits show as unsaved changes in \
                 the app"
            ))
            .into());
        }
        Ok(run)
    }

    /// Queues the run for the host and starts the thread that answers it.
    fn run(&self, id: Value, run: Run) -> JoinHandle<()> {
        let key = id.to_string();
        let cancel = CancelToken::new();
        let mut signals = Signals::new(cancel.clone());
        if run.progress {
            signals = signals.with_progress(Arc::new(Notify {
                id: id.clone(),
                writer: self.writer.clone(),
            }));
        }
        self.in_flight
            .lock()
            .expect("not poisoned")
            .insert(key.clone(), cancel);
        let (reply, answer) = mpsc::channel();
        let call = Call {
            id: run.id,
            params: run.params,
            source: self.agent.source,
            signals,
            reply,
        };
        let stopped = || CommandError::internal("the REDA app has stopped serving Commands");
        let sent = self.calls.send(call).is_ok();
        let writer = self.writer.clone();
        let in_flight = self.in_flight.clone();
        std::thread::spawn(move || {
            let outcome = match sent {
                true => answer.recv().unwrap_or_else(|_| Err(stopped())),
                false => Err(stopped()),
            };
            in_flight.lock().expect("not poisoned").remove(&key);
            let _ = send(&writer, &respond(id, outcome.map_err(Failure::Command)));
        })
    }

    fn cancel(&self, id: &Value) {
        if let Some(token) = self
            .in_flight
            .lock()
            .expect("not poisoned")
            .get(&id.to_string())
        {
            token.cancel();
        }
    }
}

/// Sends a running Command's progress to the client as `progress` notifications.
struct Notify {
    id: Value,
    writer: Writer,
}

impl Progress for Notify {
    fn report(&self, progress: f64, total: Option<f64>, message: &str) {
        let mut params = json!({ "id": self.id, "progress": progress, "message": message });
        if let Some(total) = total {
            params["total"] = json!(total);
        }
        let notification =
            json!({ "jsonrpc": "2.0", "method": protocol::PROGRESS, "params": params });
        let _ = send(&self.writer, &notification);
    }
}

/// The params of `commands.run`.
struct Run {
    id: String,
    params: Value,
    progress: bool,
}

/// `{id}` with no other keys than `keys`.
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

/// `{id, params?, progress?}`.
fn run_params(params: Value) -> Result<Run, Failure> {
    let id = command_id(&params, &["id", "params", "progress"])?;
    let progress = match params.get("progress") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(progress)) => *progress,
        Some(_) => {
            return Err(Failure::Protocol(
                protocol::INVALID_PARAMS,
                "progress must be a boolean".into(),
            ));
        }
    };
    let params = params.get("params").cloned().unwrap_or(Value::Null);
    Ok(Run {
        id,
        params,
        progress,
    })
}

fn respond(id: Value, outcome: Result<Value, Failure>) -> Value {
    match outcome {
        Ok(result) => success(id, result),
        Err(Failure::Protocol(code, message)) => error(id, code, &message, None),
        Err(Failure::Command(err)) => {
            let data = serde_json::to_value(&err).expect("errors serialise");
            error(id, protocol::COMMAND_FAILED, &err.message, Some(data))
        }
    }
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

fn send(writer: &Writer, message: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message).expect("JSON values serialise");
    line.push(b'\n');
    let mut stream = writer.lock().expect("not poisoned");
    stream.write_all(&line)?;
    stream.flush()
}
