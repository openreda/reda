//! The control port against a headless host: the instance file, token auth, the three
//! methods, and the bridge refusing to save.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;

use reda_control::client;
use reda_control::headless::HeadlessHost;
use reda_engine::Engine;
use reda_engine::command::{ErrorCode, UserDirs};
use reda_engine::journal::Source;
use reda_testkit::{TempDir, read_tree};
use serde_json::{Value, json};

/// A headless host serving a fresh engine with no Project, its folders under `home`.
fn host(home: &Path) -> HeadlessHost {
    let dirs = UserDirs::under(home);
    HeadlessHost::spawn(&dirs, || Engine::builder().user_dirs(None).build()).expect("host starts")
}

fn instance_files(home: &Path) -> Vec<std::path::PathBuf> {
    let dir = UserDirs::under(home).instances_dir();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries.map(|e| e.unwrap().path()).collect()
}

#[test]
fn the_host_writes_its_instance_file_and_removes_it_on_stop() {
    let home = TempDir::new("control-instance");
    let host = host(home.path());
    let files = instance_files(home.path());
    assert_eq!(files.len(), 1, "{files:?}");
    let file = &files[0];
    // `<pid>.json`, or `<pid>-<n>.json` for a further server in this test process.
    let name = file.file_name().unwrap().to_str().unwrap();
    let stem = name.strip_suffix(".json").unwrap();
    let pid = stem.split('-').next().unwrap();
    assert_eq!(pid, std::process::id().to_string(), "{name}");
    let instance: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(instance["pid"], std::process::id());
    assert_eq!(instance["port"], host.port());
    assert_ne!(instance["port"], 0);
    let token = instance["token"].as_str().unwrap();
    assert_eq!(token.len(), 64, "256 bits as hex");
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(instance["project"], Value::Null);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(file), 0o600);
        assert_eq!(mode(file.parent().unwrap()), 0o700);
    }

    host.stop();
    assert!(instance_files(home.path()).is_empty());
}

/// A raw line-oriented connection to the port.
struct Raw {
    reader: BufReader<TcpStream>,
    stream: TcpStream,
}

impl Raw {
    fn connect(port: u16) -> Raw {
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        Raw {
            reader: BufReader::new(stream.try_clone().unwrap()),
            stream,
        }
    }

    fn call(&mut self, message: Value) -> Option<Value> {
        self.call_line(&message.to_string())
    }

    fn call_line(&mut self, message: &str) -> Option<Value> {
        writeln!(self.stream, "{message}").unwrap();
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(serde_json::from_str(&line).unwrap()),
        }
    }

    fn closed(&mut self) -> bool {
        let mut line = String::new();
        matches!(self.reader.read_line(&mut line), Ok(0) | Err(_))
    }
}

fn token(home: &Path) -> String {
    let file = &instance_files(home)[0];
    let instance: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    instance["token"].as_str().unwrap().to_owned()
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

#[test]
fn the_first_message_must_present_the_token() {
    let home = TempDir::new("control-auth");
    let host = host(home.path());

    let mut raw = Raw::connect(host.port());
    let reply = raw.call(request(1, "commands.list", json!({}))).unwrap();
    assert_eq!(reply["error"]["code"], -32001, "{reply}");
    assert!(raw.closed());

    let mut raw = Raw::connect(host.port());
    let wrong = "0".repeat(64);
    let reply = raw
        .call(request(1, "auth", json!({"token": wrong})))
        .unwrap();
    assert_eq!(reply["error"]["code"], -32001, "{reply}");
    assert!(raw.closed());

    let mut raw = Raw::connect(host.port());
    let reply = raw
        .call(request(1, "auth", json!({"token": token(home.path())})))
        .unwrap();
    assert_eq!(reply["result"]["project"], Value::Null, "{reply}");
    let reply = raw.call(request(2, "commands.list", json!({}))).unwrap();
    assert!(reply["result"]["commands"].as_array().unwrap().len() > 3);

    let code = |reply: Option<Value>| reply.unwrap()["error"]["code"].clone();
    assert_eq!(code(raw.call_line("{not json")), -32700);
    assert_eq!(
        code(raw.call(request(3, "commands.nope", json!({})))),
        -32601
    );
    assert_eq!(
        code(raw.call(request(4, "commands.run", json!({"id": 5})))),
        -32602
    );
    assert_eq!(
        code(raw.call(json!({"id": 5, "method": "commands.list"}))),
        -32600
    );
    let failed = raw
        .call(request(
            6,
            "commands.run",
            json!({"id": "project.overview"}),
        ))
        .unwrap();
    assert_eq!(failed["error"]["code"], -32000, "{failed}");
    assert_eq!(failed["error"]["data"]["code"], "unavailable", "{failed}");
    host.stop();
}

/// Saves a new Project called `name` under `dir` and returns its `.reda` file.
fn saved_project(dir: &Path, name: &str) -> std::path::PathBuf {
    let mut engine = Engine::builder().user_dirs(None).build();
    engine
        .run("project.new", json!({"path": dir, "name": name}))
        .unwrap();
    let saved = engine.run("project.save", json!({})).unwrap();
    std::path::PathBuf::from(saved["path"].as_str().unwrap())
}

/// A host that has `project` open.
fn host_with(home: &Path, project: &Path) -> HeadlessHost {
    let project = project.to_path_buf();
    HeadlessHost::spawn(&UserDirs::under(home), move || {
        let mut engine = Engine::builder().user_dirs(None).build();
        engine
            .run("project.open", json!({ "path": project }))
            .unwrap();
        engine
    })
    .expect("host starts")
}

#[test]
fn a_client_lists_describes_and_runs_commands_on_the_host() {
    let home = TempDir::new("control-run");
    let work = TempDir::new("control-run-work");
    let project = saved_project(work.path(), "Demo");
    let before = read_tree(work.path());
    let host = host_with(home.path(), &project);

    let dirs = UserDirs::under(home.path());
    let mut client = client::bridge(&dirs, Some(&project), Source::Mcp).expect("bridges");
    assert_eq!(
        client.project(),
        Some(reda_control::normalize(&project).as_path())
    );

    let catalogue = client.list().unwrap();
    let ids: Vec<&str> = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"document.add"), "{ids:?}");
    assert_eq!(client.describe("document.add").unwrap()["kind"], "edit");
    let err = client.describe("no.such").unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);

    let added = client
        .run(
            "document.add",
            json!({"kind": "schematic", "title": "Power"}),
        )
        .unwrap();
    assert!(added["changes"].is_object(), "{added}");
    let overview = client.run("project.overview", json!({})).unwrap();
    assert!(overview.to_string().contains("Power"), "{overview}");
    let err = client
        .run("document.add", json!({"kind": "schematic", "nope": 1}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);

    drop(client);
    let leftover = host.stop();
    assert!(
        leftover.dirty_files > 0,
        "the edit shows as unsaved on the host"
    );
    let bridged: Vec<Source> = leftover
        .journal
        .iter()
        .filter(|e| e.id == "document.add")
        .map(|e| e.source)
        .collect();
    assert_eq!(bridged, [Source::Mcp, Source::Mcp]);
    assert_eq!(read_tree(work.path()), before, "nothing was written");
}

