//! The always-bridge, end to end: `reda run` and `reda mcp` against a headless host in
//! this test process, found through the instance file under `REDA_HOME`.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use reda_control::headless::{HeadlessHost, Leftover};
use reda_engine::Engine;
use reda_engine::command::UserDirs;
use reda_engine::journal::Source;
use reda_testkit::{TempDir, read_tree};
use serde_json::{Value, json};

/// A host with a saved Project open, in its own `REDA_HOME`.
struct Fixture {
    home: TempDir,
    work: TempDir,
    project: PathBuf,
    host: HeadlessHost,
}

impl Fixture {
    fn new(label: &str) -> Fixture {
        let home = TempDir::new(&format!("{label}-home"));
        let work = TempDir::new(&format!("{label}-work"));
        let mut engine = Engine::builder().user_dirs(None).build();
        engine
            .run("project.new", json!({"path": work.path(), "name": "Demo"}))
            .unwrap();
        let saved = engine.run("project.save", json!({})).unwrap();
        let project = PathBuf::from(saved["path"].as_str().unwrap());
        let open = project.clone();
        let host = HeadlessHost::spawn(&UserDirs::under(home.path()), move || {
            let mut engine = Engine::builder().user_dirs(None).build();
            engine.run("project.open", json!({ "path": open })).unwrap();
            engine
        })
        .unwrap();
        Fixture {
            home,
            work,
            project,
            host,
        }
    }

    fn reda(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_reda"))
            .args(args)
            .env("REDA_HOME", self.home.path())
            .output()
            .expect("run reda")
    }

    fn project(&self) -> &str {
        self.project.to_str().unwrap()
    }

    fn files(&self) -> std::collections::BTreeMap<String, Vec<u8>> {
        read_tree(self.work.path())
    }

    fn stop(self) -> Leftover {
        self.host.stop()
    }
}

fn ok_json(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "reda failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is JSON")
}

fn sources(leftover: &Leftover, id: &str) -> Vec<Source> {
    leftover
        .journal
        .iter()
        .filter(|e| e.id == id)
        .map(|e| e.source)
        .collect()
}

#[test]
fn run_bridges_to_the_instance_that_has_the_project_open() {
    let fixture = Fixture::new("bridge-run");
    let before = fixture.files();
    let out = fixture.reda(&[
        "run",
        "document.add",
        "--kind",
        "schematic",
        "--title",
        "Power",
        "--project",
        fixture.project(),
    ]);
    let added = ok_json(&out);
    assert!(added["changes"].is_object(), "{added}");
    assert_eq!(fixture.files(), before, "a bridged run writes nothing");

    // The host session holds the change, so a second run sees it.
    let overview =
        ok_json(&fixture.reda(&["run", "project.overview", "--project", fixture.project()]));
    assert!(overview.to_string().contains("Power"), "{overview}");

    let leftover = fixture.stop();
    assert_eq!(sources(&leftover, "document.add"), [Source::Cli]);
    assert!(
        leftover.dirty_files > 0,
        "the edit shows as unsaved in the host"
    );
}

#[test]
fn run_without_project_bridges_to_the_only_instance() {
    let fixture = Fixture::new("bridge-only");
    let overview = ok_json(&fixture.reda(&["run", "project.overview"]));
    assert_eq!(overview["name"], "Demo", "{overview}");
    let leftover = fixture.stop();
    assert_eq!(sources(&leftover, "project.overview"), [Source::Cli]);
}

#[test]
fn bridged_runs_never_save() {
    let fixture = Fixture::new("bridge-save");
    let before = fixture.files();
    let out = fixture.reda(&[
        "run",
        "document.add",
        "--kind",
        "schematic",
        "--project",
        fixture.project(),
        "--save",
    ]);
    ok_json(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not saving"), "{stderr}");

    let out = fixture.reda(&["run", "project.save", "--project", fixture.project()]);
    assert_eq!(out.status.code(), Some(1));
    let err: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["code"], "rejected", "{err}");

    assert_eq!(fixture.files(), before);
    let leftover = fixture.stop();
    assert!(sources(&leftover, "project.save").is_empty());
    assert!(leftover.dirty_files > 0);
}

