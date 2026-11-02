//! The control port against a headless host: the instance file, token auth, the three
//! methods, and the bridge refusing to save.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};

use reda_control::client;
use reda_control::headless::HeadlessHost;
use reda_engine::Engine;
use reda_engine::command::{CancelToken, ErrorCode, Progress, Signals, UserDirs};
use reda_engine::journal::Source;
use reda_testkit::{TempDir, read_tree};
use serde_json::{Value, json};

/// A headless host serving a fresh engine with no Project, its folders under `home`.
fn host(home: &Path) -> HeadlessHost {
    let dirs = UserDirs::under(home);
    HeadlessHost::spawn(&dirs, || Engine::builder().user_dirs(None).build()).expect("host starts")
}

/// [`client::bridge`], which must not be refused.
fn bridge(dirs: &UserDirs, project: Option<&Path>, source: Source) -> Option<client::Connection> {
    client::bridge(dirs, project, source).expect("not refused")
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
    let mut client = bridge(&dirs, Some(&project), Source::Mcp).expect("bridges");
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
    let mut client = bridge(&UserDirs::under(home.path()), Some(&project), Source::Cli).unwrap();
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
    let none = |project: Option<&Path>| bridge(&dirs, project, Source::Cli).is_none();

    assert!(none(None), "no instances");
    let empty = host(home.path());
    assert!(none(None), "an instance with no Project is not bridged to");
    assert!(none(Some(&a)));

    let host_a = host_with(home.path(), &a);
    // The folder names the Project as well as its `.reda` file.
    let by_folder = bridge(&dirs, Some(a.parent().unwrap()), Source::Cli).unwrap();
    assert_eq!(
        by_folder.project(),
        Some(reda_control::normalize(&a).as_path())
    );
    let only = bridge(&dirs, None, Source::Cli).expect("the only instance with a Project");
    assert_eq!(only.project(), by_folder.project());
    assert!(none(Some(&b)));

    // A stale file (its instance died) is skipped.
    let stale = json!({
        "pid": 1, "port": 1, "token": "0".repeat(64), "project": reda_control::normalize(&b)
    });
    std::fs::write(dirs.instances_dir().join("1.json"), stale.to_string()).unwrap();
    assert!(none(Some(&b)));
    assert!(bridge(&dirs, None, Source::Cli).is_some());

    // The host reports a Project it opens itself.
    empty
        .server()
        .set_project(Some(reda_control::normalize(&b)))
        .unwrap();
    assert!(bridge(&dirs, Some(&b), Source::Cli).is_some());
    assert!(none(None), "two instances have a Project open");

    drop((by_folder, only));
    host_a.stop();
    empty.stop();
}

/// A host with a new Project open and `test.slow` registered.
fn slow_host(home: &Path, work: &Path) -> HeadlessHost {
    let work = work.to_path_buf();
    HeadlessHost::spawn(&UserDirs::under(home), move || {
        let mut engine = Engine::builder()
            .user_dirs(None)
            .commands(reda_testkit::slow_commands)
            .build();
        engine
            .run("project.new", json!({"path": work, "name": "Demo"}))
            .unwrap();
        engine
    })
    .expect("host starts")
}

/// Collects progress reports, and cancels `cancel` on the first one when set.
struct Reports {
    seen: Mutex<Vec<(f64, Option<f64>, String)>>,
    cancel: Option<CancelToken>,
}

impl Progress for Reports {
    fn report(&self, progress: f64, total: Option<f64>, message: &str) {
        self.seen
            .lock()
            .unwrap()
            .push((progress, total, message.to_owned()));
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }
}

#[test]
fn progress_and_cancellation_cross_the_port() {
    let home = TempDir::new("control-progress");
    let work = TempDir::new("control-progress-work");
    let host = slow_host(home.path(), work.path());
    let mut client = bridge(&UserDirs::under(home.path()), None, Source::Mcp).unwrap();

    let reports = Arc::new(Reports {
        seen: Mutex::new(Vec::new()),
        cancel: None,
    });
    let signals = Signals::new(CancelToken::new()).with_progress(reports.clone());
    let out = client
        .run_with("test.slow", json!({"steps": 2, "wait": false}), &signals)
        .unwrap();
    assert_eq!(out["result"], 2);
    assert_eq!(
        *reports.seen.lock().unwrap(),
        [
            (1.0, Some(2.0), "working".to_owned()),
            (2.0, Some(2.0), "working".to_owned())
        ]
    );

    // Cancelling while the Command runs stops it and rolls it back.
    let cancel = CancelToken::new();
    let reports = Arc::new(Reports {
        seen: Mutex::new(Vec::new()),
        cancel: Some(cancel.clone()),
    });
    let signals = Signals::new(cancel).with_progress(reports.clone());
    let started = std::time::Instant::now();
    let err = client
        .run_with("test.slow", json!({"steps": 1, "wait": true}), &signals)
        .unwrap_err();
    assert!(err.is_cancelled(), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));

    // A run cancelled before it is sent is not run at all.
    let cancelled = CancelToken::new();
    cancelled.cancel();
    let err = client
        .run_with(
            "test.slow",
            json!({"steps": 1, "wait": false}),
            &Signals::new(cancelled),
        )
        .unwrap_err();
    assert!(err.is_cancelled(), "{err}");

    drop(client);
    let leftover = host.stop();
    let slow: Vec<bool> = leftover
        .journal
        .iter()
        .filter(|e| e.id == "test.slow")
        .map(|e| e.outcome.is_ok())
        .collect();
    assert_eq!(slow, [true, false]);
}

