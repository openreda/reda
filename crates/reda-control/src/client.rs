//! The client side: find the running instances and talk to one.
//!
//! [`bridge`] picks the instance the CLI and MCP server must bridge to (ADR-0007): the one
//! with the target Project open, or, with no Project named, the only instance that has a
//! Project open. Otherwise they run in-process.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use reda_engine::command::{CommandError, UserDirs};
use reda_engine::journal::Source;
use reda_engine::store;
use serde_json::{Value, json};

use crate::normalize;
use crate::protocol::{self, InstanceFile};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Connects to the instance to bridge to, if there is one: with `project` (a `.reda` file
/// or its folder), the live instance that has it open; without, the only live instance
/// with any Project open.
pub fn bridge(dirs: &UserDirs, project: Option<&Path>, source: Source) -> Option<Connection> {
    let all = instances(dirs);
    match project {
        Some(project) => {
            let wanted = normalize(&store::resolve_project_file(project));
            all.iter()
                .filter(|i| i.project.as_deref() == Some(wanted.as_path()))
                .find_map(|i| Connection::connect(i, source).ok())
        }
        None => {
            let mut live: Vec<Connection> = all
                .iter()
                .filter(|i| i.project.is_some())
                .filter_map(|i| Connection::connect(i, source).ok())
                .collect();
            (live.len() == 1).then(|| live.remove(0))
        }
    }
}

/// An authenticated connection to one instance. Requests go one at a time.
pub struct Connection {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    next_id: u64,
    project: Option<PathBuf>,
}

impl Connection {
    /// Connects and presents the instance's token, naming `source` for its Journal.
    pub fn connect(instance: &InstanceFile, source: Source) -> std::io::Result<Connection> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, instance.port));
        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        stream.set_read_timeout(Some(AUTH_TIMEOUT))?;
        let mut connection = Connection {
            reader: BufReader::new(stream.try_clone()?),
            stream,
            next_id: 0,
            project: None,
        };
        let reply = connection
            .request(
                protocol::AUTH,
                json!({ "token": instance.token, "source": source }),
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e.message))?;
        connection.project = serde_json::from_value(reply["project"].clone())
            .ok()
            .flatten();
        connection.stream.set_read_timeout(None)?;
        Ok(connection)
    }

    /// The Project file the instance had open when this connected.
    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    /// The instance's catalogue, `{commands: [...]}`.
    pub fn list(&mut self) -> Result<Value, CommandError> {
        self.request(protocol::LIST, json!({}))
    }

    /// One catalogue entry.
    pub fn describe(&mut self, id: &str) -> Result<Value, CommandError> {
        self.request(protocol::DESCRIBE, json!({ "id": id }))
    }

    /// Runs a Command on the instance's writer.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        self.request(protocol::RUN, json!({ "id": id, "params": params }))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, CommandError> {
        self.next_id += 1;
        let id = self.next_id;
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let lost = |e: std::io::Error| {
            CommandError::internal(format!("lost the connection to the REDA app: {e}"))
        };
        let mut line = serde_json::to_vec(&request).expect("JSON values serialise");
        line.push(b'\n');
        self.stream.write_all(&line).map_err(lost)?;
        self.stream.flush().map_err(lost)?;
        let mut line = String::new();
        if self.reader.read_line(&mut line).map_err(lost)? == 0 {
            return Err(lost(std::io::ErrorKind::UnexpectedEof.into()));
        }
        let mut response: Value = serde_json::from_str(&line).map_err(|e| {
            CommandError::internal(format!("the REDA app sent a malformed reply: {e}"))
        })?;
        if response["id"] != id {
            return Err(CommandError::internal(
                "the REDA app answered another request",
            ));
        }
        if let Some(result) = response.get_mut("result") {
            return Ok(result.take());
        }
        let error = &response["error"];
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
}
