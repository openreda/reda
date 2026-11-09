//! Update from Library on the board: `pcb.footprint.update_preview` and
//! `pcb.footprint.update_from_library` update Mechanical Parts placed straight onto the PCB
//! by Footprint hash, keeping placement and leaving fields alone.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, starter_library, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const HOLE_PAD: &str = "1a21d727-dfca-41ae-9639-309d8abf4714";
const HOLE_PAD_FP: &str = "8cef86d0-5480-4597-8542-ac0c2cbd9774";
const HOLE_FILE: &str = "footprints/MountingHole_M3_Pad.redafp";
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";

struct Board {
    library: PathBuf,
    engine: TestEngine,
    _dirs: (TempDir, TempDir),
}

fn copy_dir(from: &Path, to: &Path) {
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

/// A Project on a writable copy of the Starter Library, with H1 (a plated mounting hole)
/// placed on the board and R1 synced from the schematic.
fn board(label: &str) -> (Board, Value) {
    let lib_dir = TempDir::new(&format!("{label}-lib"));
    let library = lib_dir.path().join("Starter");
    copy_dir(&starter_library(), &library);
    let dir = TempDir::new(label);
    let mut engine = test_engine_with_library(&library);
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": RESISTOR, "at": {"x": "0mm", "y": "0mm"}}),
    );
    let list = engine.run_ok("sync.preview", json!({}));
    engine.run_ok("sync.apply", json!({"hash": list["hash"]}));
    let h1 = engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": HOLE_PAD, "at": {"x": "20mm", "y": "10mm"}}),
    )["result"]["footprint"]
        .clone();
    let board = Board {
        library,
        engine,
        _dirs: (lib_dir, dir),
    };
    (board, h1)
}

impl Board {
    fn edit_library(&self, file: &str, from: &str, to: &str) {
        let path = self.library.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no {from:?}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
    }

    /// Widens the hole's Pad ring in the Library.
    fn widen_library_pad(&self) {
        self.edit_library(
            HOLE_FILE,
            r#""width":5500000,"height":5500000"#,
            r#""width":6000000,"height":6000000"#,
        );
    }

    fn preview(&mut self, params: Value) -> Value {
        self.engine.run_ok("pcb.footprint.update_preview", params)
    }

    /// Previews with `scope`, then applies that preview with `scope` and `extra`.
    fn apply_with(&mut self, scope: Value, extra: Value) -> Value {
        let mut params = scope.clone();
        let preview = self.preview(scope);
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        params["preview_hash"] = preview["preview_hash"].clone();
        self.engine
            .run_ok("pcb.footprint.update_from_library", params)
    }

    fn apply(&mut self, scope: Value) -> Value {
        self.apply_with(scope, json!({}))
    }

    fn object(&mut self, uuid: &Value) -> Value {
        self.engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
    }

    /// The Footprint copy an instance is placed from.
    fn copy_of(&mut self, instance: &Value) -> Value {
        let copy = self.object(instance)["footprint"].clone();
        self.object(&copy)
    }
}

