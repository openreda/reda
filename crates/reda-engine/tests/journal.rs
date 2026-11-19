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

#[test]
fn relocating_a_new_project_also_moves_later_paths_inside_its_folder() {
    let text = [
        r#"{"id":"project.new","params":{"path":"/nonexistent","name":"Demo"}}"#,
        r#"{"id":"project.open","params":{"path":"/nonexistent/Demo/Demo.reda"}}"#,
        r#"{"id":"project.open","params":{"path":"/nonexistent"}}"#,
    ]
    .join("\n");
    let (dir, script) = script(&text);
    assert_eq!(
        script[1].params["path"],
        json!(dir.path().join("1").join("Demo/Demo.reda"))
    );
    assert_eq!(script[2].params["path"], json!(dir.path().join("1")));
    let mut e = test_engine();
    journal::replay(&mut e, &script[..2]).unwrap();
}

#[test]
fn replay_with_random_uuids_remaps_the_change_list_a_sync_names() {
    // A reference Project's script: it previews Sync and applies it by the Change List's
    // hash, which is built from the UUIDs of what it lists.
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/scenarios/minimal_board.jsonl"),
    )
    .unwrap();
    let (_dir, script) = script(&text);
    let mut replayed = Engine::builder()
        .user_dirs(None)
        .starter_library(Some(reda_testkit::starter_library()))
        .build();
    journal::replay(&mut replayed, &script).unwrap();
    let pcb = replayed.project().unwrap().pcb().unwrap();
    assert_eq!(pcb.footprints().count(), 3);
}

#[test]
fn replay_maps_copper_and_dielectric_identities_created_by_apply_construction() {
    let dir = TempDir::new("construction-replay");
    let mut e = reda_testkit::TestEngine(Engine::builder().user_dirs(None).build());
    let new = e.run_ok("project.new", json!({"path":dir.path(),"name":"Replay"}));
    let document = &new["documents"][1]["uuid"];
    let preset = e.project().unwrap().fab_presets().next().unwrap().uuid;
    let p = json!({"document":document,"preset":preset,"construction":"4 Layer"});
    e.run_ok(
        "pcb.stackup.change_preview",
        json!({"change":{"id":"pcb.stackup.apply_construction","params":p}}),
    );
    let ids = e.run_ok("pcb.stackup.apply_construction", p);
    e.run_ok(
        "pcb.layer.set",
        json!({"layer":ids["result"]["layers"][1],"name":"Replay return"}),
    );
    e.run_ok("pcb.stackup.set",json!({"dielectrics":[{"uuid":ids["result"]["dielectrics"][1],"material":"Test material"}]}));
    let (_dir, script) = script(&journal::export(e.journal()));
    let mut replayed = Engine::builder().user_dirs(None).build();
    journal::replay(&mut replayed, &script).unwrap();
    let board = replayed.project().unwrap().pcb().unwrap();
    assert_eq!(
        board.copper_layers().nth(1).unwrap().name.as_deref(),
        Some("Replay return")
    );
    assert_eq!(
        board
            .stackup()
            .dielectrics()
            .nth(1)
            .unwrap()
            .material
            .as_deref(),
        Some("Test material")
    );
}

#[test]
fn a_new_pcb_documents_seeded_rows_replay_from_an_independent_id_source() {
    let work = TempDir::new("journal-document-seeds");
    let mut e = engine_seeded(7);
    let project = e
        .run("project.new", json!({"path":work.path(),"name":"Board"}))
        .unwrap();
    e.run(
        "document.delete",
        json!({"document":project["documents"][1]["uuid"]}),
    )
    .unwrap();
    e.run(
        "batch",
        json!({"commands":[{"id":"document.add","params":{"kind":"pcb","layers":4}}]}),
    )
    .unwrap();
    let pcb = e.project().unwrap().pcb().unwrap();
    let layer = pcb.copper_layers().nth(2).unwrap().uuid;
    let sheet = pcb.stackup().dielectrics().next().unwrap().uuid;
    let through = pcb.stackup().through_type().uuid;
    let rule = pcb.board_rule(reda_engine::model::RuleKind::Clearance).uuid;
    e.run("pcb.layer.set", json!({"layer":layer,"name":"Reference"}))
        .unwrap();
    e.run(
        "pcb.stackup.set",
        json!({"dielectrics":[{"uuid":sheet,"material":"Glass"}]}),
    )
    .unwrap();
    e.run(
        "pcb.via_type.set",
        json!({"via_type":through,"name":"Mechanical"}),
    )
    .unwrap();
    e.run("object.get", json!({"uuid":rule})).unwrap();
    let (_copy, steps) = script(&journal::export(e.journal()));
    let mut replayed = Engine::builder().user_dirs(None).build();
    journal::replay(&mut replayed, &steps).unwrap();
    let pcb = replayed.project().unwrap().pcb().unwrap();
    assert_eq!(
        pcb.copper_layers().nth(2).unwrap().name.as_deref(),
        Some("Reference")
    );
    assert_eq!(
        pcb.stackup()
            .dielectrics()
            .next()
            .unwrap()
            .material
            .as_deref(),
        Some("Glass")
    );
    assert_eq!(
        pcb.stackup().through_type().name.as_deref(),
        Some("Mechanical")
    );
    assert_eq!(
        replayed.journal().last().unwrap().outcome.as_ref().unwrap()["object"]["kind"],
        "clearance"
    );
}

