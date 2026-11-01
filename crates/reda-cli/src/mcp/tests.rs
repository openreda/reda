//! The MCP server in-process, over an in-memory pipe, with a long-running test Command:
//! progress notifications and cancellation reach the Command's signals.

use std::time::{Duration, Instant};

use reda_engine::Engine;
use reda_engine::command::{Command, CommandError, Context, Kind, Meta};
use reda_engine::model::Object;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};

use super::{Server, serve, writer};

/// Renames the Project to "slow", reports `steps` progress steps, then (with `wait`)
/// waits for cancellation, giving up after ten seconds.
struct Slow;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SlowParams {
    steps: u32,
    wait: bool,
}

impl Command for Slow {
    const META: Meta = Meta {
        id: "test.slow",
        title: "Slow",
        description: "A long-running edit for tests.",
        category: "test",
        icon: "slow",
        kind: Kind::Edit,
    };
    type Params = SlowParams;
    type Output = u32;

    fn run(&self, cx: &mut Context<'_>, p: SlowParams) -> Result<u32, CommandError> {
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        props.name = "slow".into();
        tx.replace(Object::Project(props))?;
        for step in 1..=p.steps {
            cx.progress(f64::from(step), Some(f64::from(p.steps)), "working");
        }
        let start = Instant::now();
        while p.wait && start.elapsed() < Duration::from_secs(10) {
            cx.check_cancelled()?;
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(p.steps)
    }
}

struct Client {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    out: WriteHalf<DuplexStream>,
    server: tokio::task::JoinHandle<Result<(), String>>,
    thread: writer::WriterThread,
    /// Notifications seen while waiting for responses.
    notifications: Vec<Value>,
}

impl Client {
    async fn start() -> Client {
        let (writer, thread, catalogue) = writer::spawn(|| {
            Engine::builder()
                .user_dirs(None)
                .commands(|r| r.add(Slow))
                .build()
        });
        let (ours, theirs) = tokio::io::duplex(1 << 16);
        let server = tokio::spawn(serve(
            Server::new(writer, &catalogue),
            tokio::io::split(theirs),
        ));
        let (read, out) = tokio::io::split(ours);
        let mut client = Client {
            lines: BufReader::new(read).lines(),
            out,
            server,
            thread,
            notifications: Vec::new(),
        };
        client
            .request(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "0"}
                }),
            )
            .await;
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        let created = client
            .call(
                2,
                json!({"name": "run_command", "arguments": {
                    "id": "project.new", "params": {"path": "/nonexistent", "name": "Demo"}
                }}),
            )
            .await;
        assert_eq!(created["result"]["isError"], false, "{created}");
        client
    }

    async fn send(&mut self, message: Value) {
        let line = format!("{message}\n");
        self.out.write_all(line.as_bytes()).await.unwrap();
        self.out.flush().await.unwrap();
    }

    async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        self.response(id).await
    }

    async fn call(&mut self, id: u64, params: Value) -> Value {
        self.request(id, "tools/call", params).await
    }

    /// Reads until the response to `id`, keeping notifications.
    async fn response(&mut self, id: u64) -> Value {
        loop {
            let line = tokio::time::timeout(Duration::from_secs(30), self.lines.next_line())
                .await
                .expect("the server answers in time")
                .unwrap()
                .expect("the server keeps the pipe open");
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
            self.notifications.push(message);
        }
    }

    async fn name(&mut self, id: u64) -> Value {
        let overview = self
            .call(id, json!({"name": "project.overview", "arguments": {}}))
            .await;
        overview["result"]["structuredContent"]["name"].clone()
    }

    async fn finish(self) -> super::Leftover {
        drop(self.out);
        drop(self.lines);
        self.server.await.unwrap().unwrap();
        self.thread.finish()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_notifications_carry_the_request_progress_token() {
    let mut client = Client::start().await;
    let done = client
        .call(
            3,
            json!({
                "name": "run_command",
                "arguments": {"id": "test.slow", "params": {"steps": 2, "wait": false}},
                "_meta": {"progressToken": "tok"}
            }),
        )
        .await;
    assert_eq!(done["result"]["structuredContent"]["result"], 2, "{done}");
    let progress: Vec<&Value> = client
        .notifications
        .iter()
        .filter(|n| n["method"] == "notifications/progress")
        .map(|n| &n["params"])
        .collect();
    assert_eq!(
        progress,
        [
            &json!({"progressToken": "tok", "progress": 1.0, "total": 2.0, "message": "working"}),
            &json!({"progressToken": "tok", "progress": 2.0, "total": 2.0, "message": "working"}),
        ]
    );
    client.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_progress_token_means_no_progress_notifications() {
    let mut client = Client::start().await;
    client
        .call(
            3,
            json!({
                "name": "run_command",
                "arguments": {"id": "test.slow", "params": {"steps": 2, "wait": false}}
            }),
        )
        .await;
    assert!(
        client
            .notifications
            .iter()
            .all(|n| n["method"] != "notifications/progress")
    );
    client.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_the_request_cancels_the_command_and_rolls_it_back() {
    let mut client = Client::start().await;
    client
        .send(json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {
                "name": "run_command",
                "arguments": {"id": "test.slow", "params": {"steps": 1, "wait": true}},
                "_meta": {"progressToken": "slow"}
            }
        }))
        .await;
    // The first progress notification shows the Command is running.
    loop {
        let line = client.lines.next_line().await.unwrap().unwrap();
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["method"] == "notifications/progress" {
            break;
        }
    }
    client
        .send(json!({
            "jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": 3, "reason": "test"}
        }))
        .await;
    // The writer runs one Command at a time, so this answers only once the slow one has
    // stopped; well inside the ten seconds it would otherwise wait.
    let start = Instant::now();
    assert_eq!(client.name(4).await, "Demo");
    assert!(start.elapsed() < Duration::from_secs(5));
    let leftover = client.finish().await;
    assert_eq!(leftover.project.as_deref(), Some("Demo"));
}
