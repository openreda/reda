//! `reda mcp [--project p]`: an MCP server over stdio, in-process.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use reda_testkit::{TempDir, read_tree};
use serde_json::{Value, json};

/// A `reda mcp` child process spoken to as an MCP client, one request at a time.
struct Client {
    child: Child,
    /// The child's own `REDA_HOME`, so it never bridges to a running REDA.
    _home: TempDir,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: u64,
}

impl Client {
    fn spawn(args: &[&str]) -> Client {
        let home = TempDir::new("mcp-home");
        let mut child = Command::new(env!("CARGO_BIN_EXE_reda"))
            .arg("mcp")
            .args(args)
            .env("REDA_HOME", home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run reda mcp");
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        let mut client = Client {
            child,
            _home: home,
            stdin,
            lines,
            next_id: 0,
        };
        let init = client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "reda", "{init}");
        client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        client
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    /// Sends a request and returns its response, skipping notifications.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(30))
                .expect("a response from reda mcp");
            let message: Value = serde_json::from_str(&line).expect("stdout carries JSON-RPC");
            if message["id"] == id {
                return message;
            }
        }
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        response["result"].clone()
    }

    /// Closes stdin, waits for the server to exit, and returns its stderr.
    fn finish(mut self) -> String {
        drop(self.stdin.take());
        let out = self.child.wait_with_output().expect("reda mcp exits");
        assert!(out.status.success(), "reda mcp failed: {out:?}");
        String::from_utf8(out.stderr).unwrap()
    }
}

fn new_project(dir: &Path) -> Value {
    json!({"id": "project.new", "params": {"path": dir, "name": "Demo"}})
}

#[test]
fn lists_the_generic_tools_and_the_query_commands() {
    let mut client = Client::spawn(&[]);
    let tools = client.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap().clone();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "search_commands",
            "describe_command",
            "run_command",
            "project.overview",
            "objects.query",
            "object.get",
            "document.render",
        ]
    );
    let overview = &tools[3];
    let catalogue = reda_engine::registry()
        .describe("project.overview")
        .unwrap();
    assert_eq!(overview["inputSchema"], catalogue["params"]);
    assert_eq!(overview["outputSchema"], catalogue["result"]);
    assert_eq!(overview["annotations"]["readOnlyHint"], true);
    for tool in &tools[..2] {
        assert_eq!(tool["outputSchema"]["type"], "object", "{tool}");
    }
    client.finish();
}

#[test]
fn search_finds_commands_by_words_in_their_id_title_or_description() {
    let mut client = Client::spawn(&[]);
    let found = client.call("search_commands", json!({"query": "save project"}));
    let ids: Vec<&str> = found["structuredContent"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.first(), Some(&"project.save"), "{found}");
    assert!(!ids.contains(&"edit.undo"));

    let none = client.call("search_commands", json!({"query": "frobnicate"}));
    assert_eq!(none["structuredContent"]["commands"], json!([]));

    let edits = client.call("search_commands", json!({"kind": "edit"}));
    let kinds: Vec<&Value> = edits["structuredContent"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["kind"])
        .collect();
    assert!(!kinds.is_empty());
    assert!(kinds.iter().all(|k| *k == "edit"), "{edits}");
    client.finish();
}

#[test]
fn describe_returns_the_catalogue_entry_with_its_result_schema() {
    let mut client = Client::spawn(&[]);
    let described = client.call("describe_command", json!({"id": "project.save"}));
    assert_eq!(
        described["structuredContent"],
        reda_engine::registry().describe("project.save").unwrap()
    );
    let missing = client.call("describe_command", json!({"id": "no.such"}));
    assert_eq!(missing["isError"], true);
    assert_eq!(missing["structuredContent"]["code"], "not_found");
    client.finish();
}