#[test]
fn old_and_new_preset_recordings_remap_each_created_inner_rule_by_kind() {
    let folder = TempDir::new("journal-created-rules");
    let mut e = engine_seeded(3);
    let project = e
        .run(
            "project.new",
            json!({"path":folder.path(),"name":"RuleSeeds","layers":4}),
        )
        .unwrap();
    let doc = e.project().unwrap().pcb().unwrap().uuid;
    let preset = e.project().unwrap().fab_presets().next().unwrap().uuid;
    e.run("fab_preset.set",json!({"preset":preset,"values":{"rules":{"inner":{"clearance":123456,"track_width_min":154000}}}})).unwrap();
    e.run("project.save", json!({})).unwrap();
    let before = e.journal().len();
    let applied = e
        .run(
            "batch",
            json!({"commands":[{"id":"pcb.rules.apply_fab_preset", "params":{"preset":preset}}]}),
        )
        .unwrap();
    let created = &applied["result"][0]["created_rules"];
    let kinds = ["clearance", "track_width"];
    for kind in kinds {
        e.run("object.get", json!({"uuid":created[kind]})).unwrap();
    }
    let current = journal::export(&e.journal()[before..]);
    let mut archived = current.clone();
    for kind in kinds {
        let old = doc.child(&format!("inner.{kind}"));
        archived = archived.replace(created[kind].as_str().unwrap(), &old.to_string());
    }
    let mut rows = lines(&archived);
    rows[0]["result"]["result"][0]
        .as_object_mut()
        .unwrap()
        .remove("created_rules");
    rows[0]["result"]["changes"][doc.to_string()]["created"]
        .as_array_mut()
        .unwrap()
        .sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    let archived = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    for recording in [archived, current] {
        let steps = journal::parse(&recording).unwrap();
        let mut replayed = engine_seeded(92);
        replayed
            .run("project.open", json!({"path":project["path"]}))
            .unwrap();
        journal::replay(&mut replayed, &steps).unwrap();
        let queries = replayed.journal().iter().filter(|e| e.id == "object.get");
        let observed: Vec<_> = queries
            .map(|e| {
                e.outcome.as_ref().unwrap()["object"]["kind"]
                    .as_str()
                    .unwrap()
            })
            .collect();
        assert_eq!(observed, kinds);
    }
}

#[test]
fn older_board_creation_recordings_accept_only_the_added_identity_metadata() {
    let folder = TempDir::new("old-board-creation");
    let mut e = engine_seeded(0);
    let project = e
        .run(
            "project.new",
            json!({"path":folder.path(),"name":"OldBoard"}),
        )
        .unwrap();
    let doc = &project["documents"][1]["uuid"];
    e.run(
        "object.get",
        json!({"uuid":project["documents"][0]["uuid"]}),
    )
    .unwrap();
    e.run("document.delete", json!({"document":doc})).unwrap();
    let direct = e.run("document.add", json!({"kind":"pcb"})).unwrap();
    e.run(
        "document.delete",
        json!({"document":direct["result"]["uuid"]}),
    )
    .unwrap();
    e.run(
        "batch",
        json!({"commands":[{"id":"document.add","params":{"kind":"pcb"}}]}),
    )
    .unwrap();
    let mut rows = lines(&journal::export(e.journal()));
    rows[0]["result"].as_object_mut().unwrap().remove("seeded");
    rows[3]["result"]["result"]
        .as_object_mut()
        .unwrap()
        .remove("seeded");
    rows[5]["result"]["result"][0]
        .as_object_mut()
        .unwrap()
        .remove("seeded");
    let export = |rows: &[Value]| {
        rows.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let (_copy, steps) = script(&export(&rows));
    journal::replay(&mut engine_seeded(70), &steps).unwrap();
    rows[5]["result"]["result"][0]
        .as_object_mut()
        .unwrap()
        .remove("title");
    let (_copy, malformed) = script(&export(&rows));
    let err = journal::replay(&mut engine_seeded(70), &malformed).unwrap_err();
    assert_eq!(err.pointer.as_deref(), Some("/result/0/title"));
}
