//! The session Journal: recording, JSON-lines export and replay with UUID remapping.

use reda_engine::command::{ErrorCode, FixedClock, SeededIds};
use reda_engine::journal::ScriptEntry;
use reda_engine::journal::{self, Source};
use reda_engine::{Engine, store};
use reda_testkit::{TempDir, test_engine};
use serde_json::{Value, json};

fn engine_seeded(seed: u64) -> Engine {
    Engine::builder()
        .ids(SeededIds::new(seed))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .build()
}

fn lines(text: &str) -> Vec<Value> {
    text.lines()
        .map(|l| serde_json::from_str(l).expect("each line is JSON"))
        .collect()
}

/// A script parsed from `text`, with its `project.new` steps pointed at a fresh temporary
/// folder, which must outlive the replay.
fn script(text: &str) -> (TempDir, Vec<ScriptEntry>) {
    let dir = TempDir::new("journal-replay");
    let mut steps = journal::parse(text).unwrap();
    journal::relocate_new_projects(&mut steps, dir.path());
    (dir, steps)
}

/// A session on seed 0: a new Project, a query naming the created Sheet, and a failure.
fn recorded_session() -> String {
    let dir = TempDir::new("journal-recorded");
    let mut e = test_engine();
    let sheet = e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}))["documents"]
        [0]["uuid"]
        .clone();
    e.run_ok("object.get", json!({ "uuid": sheet }));
    e.run_err(
        "object.get",
        json!({"uuid": "00000000-0000-4000-8000-0000000000ff"}),
    );
    journal::export(e.journal())
}

#[test]
fn the_journal_records_source_and_outcome_of_every_command() {
    let mut e = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .source(Source::Mcp)
        .build();
    e.run("project.overview", json!({})).unwrap_err();
    let dir = TempDir::new("journal-source");
    e.run("project.new", json!({"path": dir.path(), "name": "Demo"}))
        .unwrap();
    let journal = e.journal();
    assert_eq!(journal.len(), 2);
    assert!(journal.iter().all(|entry| entry.source == Source::Mcp));
    assert_eq!(
        journal[0].outcome.as_ref().unwrap_err().code,
        ErrorCode::Unavailable
    );
    assert_eq!(journal[1].outcome.as_ref().unwrap()["name"], "Demo");
}

#[test]
fn the_journal_exports_as_json_lines() {
    let exported = recorded_session();
    assert!(exported.ends_with('\n'));
    let lines = lines(&exported);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["id"], "project.new");
    assert_eq!(lines[0]["source"], "cli");
    assert_eq!(lines[0]["params"]["name"], "Demo");
    assert_eq!(
        lines[0]["result"]["documents"][0]["uuid"],
        "00000000-0000-4000-8000-000000000002"
    );
    assert!(lines[0].get("error").is_none());
    assert_eq!(lines[2]["error"]["code"], "not_found");
    assert!(lines[2].get("result").is_none());
}

#[test]
fn a_result_of_null_is_recorded() {
    let entry = journal::JournalEntry {
        id: "x".into(),
        params: json!({}),
        source: Source::Gui,
        outcome: Ok(Value::Null),
    };
    let exported = journal::export(std::slice::from_ref(&entry));
    assert_eq!(
        exported,
        "{\"id\":\"x\",\"source\":\"gui\",\"params\":{},\"result\":null}\n"
    );
    let script = journal::parse(&exported).unwrap();
    assert_eq!(script[0].outcome, Some(Ok(Value::Null)));
}

#[test]
fn replay_substitutes_created_uuids_in_later_params() {
    let (_dir, script) = script(&recorded_session());
    // Seed 7 creates different UUIDs, so the recorded Sheet UUID only resolves if the
    // replay substitutes the one created on replay.
    let mut e = engine_seeded(7);
    journal::replay(&mut e, &script).unwrap();
    let sheet = e.project().unwrap().documents().next().unwrap().uuid;
    assert_eq!(sheet.to_string(), "00000000-0007-4000-8000-000000000002");
    let replayed = lines(&journal::export(e.journal()));
    assert_eq!(replayed[1]["params"]["uuid"], sheet.to_string());
    assert_eq!(replayed[1]["result"]["object"]["uuid"], sheet.to_string());
}

#[test]
fn replay_reproduces_the_recorded_documents() {
    let (_dir, script) = script(&recorded_session());
    let mut e = engine_seeded(0);
    journal::replay(&mut e, &script).unwrap();
    let dir = TempDir::new("journal-original");
    let mut original = test_engine();
    original.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    assert_eq!(
        store::encode(e.project().unwrap()).unwrap(),
        store::encode(original.project().unwrap()).unwrap()
    );
}

#[test]
fn replay_fails_when_a_result_shape_differs() {
    let mut recorded = lines(&recorded_session());
    recorded[0]["result"]["extra"] = json!(1);
    let text: String = recorded.iter().map(|l| format!("{l}\n")).collect();
    let (_dir, script) = script(&text);
    let err = journal::replay(&mut engine_seeded(0), &script).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.pointer.as_deref(), Some("/extra"));
    assert!(err.message.contains("shape"), "{}", err.message);
}

