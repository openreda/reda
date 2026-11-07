//! Locally Edited Footprints (ADR-0013): `pcb.pad.edit` gives a placed Footprint its own
//! part-table entry with `edited_from`, which Sync compares, and
//! `pcb.footprint.revert_local_edits` returns it to that copy.

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, starter_library, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
/// The Starter Library's Resistor, and its second Footprint alternate.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const SECOND_FOOTPRINT: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";

/// A Project with `count` 10k resistors synced to the board.
fn board(dir: &TempDir, count: i64) -> TestEngine {
    let mut engine = test_engine_with_library(&starter_library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    for i in 0..count {
        engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET, "part": RESISTOR,
                   "at": {"x": format!("{}mm", i * 10), "y": "0mm"}}),
        );
    }
    sync(&mut engine);
    engine
}

fn preview(engine: &mut TestEngine) -> Value {
    engine.run_ok("sync.preview", json!({}))
}

fn sync(engine: &mut TestEngine) -> Value {
    let list = preview(engine);
    engine.run_ok("sync.apply", json!({"hash": list["hash"]}))
}

fn query(engine: &mut TestEngine, kind: &str) -> Vec<Value> {
    engine.run_ok("objects.query", json!({"type": kind, "document": PCB}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

fn instance(engine: &mut TestEngine, designator: &str) -> Value {
    query(engine, "footprint_instance")
        .into_iter()
        .find(|i| i["designator"] == designator)
        .unwrap()
}

fn copy(engine: &mut TestEngine, uuid: &Value) -> Value {
    query(engine, "footprint_copy")
        .into_iter()
        .find(|c| c["uuid"] == *uuid)
        .unwrap()
}

/// The UUID of the instance's first Pad, and the Pad as its copy holds it.
fn first_pad(engine: &mut TestEngine, designator: &str) -> (Value, Value) {
    let i = instance(engine, designator);
    let c = copy(engine, &i["footprint"]);
    let pad = c["footprint"]["pads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["number"] == "1")
        .unwrap()
        .clone();
    (i["uuid"].clone(), pad)
}

fn widen(engine: &mut TestEngine, designator: &str) -> Value {
    let (instance, pad) = first_pad(engine, designator);
    engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": pad["uuid"], "width": "2mm"}),
    )
}

#[test]
fn editing_a_placed_pad_gives_the_instance_its_own_copy() {
    let dir = TempDir::new("local-edit");
    let mut engine = board(&dir, 1);
    let before = instance(&mut engine, "R1");
    let original = copy(&mut engine, &before["footprint"]);
    let (_, pad) = first_pad(&mut engine, "R1");

    widen(&mut engine, "R1");

    let after = instance(&mut engine, "R1");
    assert_ne!(after["footprint"], before["footprint"]);
    let edited = copy(&mut engine, &after["footprint"]);
    assert_eq!(
        edited["edited_from"],
        json!({"uuid": original["uuid"], "hash": original["hash"]})
    );
    assert_ne!(edited["hash"], original["hash"]);
    let widened = edited["footprint"]["pads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["uuid"] == pad["uuid"])
        .unwrap();
    assert_eq!(widened["width"], 2_000_000);
    assert_eq!(widened["number"], "1");
    // The original stays in the table, because the edited entry names it.
    assert_eq!(query(&mut engine, "footprint_copy").len(), 2);
    // A local edit is not out of sync.
    assert_eq!(preview(&mut engine)["count"], 0);
}

#[test]
fn a_placed_pad_keeps_its_number() {
    let dir = TempDir::new("local-edit-number");
    let mut engine = board(&dir, 1);
    let (instance, pad) = first_pad(&mut engine, "R1");
    let err = engine.run_err(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": pad["uuid"], "number": "7"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    // Nothing on the board adds or removes a Footprint's Pads.
    let catalogue = reda_engine::registry().catalogue();
    let ids: Vec<&str> = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["id"].as_str())
        .filter(|id| id.starts_with("pcb.pad."))
        .collect();
    assert_eq!(ids, ["pcb.pad.edit"]);
}

#[test]
fn identical_edits_share_one_entry_and_undoing_an_edit_returns_to_the_copy() {
    let dir = TempDir::new("local-edit-share");
    let mut engine = board(&dir, 2);
    let original = instance(&mut engine, "R1")["footprint"].clone();
    widen(&mut engine, "R1");
    widen(&mut engine, "R2");
    let r1 = instance(&mut engine, "R1")["footprint"].clone();
    assert_eq!(instance(&mut engine, "R2")["footprint"], r1);
    assert_eq!(query(&mut engine, "footprint_copy").len(), 2);

    // Narrowing R1's Pad back to the Library's width undoes its only edit.
    let (r1_instance, pad) = first_pad(&mut engine, "R1");
    let width = copy(&mut engine, &original)["footprint"]["pads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["uuid"] == pad["uuid"])
        .unwrap()["width"]
        .clone();
    engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": r1_instance, "pad": pad["uuid"], "width": width}),
    );
    assert_eq!(instance(&mut engine, "R1")["footprint"], original);

    // Once no instance uses the edited entry, it leaves the table.
    let r2 = instance(&mut engine, "R2")["uuid"].clone();
    let done = engine.run_ok(
        "pcb.footprint.revert_local_edits",
        json!({"instances": [r2, r1_instance]}),
    );
    assert_eq!(done["result"]["reverted"], json!([r2]));
    assert_eq!(instance(&mut engine, "R2")["footprint"], original);
    assert_eq!(query(&mut engine, "footprint_copy").len(), 1);
}

#[test]
fn an_edit_is_one_undo_step() {
    let dir = TempDir::new("local-edit-undo");
    let mut engine = board(&dir, 1);
    let original = instance(&mut engine, "R1")["footprint"].clone();
    widen(&mut engine, "R1");
    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(instance(&mut engine, "R1")["footprint"], original);
    assert_eq!(query(&mut engine, "footprint_copy").len(), 1);
}

fn changes(preview: &Value) -> Vec<Value> {
    preview["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["changes"].as_array().unwrap().clone())
        .collect()
}

#[test]
fn sync_keeps_local_edits_while_other_data_changes() {
    let dir = TempDir::new("local-edit-sync");
    let mut engine = board(&dir, 1);
    widen(&mut engine, "R1");
    let edited = instance(&mut engine, "R1")["footprint"].clone();
    let component = instance(&mut engine, "R1")["component"]["uuid"].clone();
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "value": "4k7"}),
    );
    let list = preview(&mut engine);
    assert_eq!(list["count"], 1);
    assert_eq!(changes(&list)[0]["kind"], "fields");
    sync(&mut engine);
    assert_eq!(instance(&mut engine, "R1")["footprint"], edited);
    assert_eq!(query(&mut engine, "footprint_copy").len(), 2);
}

#[test]
fn a_footprint_change_removes_local_edits_and_its_row_lists_them() {
    let dir = TempDir::new("local-edit-swap");
    let mut engine = board(&dir, 1);
    let (_, pad) = first_pad(&mut engine, "R1");
    widen(&mut engine, "R1");
    let component = instance(&mut engine, "R1")["component"]["uuid"].clone();
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "footprint": SECOND_FOOTPRINT}),
    );
    let list = preview(&mut engine);
    let swap = changes(&list)
        .into_iter()
        .find(|c| c["kind"] == "change_footprint")
        .unwrap();
    assert_eq!(
        swap["lost_local_edits"],
        json!([{"pad": pad["uuid"], "number": "1", "property": "width",
                "original": pad["width"], "edited": 2_000_000}])
    );

    sync(&mut engine);
    let after = instance(&mut engine, "R1");
    let now = copy(&mut engine, &after["footprint"]);
    assert_eq!(now["footprint"]["uuid"], SECOND_FOOTPRINT);
    assert!(now.get("edited_from").is_none());
    // Neither the edited entry nor the copy it came from is needed any more.
    assert_eq!(query(&mut engine, "footprint_copy").len(), 1);
    assert_eq!(preview(&mut engine)["count"], 0);
}

