//! Update from Library: Components whose Library Part changed since they were placed are
//! marked, previewed and updated on request, and a placed Component leads back to its
//! Library elements for Edit in Library.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const OPAMP_UNIT_B: &str = "a0000000-0000-4000-8000-000000000032";
const R_SYMBOL: &str = "a0000000-0000-4000-8000-000000000010";
const R_0603: &str = "a0000000-0000-4000-8000-000000000040";

/// A writable copy of the fixture Library and a Project using it as its starter Library.
struct Fixture {
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

fn fixture(label: &str) -> Fixture {
    let lib_dir = TempDir::new(&format!("{label}-lib"));
    let library = lib_dir.path().join("Test");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test"),
        &library,
    );
    let project_dir = TempDir::new(label);
    let mut engine = test_engine_with_library(&library);
    engine.run_ok(
        "project.new",
        json!({"path": project_dir.path(), "name": "Demo"}),
    );
    Fixture {
        library,
        engine,
        _dirs: (lib_dir, project_dir),
    }
}

impl Fixture {
    fn place(&mut self, part: &str, x: &str) -> Value {
        self.engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET, "part": part, "at": {"x": x, "y": "0mm"}}),
        )["result"]
            .clone()
    }

    /// Rewrites a Library file, replacing `from` with `to`.
    fn edit_library(&self, file: &str, from: &str, to: &str) {
        let path = self.library.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no {from:?}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
    }

    fn outdated(&mut self) -> Vec<Value> {
        self.engine
            .run_ok("schematic.component.outdated", json!({}))["components"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn object(&mut self, uuid: &Value) -> Value {
        self.engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
    }
}

#[test]
fn a_component_is_marked_once_its_library_part_changes() {
    let mut f = fixture("outdated");
    let r1 = f.place(R_10K, "0mm");
    f.place(C_100N, "20mm");
    assert_eq!(f.outdated(), Vec::<Value>::new());

    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");

    let outdated = f.outdated();
    assert_eq!(outdated.len(), 1, "{outdated:?}");
    assert_eq!(outdated[0]["component"], r1["component"]["uuid"]);
    assert_eq!(outdated[0]["designator"], "R1");
    assert_eq!(outdated[0]["part"], R_10K);
    assert_ne!(outdated[0]["placed_hash"], outdated[0]["library_hash"]);
}

#[test]
fn the_preview_lists_what_would_change_and_changes_nothing() {
    let mut f = fixture("preview");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "fields": {"mpn": "MY-OWN-MPN"}}),
    );
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    f.edit_library("parts/R_10k.redapart", "RC0603FR-0710KL", "RC0603FR-07NEW");
    f.edit_library("footprints/R_0603.redafp", "950000", "1000000");
    let before = f.object(&r1["unit"]);

    let preview = f.engine.run_ok(
        "schematic.component.update_preview",
        json!({"components": [component]}),
    );

    let components = preview["components"].as_array().unwrap();
    assert_eq!(components.len(), 1);
    assert_eq!(components[0]["component"], component);
    assert_eq!(components[0]["designator"], "R1");
    assert_eq!(
        components[0]["changes"],
        json!([
            {"kind": "field", "name": "mpn", "from": "RC0603FR-0710KL", "to": "RC0603FR-07NEW", "edited": true},
            {"kind": "field", "name": "LCSC", "from": "C25804", "to": "C99999"},
            {"kind": "footprint", "footprint": R_0603, "name": "R_0603", "change": "modified"},
        ])
    );
    assert_eq!(f.object(&r1["unit"]), before);
    assert_eq!(f.outdated().len(), 1);
}

