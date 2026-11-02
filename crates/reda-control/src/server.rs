//! The server side: listens, authenticates (asking the host to admit each agent when it
//! gates them), answers `commands.list`/`describe` from the catalogue and hands each
//! `commands.run` to the host as a [`Call`].
//!
//! One thread accepts; one thread per connection reads its lines ([`connection`]). Every
//! connection's Calls go to one channel, which the host drains on its single writer, so
//! Calls run in the order they arrive.

mod connection;
mod instance;

use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use reda_engine::command::{CommandError, Signals, UserDirs};
use reda_engine::journal::Source;
use serde_json::Value;

use crate::protocol::InstanceFile;

type Outcome = Result<Value, CommandError>;

/// One `commands.run` from a client, for the host to run on its writer and answer.
pub struct Call {
    pub id: String,
    pub params: Value,
    /// Who sent it (the client's `auth {source}`), for the Journal and undo labels.
    pub source: Source,
    /// The client's cancellation and progress: a `cancel` notification cancels the token,
    /// and progress reports go back to the client as `progress` notifications.
    pub signals: Signals,
    reply: Sender<Outcome>,
}

impl Call {
    /// Sends the outcome back to the client. A client that has gone away is ignored.
    pub fn reply(self, outcome: Outcome) {
        let _ = self.reply.send(outcome);
    }

    /// Runs the Call with `run` (given its id, params and signals) and replies with the
    /// outcome, unless the client cancelled it while it was queued, which replies
    /// `cancelled` without running it. Returns the outcome it sent.
    pub fn run(self, run: impl FnOnce(&str, Value, &Signals) -> Outcome) -> Outcome {
        let outcome = if self.signals.cancel.is_cancelled() {
            Err(CommandError::cancelled())
        } else {
            run(&self.id, self.params.clone(), &self.signals)
        };
        let _ = self.reply.send(outcome.clone());
        outcome
    }
}

/// A connected client: who it says it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    /// The name it gave in `auth {agent}`, or a name for its source when it gave none.
    pub name: String,
    pub source: Source,
}

/// An authenticated client waiting for the host to let it in: the app asks "Allow this
/// agent?". Dropping it denies.
pub struct Admission {
    pub agent: Agent,
    reply: Sender<bool>,
}

impl Admission {
    pub fn allow(self) {
        let _ = self.reply.send(true);
    }

    pub fn deny(self) {
        let _ = self.reply.send(false);
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
    /// Where admissions go when the host gates agents.
    gate: Option<Mutex<Sender<Admission>>>,
    /// The admitted agents still connected, by connection number.
    agents: Mutex<Vec<(u64, Agent)>>,
    next_connection: AtomicU64,
}

impl ControlServer {
    /// Opens the port and writes the instance file. `catalogue` is the host registry's
    /// [`reda_engine::command::Registry::catalogue`]; `project` the open Project file
    /// ([`crate::project_file`]). Calls arrive on the returned receiver; it ends once the
    /// server has stopped and every connection is gone. Every agent with the token is
    /// admitted.
    pub fn start(
        dirs: &UserDirs,
        catalogue: &Value,
        project: Option<PathBuf>,
    ) -> std::io::Result<(ControlServer, Receiver<Call>)> {
        start(dirs, catalogue, project, None)
    }

    /// [`ControlServer::start`], but each agent that presents the token waits on the
    /// returned [`Admission`] receiver for the host to allow or deny it. The app's own
    /// file-association hand-off (`source: gui`) is admitted without asking.
    pub fn start_gated(
        dirs: &UserDirs,
        catalogue: &Value,
        project: Option<PathBuf>,
    ) -> std::io::Result<(ControlServer, Receiver<Call>, Receiver<Admission>)> {
        let (gate, admissions) = mpsc::channel();
        let (server, calls) = start(dirs, catalogue, project, Some(gate))?;
        Ok((server, calls, admissions))
    }

    pub fn port(&self) -> u16 {
        self.inner.port
    }

    /// The instance file this server wrote.
    pub fn instance_file(&self) -> &Path {
        &self.inner.file
    }

    /// The admitted agents connected now, in the order they connected.
    pub fn agents(&self) -> Vec<Agent> {
        let agents = self.inner.shared.agents.lock().expect("not poisoned");
        agents.iter().map(|(_, a)| a.clone()).collect()
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

fn start(
    dirs: &UserDirs,
    catalogue: &Value,
    project: Option<PathBuf>,
    gate: Option<Sender<Admission>>,
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
        gate: gate.map(Mutex::new),
        agents: Mutex::new(Vec::new()),
        next_connection: AtomicU64::new(0),
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
                connection::serve(&shared, &stream, &calls);
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