#[test]
fn the_drc_report_lists_locally_edited_footprints() {
    let dir = TempDir::new("local-edit-drc");
    let mut engine = board(&dir, 2);
    assert_eq!(
        engine.run_ok("drc.run", json!({}))["locally_edited"],
        json!([])
    );
    widen(&mut engine, "R2");
    let r2 = instance(&mut engine, "R2")["uuid"].clone();
    assert_eq!(
        engine.run_ok("drc.run", json!({}))["locally_edited"],
        json!([{"instance": r2, "designator": "R2"}])
    );
}

/// Saves the Project, rewrites its PCB file through `change` and opens it again.
fn reopen(
    engine: &mut TestEngine,
    dir: &TempDir,
    change: impl FnOnce(&mut Value),
) -> Result<TestEngine, String> {
    engine.run_ok("project.save", json!({}));
    let path = dir.path().join("Demo/Demo.redapcb");
    let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    change(&mut file);
    std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap()).unwrap();
    let mut other = test_engine_with_library(&starter_library());
    match other.run("project.open", json!({"path": dir.path().join("Demo")})) {
        Ok(_) => Ok(other),
        Err(err) => {
            assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
            Err(err.message)
        }
    }
}

#[test]
fn local_edits_survive_save_and_open() {
    let dir = TempDir::new("local-edit-save");
    let mut engine = board(&dir, 1);
    widen(&mut engine, "R1");
    let copies = query(&mut engine, "footprint_copy");
    let mut other = reopen(&mut engine, &dir, |_| {}).unwrap();
    assert_eq!(query(&mut other, "footprint_copy"), copies);
    assert_eq!(preview(&mut other)["count"], 0);
}