#[test]
fn project_new_and_open_run_in_process_even_with_an_instance() {
    let fixture = Fixture::new("bridge-new");
    let other = TempDir::new("bridge-new-other");
    let params = json!({"path": other.path(), "name": "Other"}).to_string();
    let out = fixture.reda(&["run", "project.new", "--params", &params, "--save"]);
    assert_eq!(ok_json(&out)["name"], "Other");
    assert!(other.path().join("Other").join("Other.reda").is_file());
    let leftover = fixture.stop();
    assert!(sources(&leftover, "project.new").is_empty());
}

#[test]
fn a_script_replays_over_the_bridge() {
    let fixture = Fixture::new("bridge-script");
    let script = fixture.work.path().join("steps.jsonl");
    std::fs::write(
        &script,
        "{\"id\":\"document.add\",\"params\":{\"kind\":\"schematic\",\"title\":\"Power\"}}\n\
         {\"id\":\"project.overview\"}\n",
    )
    .unwrap();
    let out = fixture.reda(&[
        "run",
        "--script",
        script.to_str().unwrap(),
        "--project",
        fixture.project(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let ids: Vec<&str> = lines.iter().map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["document.add", "project.overview"]);
    assert!(lines[1]["result"].to_string().contains("Power"));
    let leftover = fixture.stop();
    assert_eq!(sources(&leftover, "document.add"), [Source::Cli]);
}

/// Runs `reda mcp` with `args`, makes the `tools/call`s in order, closes stdin, and returns
/// each call's result and the server's stderr.
fn mcp(fixture: &Fixture, args: &[&str], calls: &[(&str, Value)]) -> (Vec<Value>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_reda"))
        .arg("mcp")
        .args(args)
        .env("REDA_HOME", fixture.home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut request = |id: usize, method: &str, params: Value| -> Value {
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(stdin, "{message}").unwrap();
        loop {
            let reply: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
            if reply["id"] == id {
                return reply;
            }
        }
    };
    request(
        0,
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "0"}}),
    );
    let results = calls
        .iter()
        .enumerate()
        .map(|(i, (tool, arguments))| {
            request(
                i + 1,
                "tools/call",
                json!({"name": tool, "arguments": arguments}),
            )["result"]
                .clone()
        })
        .collect();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    (results, String::from_utf8(out.stderr).unwrap())
}

#[test]
fn mcp_bridges_and_its_edits_stay_unsaved_in_the_host() {
    let fixture = Fixture::new("bridge-mcp");
    let before = fixture.files();
    let (results, stderr) = mcp(
        &fixture,
        &["--project", fixture.project()],
        &[
            (
                "run_command",
                json!({"id": "document.add", "params": {"kind": "schematic", "title": "Power"}}),
            ),
            ("project.overview", json!({})),
            ("run_command", json!({"id": "project.save"})),
        ],
    );
    assert_eq!(results[0]["isError"], false, "{}", results[0]);
    assert!(
        results[1]["structuredContent"]
            .to_string()
            .contains("Power")
    );
    assert_eq!(results[2]["isError"], true);
    assert_eq!(results[2]["structuredContent"]["code"], "rejected");
    assert!(
        !stderr.contains("discarding"),
        "the host keeps the changes: {stderr}"
    );
    assert_eq!(fixture.files(), before);
    let leftover = fixture.stop();
    assert_eq!(sources(&leftover, "document.add"), [Source::Mcp]);
    assert!(leftover.dirty_files > 0);
}

#[test]
fn without_an_instance_the_cli_runs_in_process() {
    let home = TempDir::new("bridge-none-home");
    let out = Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(["run", "project.overview"])
        .env("REDA_HOME", home.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["code"], "unavailable", "no Project is open in-process");
}