#[test]
fn run_command_runs_on_one_session_and_query_tools_see_its_result() {
    let dir = TempDir::new("mcp-run");
    let mut client = Client::spawn(&[]);
    let created = client.call("run_command", new_project(dir.path()));
    assert_eq!(created["structuredContent"]["name"], "Demo", "{created}");
    // project.new writes the new Project; later edits stay in memory.
    let written = read_tree(dir.path());
    let added = client.call(
        "run_command",
        json!({"id": "document.add", "params": {"kind": "schematic"}}),
    );
    assert_eq!(added["isError"], false, "{added}");

    let overview = client.call("project.overview", json!({}));
    assert_eq!(overview["structuredContent"]["name"], "Demo", "{overview}");

    let bad = client.call(
        "run_command",
        json!({"id": "project.overview", "params": {"extra": 1}}),
    );
    assert_eq!(bad["isError"], true);
    assert_eq!(bad["structuredContent"]["code"], "invalid_params");
    assert_eq!(bad["structuredContent"]["details"]["pointer"], "/extra");

    let stderr = client.finish();
    assert_eq!(read_tree(dir.path()), written, "the edit is not saved");
    assert!(stderr.contains("discarding unsaved changes"), "{stderr}");
}

#[test]
fn saves_only_through_project_save() {
    let dir = TempDir::new("mcp-save");
    let mut client = Client::spawn(&[]);
    client.call("run_command", new_project(dir.path()));
    client.call(
        "run_command",
        json!({"id": "document.add", "params": {"kind": "schematic"}}),
    );
    let sheet_2 = dir.path().join("Demo").join("sheet-2.redasch");
    assert!(!sheet_2.exists());
    let saved = client.call("run_command", json!({"id": "project.save"}));
    assert_eq!(saved["isError"], false, "{saved}");
    let stderr = client.finish();
    assert!(sheet_2.is_file());
    assert!(!stderr.contains("discarding"), "{stderr}");
}

#[test]
fn project_flag_opens_the_project_first() {
    let dir = TempDir::new("mcp-project");
    let home = TempDir::new("mcp-home");
    let params = json!({"path": dir.path(), "name": "Demo"}).to_string();
    let out = Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(["run", "project.new", "--params", &params])
        .env("REDA_HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let project = dir.path().join("Demo").join("Demo.reda");

    let mut client = Client::spawn(&["--project", project.to_str().unwrap()]);
    let overview = client.call("project.overview", json!({}));
    assert_eq!(overview["structuredContent"]["name"], "Demo", "{overview}");
    assert!(client.finish().is_empty());
}

#[test]
fn a_project_that_does_not_open_is_an_error_before_serving() {
    let home = TempDir::new("mcp-home");
    let out = Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(["mcp", "--project", "/nonexistent/Demo.reda"])
        .env("REDA_HOME", home.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr is the error JSON");
    assert_eq!(err["code"], "not_found");
}

#[test]
fn document_render_returns_a_png_as_an_image_block() {
    let dir = TempDir::new("mcp-render");
    let mut client = Client::spawn(&[]);
    client.call("run_command", new_project(dir.path()));
    let overview = client.call("project.overview", json!({}));
    let sheet = overview["structuredContent"]["documents"][0]["uuid"].clone();
    let out = client.call("document.render", json!({"document": sheet, "size": 64}));
    if out["isError"] == true {
        // No GPU or software adapter here: the error names the cause.
        assert_eq!(out["structuredContent"]["code"], "unavailable", "{out}");
        eprintln!("skipping: {}", out["structuredContent"]["message"]);
        client.finish();
        return;
    }
    let image = &out["content"][0];
    assert_eq!(image["type"], "image", "{out}");
    assert_eq!(image["mimeType"], "image/png");
    assert!(image["data"].as_str().unwrap().starts_with("iVBORw0KGgo"));
    let structured = &out["structuredContent"];
    assert_eq!(structured["format"], "png");
    assert_eq!(structured["width"], 64);
    assert!(
        structured.get("png").is_none(),
        "the PNG is not repeated as text"
    );
    client.finish();
}

#[test]
fn unexpected_arguments_are_usage_errors() {
    let home = TempDir::new("mcp-home");
    let out = Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(["mcp", "--frob"])
        .env("REDA_HOME", home.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