#[test]
fn update_from_library_applies_the_preview_as_one_undoable_edit_on_the_sheet() {
    let mut f = fixture("apply");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "fields": {"mpn": "MY-OWN-MPN"}}),
    );
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    f.edit_library("parts/R_10k.redapart", "RC0603FR-0710KL", "RC0603FR-07NEW");
    f.edit_library("footprints/R_0603.redafp", "950000", "1000000");
    let before = f.object(&r1["unit"]);
    let preview = f.engine.run_ok(
        "schematic.component.update_preview",
        json!({"components": [component]}),
    );

    let applied = f.engine.run_ok(
        "schematic.component.update_from_library",
        json!({"components": [component]}),
    );

    assert_eq!(applied["result"], preview);
    let changed: Vec<&String> = applied["changes"].as_object().unwrap().keys().collect();
    assert_eq!(
        changed,
        [SHEET],
        "only the sheet changes; Sync carries Footprints"
    );
    let unit = f.object(&r1["unit"]);
    assert_eq!(
        unit["fields"]["mpn"], "MY-OWN-MPN",
        "an edited field is kept"
    );
    assert_eq!(
        unit["fields"]["custom"],
        json!([{"name": "LCSC", "value": "C99999"}])
    );
    assert_ne!(unit["part"], before["part"]);
    assert_eq!(f.outdated(), Vec::<Value>::new());
    let deleted = &applied["changes"][SHEET]["deleted"];
    assert!(
        deleted.as_array().unwrap().contains(&before["part"]),
        "the unused old entry goes: {deleted}"
    );

    f.engine.run_ok("edit.undo", json!({"document": SHEET}));

    assert_eq!(f.object(&r1["unit"]), before);
    assert_eq!(f.outdated().len(), 1);
}

#[test]
fn an_update_keeps_entries_and_copies_other_components_still_use() {
    let mut f = fixture("apply-shared");
    let r1 = f.place(R_10K, "0mm");
    let r2 = f.place(R_10K, "10mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");

    f.engine.run_ok(
        "schematic.component.update_from_library",
        json!({"components": [r1["component"]["uuid"]]}),
    );

    let (r1_unit, r2_unit) = (f.object(&r1["unit"]), f.object(&r2["unit"]));
    assert_ne!(r1_unit["part"], r2_unit["part"]);
    let r2_entry = f.object(&r2_unit["part"]);
    assert_eq!(r2_entry["part"]["fields"]["custom"][0]["value"], "C25804");
    assert_eq!(f.outdated().len(), 1);
    assert_eq!(f.outdated()[0]["designator"], "R2");
}

#[test]
fn an_update_that_would_drop_a_placed_unit_is_refused() {
    let mut f = fixture("apply-blocked");
    let u1 = f.place(LM358, "0mm");
    let component = u1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "at": {"x": "20mm", "y": 0}}),
    );
    // Unit B and its Pin are the middle rows of their tables, so no commas need fixing.
    let path = f.library.join("symbols/OpAmp_Dual.redasym");
    let symbol = std::fs::read_to_string(&path).unwrap();
    let kept: Vec<&str> = symbol
        .lines()
        .filter(|l| !l.contains(OPAMP_UNIT_B))
        .collect();
    std::fs::write(&path, kept.join("\n")).unwrap();

    let preview = f
        .engine
        .run_ok("schematic.component.update_preview", json!({}));
    assert_eq!(preview["components"][0]["blocked"], true);
    assert!(
        preview["components"][0]["changes"]
            .as_array()
            .unwrap()
            .contains(&json!({"kind": "unit_removed", "unit": "B"}))
    );

    let err = f
        .engine
        .run_err("schematic.component.update_from_library", json!({}));
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn a_placed_component_leads_back_to_its_library_element_files() {
    let mut f = fixture("element");
    let r1 = f.place(R_10K, "0mm");

    let found = f.engine.run_ok(
        "schematic.component.library_element",
        json!({"component": r1["component"]["uuid"]}),
    );

    let library = f.library.canonicalize().unwrap();
    let path = |v: &Value| PathBuf::from(v.as_str().unwrap()).canonicalize().unwrap();
    assert_eq!(found["part"]["element"], R_10K);
    assert_eq!(
        path(&found["part"]["path"]),
        library.join("parts/R_10k.redapart")
    );
    assert_eq!(
        found["part"]["read_only"], true,
        "the starter Library is read-only"
    );
    assert_eq!(found["symbol"]["element"], R_SYMBOL);
    assert_eq!(
        path(&found["symbol"]["path"]),
        library.join("symbols/R.redasym")
    );
    assert_eq!(found["footprint"]["element"], R_0603);
    assert_eq!(
        path(&found["footprint"]["path"]),
        library.join("footprints/R_0603.redafp")
    );
    assert_eq!(found["part"]["library"], found["symbol"]["library"]);
}