#[test]
fn only_pcb_only_footprints_whose_library_footprint_differs_are_listed() {
    let (mut b, h1) = board("pcb-update-list");
    let preview = b.preview(json!({}));
    assert_eq!(preview["footprints"], json!([]), "everything is up to date");

    b.widen_library_pad();
    // The Resistor's Footprint changes too, but R1 updates through the schematic.
    b.edit_library(
        "footprints/RESC1005X40N.redafp",
        "Chip resistor, 0402",
        "Chip resistor 0402",
    );
    let preview = b.preview(json!({}));
    assert_eq!(
        preview["footprints"],
        json!([{
            "instance": h1,
            "designator": "H1",
            "footprint": HOLE_PAD_FP,
            "name": "MountingHole_M3_Pad",
            "status": "library_differs",
            "changes": [{"kind": "footprint", "footprint": HOLE_PAD_FP,
                         "name": "MountingHole_M3_Pad", "change": "modified",
                         "pads_resized": ["1"]}],
        }])
    );
    assert!(preview["preview_hash"].is_string());

    let r1 = b.engine.run_ok(
        "objects.query",
        json!({"type": "footprint_instance", "document": PCB}),
    )["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .find(|i| i["designator"] == "R1")
        .unwrap();
    let err = b.engine.run_err(
        "pcb.footprint.update_preview",
        json!({"footprints": [r1["uuid"]]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("schematic"), "{}", err.message);
}

#[test]
fn an_update_keeps_placement_and_leaves_fields_alone() {
    let (mut b, h1) = board("pcb-update-apply");
    b.engine.run_ok(
        "pcb.mechanical.set",
        json!({"footprints": [h1], "value": "M3 nylon", "fields": {"Note": "hand"},
               "in_bom": true}),
    );
    b.engine
        .run_ok("pcb.rotate", json!({"objects": [h1], "angle": "90deg"}));
    b.engine.run_ok("pcb.flip", json!({"objects": [h1]}));
    b.engine.run_ok(
        "pcb.designator_text.set",
        json!({"footprint": h1, "at": {"x": "25mm", "y": "3mm"}, "size": "1.5mm"}),
    );
    b.engine
        .run_ok("pcb.lock", json!({"objects": [h1], "locked": true}));
    let before = b.object(&h1);
    assert!(before.get("text_overrides").is_some(), "{before}");
    assert_eq!(
        (&before["side"], &before["locked"]),
        (&json!("bottom"), &json!(true))
    );
    let copy_before = b.copy_of(&h1);
    b.widen_library_pad();
    // A Library field change is not taken: fields are left alone.
    b.edit_library(
        "parts/MountingHole_M3_Pad.redapart",
        r#""value":"M3""#,
        r#""value":"M3 steel""#,
    );

    let done = b.apply(json!({}));

    assert_eq!(done["result"]["updated"], json!([h1]));
    let after = b.object(&h1);
    for key in [
        "uuid",
        "at",
        "rotation",
        "side",
        "locked",
        "designator",
        "fields",
        "component_type",
        "in_bom",
        "in_cpl",
        "not_fitted",
        "text_overrides",
        "pads",
    ] {
        assert_eq!(after[key], before[key], "{key} is kept");
    }
    let copy = b.copy_of(&h1);
    assert_ne!(copy["hash"], copy_before["hash"]);
    assert_eq!(copy["footprint"]["pads"][0]["width"], 6_000_000);
    assert_eq!(b.preview(json!({}))["footprints"], json!([]));
    // The old copy is gone, as no instance uses it.
    let gone = b
        .engine
        .run_err("object.get", json!({"uuid": copy_before["uuid"]}));
    assert_eq!(gone.code, ErrorCode::NotFound);

    b.engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(b.object(&h1), before, "one undo step");
}

#[test]
fn a_stale_preview_is_a_conflict_and_excluded_footprints_stay() {
    let (mut b, h1) = board("pcb-update-conflict");
    let h2 = b.engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": HOLE_PAD, "at": {"x": "40mm", "y": "10mm"}}),
    )["result"]["footprint"]
        .clone();
    b.widen_library_pad();
    let preview = b.preview(json!({}));
    assert_eq!(preview["footprints"].as_array().unwrap().len(), 2);
    b.edit_library(HOLE_FILE, r#""width":6000000"#, r#""width":6500000"#);
    let err = b.engine.run_err(
        "pcb.footprint.update_from_library",
        json!({"preview_hash": preview["preview_hash"]}),
    );
    assert_eq!(err.code, ErrorCode::Conflict);

    let before = b.object(&h2);
    let done = b.apply_with(json!({}), json!({"excluded": [h2]}));
    assert_eq!(done["result"]["updated"], json!([h1]));
    assert_eq!(done["result"]["not_updated"], json!([h2]));
    assert_eq!(b.object(&h2), before);

    // A scope of instances.
    let done = b.apply(json!({"footprints": [h2]}));
    assert_eq!(done["result"]["updated"], json!([h2]));
    assert_eq!(b.object(&h1)["footprint"], b.object(&h2)["footprint"]);
}

#[test]
fn a_footprint_no_library_holds_is_source_missing_and_not_updated() {
    let (mut b, h1) = board("pcb-update-missing");
    std::fs::remove_file(b.library.join(HOLE_FILE)).unwrap();
    let preview = b.preview(json!({}));
    let row = &preview["footprints"][0];
    assert_eq!(row["instance"], h1);
    assert_eq!(row["status"], "source_missing");
    assert!(
        row["reason"].as_str().unwrap().contains(HOLE_PAD_FP),
        "{row}"
    );
    let before = b.object(&h1);
    let done = b.apply(json!({}));
    assert_eq!(done["result"]["updated"], json!([]));
    assert_eq!(done["result"]["not_updated"], json!([h1]));
    assert_eq!(b.object(&h1), before);
}

#[test]
fn the_preview_lists_the_local_pad_edits_an_update_removes() {
    let (mut b, h1) = board("pcb-update-local");
    let pad = b.copy_of(&h1)["footprint"]["pads"][0].clone();
    b.engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": h1, "pad": pad["uuid"], "width": "5mm", "height": "5mm"}),
    );
    // A local edit alone is not out of date.
    assert_eq!(b.preview(json!({}))["footprints"], json!([]));

    b.widen_library_pad();
    let preview = b.preview(json!({}));
    assert_eq!(
        preview["footprints"][0]["lost_local_edits"],
        json!([
            {"pad": pad["uuid"], "number": "1", "property": "height",
             "original": 5_500_000, "edited": 5_000_000},
            {"pad": pad["uuid"], "number": "1", "property": "width",
             "original": 5_500_000, "edited": 5_000_000},
        ])
    );
    b.apply(json!({}));
    let copy = b.copy_of(&h1);
    assert!(copy.get("edited_from").is_none(), "{copy}");
    assert_eq!(copy["footprint"]["pads"][0]["width"], 6_000_000);
    assert_eq!(
        b.engine.run_ok("drc.run", json!({}))["locally_edited"],
        json!([])
    );
}