#[test]
fn edited_from_must_name_an_unedited_copy_the_board_holds() {
    let dir = TempDir::new("local-edit-load");
    let mut engine = board(&dir, 1);
    widen(&mut engine, "R1");
    let edited = |file: &mut Value| -> usize {
        file["footprint_copies"]
            .as_array()
            .unwrap()
            .iter()
            .position(|c| c.get("edited_from").is_some())
            .unwrap()
    };
    let err = reopen(&mut engine, &dir, |file| {
        let i = edited(file);
        file["footprint_copies"][i]["edited_from"]["uuid"] =
            json!("00000000-0000-4000-8000-0000000000ff");
    })
    .err()
    .unwrap();
    assert!(err.contains("edited from"), "{err}");

    let err = reopen(&mut engine, &dir, |file| {
        let i = edited(file);
        file["footprint_copies"][i]["edited_from"]["hash"] = json!("0".repeat(32));
    })
    .err()
    .unwrap();
    assert!(err.contains("edited from"), "{err}");
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Rewrites a file of the Library at `library`, replacing `from` with `to`.
fn edit_library(library: &std::path::Path, file: &str, from: &str, to: &str) {
    let path = library.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "{file} has no {from:?}");
    std::fs::write(&path, text.replace(from, to)).unwrap();
}

#[test]
fn the_update_preview_lists_the_local_edits_a_footprint_change_removes() {
    let lib_dir = TempDir::new("local-edit-update-lib");
    let library = lib_dir.path().join("Starter");
    copy_dir(&starter_library(), &library);
    let dir = TempDir::new("local-edit-update");
    let mut engine = test_engine_with_library(&library);
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    for x in ["0mm", "10mm"] {
        engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET, "part": RESISTOR, "at": {"x": x, "y": "0mm"}}),
        );
    }
    sync(&mut engine);
    widen(&mut engine, "R1");

    // A Fields-only update leaves the Footprint's hash alone, and so the local edits.
    edit_library(
        &library,
        "parts/Resistor.redapart",
        "\"description\":\"Resistor\"",
        "\"description\":\"A resistor\"",
    );
    let preview_update = |engine: &mut TestEngine| -> Vec<Value> {
        engine.run_ok("schematic.component.update_preview", json!({}))["components"]
            .as_array()
            .unwrap()
            .clone()
    };
    let rows = preview_update(&mut engine);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.get("lost_local_edits").is_none()));

    // A changed Footprint removes them, and the preview says so for R1 only.
    edit_library(
        &library,
        "footprints/RESC1005X40N.redafp",
        "Chip resistor, 0402",
        "Chip resistor 0402",
    );
    let rows = preview_update(&mut engine);
    let r1 = rows.iter().find(|r| r["designator"] == "R1").unwrap();
    let lost = r1["lost_local_edits"].as_array().unwrap();
    assert_eq!(lost.len(), 1);
    assert_eq!(lost[0]["property"], "width");
    let r2 = rows.iter().find(|r| r["designator"] == "R2").unwrap();
    assert!(r2.get("lost_local_edits").is_none());

    engine.run_ok("schematic.component.update_from_library", json!({}));
    sync(&mut engine);
    assert_eq!(
        engine.run_ok("drc.run", json!({}))["locally_edited"],
        json!([])
    );
}