#[test]
fn a_component_whose_part_left_the_library_has_no_element_to_edit() {
    let mut f = fixture("element-gone");
    let r1 = f.place(R_10K, "0mm");
    std::fs::remove_file(f.library.join("parts/R_10k.redapart")).unwrap();

    let err = f.engine.run_err(
        "schematic.component.library_element",
        json!({"component": r1["component"]["uuid"]}),
    );

    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(
        f.outdated(),
        Vec::<Value>::new(),
        "no Library Part, so no newer version"
    );
}

#[test]
fn an_update_reaches_units_on_every_sheet() {
    let mut f = fixture("apply-sheets");
    let u1 = f.place(LM358, "0mm");
    let added = f
        .engine
        .run_ok("document.add", json!({"kind": "schematic"}));
    let second = added["result"]["uuid"].clone();
    let b = f.engine.run_ok(
        "schematic.unit.place",
        json!({"document": second, "component": u1["component"]["uuid"], "at": {"x": 0, "y": 0}}),
    )["result"]
        .clone();
    f.edit_library(
        "parts/LM358.redapart",
        "\"value\":\"LM358\"",
        "\"value\":\"LM358B\"",
    );

    f.engine
        .run_ok("schematic.component.update_from_library", json!({}));

    assert_eq!(f.object(&u1["unit"])["fields"]["value"], "LM358B");
    assert_eq!(f.object(&b["unit"])["fields"]["value"], "LM358B");
    assert_eq!(f.outdated(), Vec::<Value>::new());
}

#[test]
fn the_preview_defaults_to_every_outdated_component() {
    let mut f = fixture("preview-all");
    f.place(R_10K, "0mm");
    f.place(R_10K, "10mm");
    f.place(C_100N, "20mm");
    f.edit_library(
        "symbols/R.redasym",
        "\"name\":\"R\"",
        "\"name\":\"Resistor\"",
    );

    let preview = f
        .engine
        .run_ok("schematic.component.update_preview", json!({}));

    let designators: Vec<&Value> = preview["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["designator"])
        .collect();
    assert_eq!(designators, [&json!("R1"), &json!("R2")]);
    assert_eq!(
        preview["components"][0]["changes"],
        json!([{"kind": "symbol", "name": "Resistor"}])
    );
}

#[test]
fn a_locked_field_blocks_updates_but_can_still_be_edited_by_hand() {
    let mut f = fixture("locked-field");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "locked": {"LCSC": true}}),
    );
    assert_eq!(f.object(&r1["unit"])["locked_fields"], json!(["LCSC"]));
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    f.edit_library("parts/R_10k.redapart", "RC0603FR-0710KL", "RC0603FR-07NEW");

    let preview = f.engine.run_ok(
        "schematic.component.update_preview",
        json!({"components": [component]}),
    );
    assert_eq!(
        preview["components"][0]["changes"],
        json!([
            {"kind": "field", "name": "mpn", "from": "RC0603FR-0710KL", "to": "RC0603FR-07NEW"},
            {"kind": "field", "name": "LCSC", "from": "C25804", "to": "C99999", "locked": true},
        ])
    );
    f.engine.run_ok(
        "schematic.component.update_from_library",
        json!({"components": [component]}),
    );
    let unit = f.object(&r1["unit"]);
    assert_eq!(unit["fields"]["mpn"], "RC0603FR-07NEW");
    assert_eq!(
        unit["fields"]["custom"],
        json!([{"name": "LCSC", "value": "C25804"}]),
        "the locked field keeps its value"
    );

    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "fields": {"LCSC": "C12345"}}),
    );
    assert_eq!(
        f.object(&r1["unit"])["fields"]["custom"],
        json!([{"name": "LCSC", "value": "C12345"}]),
        "a lock does not stop a hand edit"
    );
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "locked": {"LCSC": false}}),
    );
    assert!(f.object(&r1["unit"]).get("locked_fields").is_none());
}

#[test]
fn locked_fields_are_kept_equal_on_every_unit() {
    let mut f = fixture("locked-units");
    let u1 = f.place(LM358, "0mm");
    let component = u1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "locked": {"mpn": true}}),
    );
    let b = f.engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "at": {"x": "30mm", "y": 0}}),
    );
    assert_eq!(
        f.object(&b["result"]["unit"])["locked_fields"],
        json!(["mpn"])
    );
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "locked": {"value": true}}),
    );
    for unit in [&u1["unit"], &b["result"]["unit"]] {
        assert_eq!(f.object(unit)["locked_fields"], json!(["mpn", "value"]));
    }
}