#[test]
fn progress_goes_out_as_notifications_before_the_response() {
    let home = TempDir::new("control-progress-raw");
    let work = TempDir::new("control-progress-raw-work");
    let host = slow_host(home.path(), work.path());
    let mut raw = Raw::connect(host.port());
    raw.call(request(1, "auth", json!({"token": token(home.path())})))
        .unwrap();
    let first = raw
        .call(request(
            2,
            "commands.run",
            json!({"id": "test.slow", "params": {"steps": 1, "wait": false}, "progress": true}),
        ))
        .unwrap();
    assert_eq!(
        first,
        json!({"jsonrpc": "2.0", "method": "progress",
               "params": {"id": 2, "progress": 1.0, "total": 1.0, "message": "working"}})
    );
    let mut line = String::new();
    raw.reader.read_line(&mut line).unwrap();
    let response: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(response["id"], 2, "{response}");
    assert_eq!(response["result"]["result"], 1, "{response}");
    host.stop();
}

#[test]
fn a_gated_server_asks_the_host_to_admit_each_agent() {
    use reda_control::{Agent, ControlServer};
    let home = TempDir::new("control-gate");
    let work = TempDir::new("control-gate-work");
    let project = saved_project(work.path(), "Demo");
    let dirs = UserDirs::under(home.path());
    let catalogue = json!({"commands": []});
    let (server, calls, admissions) =
        ControlServer::start_gated(&dirs, &catalogue, Some(reda_control::normalize(&project)))
            .unwrap();
    // The host: allows "Alpha", denies everyone else, and answers every run with its id.
    let asked = std::thread::spawn(move || {
        let mut asked = Vec::new();
        for admission in admissions {
            asked.push(admission.agent.clone());
            match admission.agent.name.as_str() {
                "Alpha" => admission.allow(),
                _ => admission.deny(),
            }
        }
        asked
    });
    let runs = std::thread::spawn(move || {
        for call in calls {
            let _ = call.run(|id, _, _| Ok(json!(id)));
        }
    });
    let instance = client::instances(&dirs).remove(0);
    let connect = |source, agent| client::Connection::connect_as(&instance, source, agent);

    let mut alpha = connect(Source::Mcp, "Alpha").expect("allowed");
    assert_eq!(
        alpha.run("document.add", json!({})).unwrap(),
        "document.add"
    );
    let alpha_agent = Agent {
        name: "Alpha".into(),
        source: Source::Mcp,
    };
    assert_eq!(server.agents(), [alpha_agent.clone()]);

    let denied = connect(Source::Cli, "Beta").err().expect("denied");
    assert_eq!(denied.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(server.agents(), [alpha_agent.clone()]);

    // Another REDA app is let in without asking, but may only hand over a file.
    let mut app = connect(Source::Gui, "REDA").expect("the app's hand-off");
    assert_eq!(
        app.run("app.file.open", json!({"path": "x"})).unwrap(),
        "app.file.open"
    );
    let err = app.run("document.add", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);

    drop((alpha, app));
    server.stop();
    drop(server);
    runs.join().unwrap();
    let asked: Vec<String> = asked.join().unwrap().into_iter().map(|a| a.name).collect();
    assert_eq!(asked, ["Alpha", "Beta"]);
}

#[test]
fn bridging_to_an_instance_that_denies_the_agent_is_an_error_not_in_process() {
    use reda_control::ControlServer;
    let home = TempDir::new("control-deny");
    let work = TempDir::new("control-deny-work");
    let project = saved_project(work.path(), "Demo");
    let dirs = UserDirs::under(home.path());
    let (server, _calls, admissions) = ControlServer::start_gated(
        &dirs,
        &json!({"commands": []}),
        Some(reda_control::normalize(&project)),
    )
    .unwrap();
    let host = std::thread::spawn(move || {
        for admission in admissions {
            admission.deny();
        }
    });
    let err = client::bridge(&dirs, Some(&project), Source::Mcp)
        .err()
        .expect("refused");
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("did not allow"), "{err}");
    server.stop();
    drop(server);
    host.join().unwrap();
}
