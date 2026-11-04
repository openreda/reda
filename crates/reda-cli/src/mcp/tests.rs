//! The MCP server over an in-memory pipe, in-process and bridged to a headless host, with
//! a long-running test Command (`test.slow`): progress notifications and cancellation
//! reach the Command's signals either way.

use std::time::{Duration, Instant};

use reda_control::headless::HeadlessHost;
use reda_engine::Engine;
use reda_engine::command::UserDirs;
use reda_engine::journal::Source;
use reda_testkit::{TempDir, slow_commands};
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};

use super::{Server, serve, writer};

struct Client {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    out: WriteHalf<DuplexStream>,
    server: tokio::task::JoinHandle<Result<(), String>>,
    thread: writer::WriterThread,
    /// Notifications seen while waiting for responses.
    notifications: Vec<Value>,
    /// Where the Project is created; removed on drop.
    dir: TempDir,
    /// The host the server bridges to, and its per-user folder.
    host: Option<(HeadlessHost, TempDir)>,
}

impl Client {
    /// An in-process server; `project.new` (over MCP) creates the Project "Demo".
    async fn start() -> Client {
        let (writer, thread, catalogue) = writer::spawn(|| {
            Engine::builder()
                .user_dirs(None)
                .commands(slow_commands)
                .build()
        });
        let mut client = Client::connect(Server::new(writer, &catalogue), thread, None).await;
        let path = client.dir.path().to_owned();
        let created = client
            .call(
                2,
                json!({"name": "run_command", "arguments": {
                    "id": "project.new", "params": {"path": path, "name": "Demo"}
                }}),
            )
            .await;
        assert_eq!(created["result"]["isError"], false, "{created}");
        client
    }

    /// A server bridged to a headless host that holds the Project "Demo".
    async fn bridged() -> Client {
        let dir = TempDir::new("mcp-unit-bridged");
        let home = TempDir::new("mcp-unit-home");
        let path = dir.path().to_owned();
        let host = HeadlessHost::spawn(&UserDirs::under(home.path()), move || {
            let mut engine = Engine::builder()
                .user_dirs(None)
                .commands(slow_commands)
                .build();
            engine
                .run("project.new", json!({"path": path, "name": "Demo"}))
                .unwrap();
            engine
        })
        .unwrap();
        let connection =
            reda_control::client::bridge(&UserDirs::under(home.path()), None, Source::Mcp)
                .unwrap()
                .expect("bridges to the host");
        let (writer, thread, catalogue) = writer::spawn_bridge(connection).unwrap();
        let mut client = Client::connect(
            Server::bridged(writer, &catalogue),
            thread,
            Some((host, home)),
        )
        .await;
        client.dir = dir;
        client
    }

    async fn connect(
        server: Server,
        thread: writer::WriterThread,
        host: Option<(HeadlessHost, TempDir)>,
    ) -> Client {
        let (ours, theirs) = tokio::io::duplex(1 << 16);
        let server = tokio::spawn(serve(server, tokio::io::split(theirs)));
        let (read, out) = tokio::io::split(ours);
        let mut client = Client {
            lines: BufReader::new(read).lines(),
            out,
            server,
            thread,
            notifications: Vec::new(),
            dir: TempDir::new("mcp-unit"),
            host,
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
        let leftover = self.thread.finish();
        if let Some((host, _home)) = self.host {
            host.stop();
        }
        leftover
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_notifications_carry_the_request_progress_token() {
    progress_notifications(Client::start().await).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_notifications_cross_the_bridge() {
    progress_notifications(Client::bridged().await).await;
}

async fn progress_notifications(mut client: Client) {
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
    let leftover = cancelling(Client::start().await).await;
    assert_eq!(leftover.project.as_deref(), Some("Demo"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_crosses_the_bridge() {
    cancelling(Client::bridged().await).await;
}

async fn cancelling(mut client: Client) -> super::Leftover {
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
    client.finish().await
}