#[test]
fn bridged_sessions_never_save_or_replace_the_project() {
    let home = TempDir::new("control-save");
    let work = TempDir::new("control-save-work");
    let project = saved_project(work.path(), "Demo");
    let host = host_with(home.path(), &project);
    let mut client =
        client::bridge(&UserDirs::under(home.path()), Some(&project), Source::Cli).unwrap();
    client
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let before = read_tree(work.path());
    for (id, params) in [
        ("project.save", json!({})),
        ("project.open", json!({"path": project})),
        ("project.new", json!({"path": work.path(), "name": "Other"})),
    ] {
        let err = client.run(id, params).unwrap_err();
        assert_eq!(err.code, ErrorCode::Rejected, "{id}: {err}");
    }
    assert_eq!(read_tree(work.path()), before);
    drop(client);
    let leftover = host.stop();
    assert!(leftover.dirty_files > 0);
    assert!(
        leftover
            .journal
            .iter()
            .all(|e| !e.id.starts_with("project.save"))
    );
}

#[test]
fn bridge_picks_the_instance_holding_the_project_or_the_only_one() {
    let home = TempDir::new("control-pick");
    let work = TempDir::new("control-pick-work");
    let dirs = UserDirs::under(home.path());
    let a = saved_project(work.path(), "A");
    let b = saved_project(work.path(), "B");
    let none = |project: Option<&Path>| client::bridge(&dirs, project, Source::Cli).is_none();

    assert!(none(None), "no instances");
    let empty = host(home.path());
    assert!(none(None), "an instance with no Project is not bridged to");
    assert!(none(Some(&a)));

    let host_a = host_with(home.path(), &a);
    // The folder names the Project as well as its `.reda` file.
    let by_folder = client::bridge(&dirs, Some(a.parent().unwrap()), Source::Cli).unwrap();
    assert_eq!(
        by_folder.project(),
        Some(reda_control::normalize(&a).as_path())
    );
    let only = client::bridge(&dirs, None, Source::Cli).expect("the only instance with a Project");
    assert_eq!(only.project(), by_folder.project());
    assert!(none(Some(&b)));

    // A stale file (its instance died) is skipped.
    let stale = json!({
        "pid": 1, "port": 1, "token": "0".repeat(64), "project": reda_control::normalize(&b)
    });
    std::fs::write(dirs.instances_dir().join("1.json"), stale.to_string()).unwrap();
    assert!(none(Some(&b)));
    assert!(client::bridge(&dirs, None, Source::Cli).is_some());

    // The host reports a Project it opens itself.
    empty
        .server()
        .set_project(Some(reda_control::normalize(&b)))
        .unwrap();
    assert!(client::bridge(&dirs, Some(&b), Source::Cli).is_some());
    assert!(none(None), "two instances have a Project open");

    drop((by_folder, only));
    host_a.stop();
    empty.stop();
}