#[test]
fn replay_fails_when_a_value_changes_type() {
    let mut recorded = lines(&recorded_session());
    recorded[0]["result"]["documents"] = json!({});
    let text: String = recorded.iter().map(|l| format!("{l}\n")).collect();
    let (_dir, script) = script(&text);
    let err = journal::replay(&mut engine_seeded(0), &script).unwrap_err();
    assert_eq!(err.pointer.as_deref(), Some("/documents"));
}

#[test]
fn replay_fails_when_an_array_length_differs() {
    let mut recorded = lines(&recorded_session());
    recorded[0]["result"]["documents"]
        .as_array_mut()
        .unwrap()
        .push(json!({}));
    let text: String = recorded.iter().map(|l| format!("{l}\n")).collect();
    let (_dir, script) = script(&text);
    let err = journal::replay(&mut engine_seeded(0), &script).unwrap_err();
    assert_eq!(err.pointer.as_deref(), Some("/documents"));
}

#[test]
fn replay_fails_when_a_recorded_success_now_fails() {
    let text = "{\"id\":\"project.overview\",\"params\":{},\"result\":{}}\n";
    let err = journal::replay(&mut engine_seeded(0), &journal::parse(text).unwrap()).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.id.as_deref(), Some("project.overview"));
    assert!(err.message.contains("unavailable"), "{}", err.message);
}

#[test]
fn replay_fails_when_a_recorded_error_now_succeeds_or_changes_code() {
    let text = "{\"id\":\"project.overview\",\"params\":{},\"error\":{\"code\":\"not_found\",\"message\":\"x\"}}\n";
    let err = journal::replay(&mut engine_seeded(0), &journal::parse(text).unwrap()).unwrap_err();
    assert!(err.message.contains("not_found"), "{}", err.message);

    let text = "{\"id\":\"project.new\",\"params\":{\"path\":\"/nonexistent\",\"name\":\"D\"},\"error\":{\"code\":\"rejected\",\"message\":\"x\"}}\n";
    let (_dir, script) = script(text);
    let err = journal::replay(&mut engine_seeded(0), &script).unwrap_err();
    assert!(err.message.contains("succeeded"), "{}", err.message);
}

#[test]
fn a_step_without_a_recorded_outcome_only_has_to_succeed() {
    let text = "\n{\"id\":\"project.new\",\"params\":{\"path\":\"/nonexistent\",\"name\":\"D\"}}\n\
                {\"id\":\"project.overview\"}\n";
    let (_dir, script) = script(text);
    let mut e = engine_seeded(0);
    journal::replay(&mut e, &script).unwrap();
    assert_eq!(e.journal().len(), 2);
}

#[test]
fn parse_reports_the_line_of_a_bad_entry() {
    let err = journal::parse("{\"id\":\"a\"}\n\nnot json\n").unwrap_err();
    assert_eq!(err.line, 3);
    let err = journal::parse("{\"id\":\"a\",\"bogus\":1}\n").unwrap_err();
    assert_eq!(err.line, 1);
    assert!(err.message.contains("bogus"), "{}", err.message);
    let err = journal::parse(
        "{\"id\":\"a\",\"result\":1,\"error\":{\"code\":\"internal\",\"message\":\"\"}}",
    )
    .unwrap_err();
    assert!(err.message.contains("both"), "{}", err.message);
}

#[test]
fn replay_with_random_uuids_remaps_uuid_keyed_changes_across_documents() {
    let dir = TempDir::new("journal-remap");
    let mut e = reda_testkit::probe_engine();
    let first = e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}))["documents"]
        [0]["uuid"]
        .clone();
    let second = e.run_ok("test.sheet.add", json!({"name": "B"}))["result"]["uuid"].clone();
    e.run_ok(
        "test.probe.set",
        json!({"probes": [
            {"uuid": "00000000-0000-4000-8000-0000000000a1", "document": first, "value": 1},
            {"uuid": "00000000-0000-4000-8000-0000000000a2", "document": second, "value": 2},
        ]}),
    );
    e.run_ok("edit.undo", json!({ "document": second }));
    let (_dir, script) = script(&journal::export(e.journal()));

    let mut replayed = Engine::builder()
        .user_dirs(None)
        .commands(reda_testkit::probe_commands)
        .build();
    journal::replay(&mut replayed, &script).unwrap();
    let docs: Vec<_> = replayed
        .project()
        .unwrap()
        .documents()
        .map(|d| d.uuid.to_string())
        .collect();
    assert_eq!(docs.len(), 3, "the sheet, the PCB Document and sheet B");
    assert_ne!(docs[2], second.as_str().unwrap());
    assert_eq!(replayed.journal()[3].params["document"], docs[2]);
}
