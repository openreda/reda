//! Update from Library: Components whose Library Part changed since they were placed are
//! marked, previewed and updated on request, and a placed Component leads back to its
//! Library elements for Edit in Library.

mod update_fixture;

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use serde_json::{Value, json};
use update_fixture::{Fixture, SHEET, fixture};
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const OPAMP_UNIT_B: &str = "a0000000-0000-4000-8000-000000000032";
const R_SYMBOL: &str = "a0000000-0000-4000-8000-000000000010";
const R_0603: &str = "a0000000-0000-4000-8000-000000000040";
const R_0805: &str = "a0000000-0000-4000-8000-000000000050";
const R_2512: &str = "a0000000-0000-4000-8000-000000000070";

#[test]
fn library_status_names_the_aspects_that_differ_and_reports_source_missing() {
    let mut f = fixture("status");
    let r1 = f.place(R_10K, "0mm");
    let c1 = f.place(C_100N, "20mm");
    let u1 = f.place(LM358, "40mm");
    let status = f.status();
    let rows: Vec<(&Value, &Value)> = status
        .iter()
        .map(|c| (&c["designator"], &c["status"]))
        .collect();
    assert_eq!(
        rows,
        [
            (&json!("C1"), &json!("up_to_date")),
            (&json!("R1"), &json!("up_to_date")),
            (&json!("U1"), &json!("up_to_date")),
        ]
    );

    // R: Fields and Symbol; C (another Symbol) stays Up to Date.
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    f.edit_library(
        "symbols/R.redasym",
        "\"name\":\"R\"",
        "\"name\":\"Resistor\"",
    );
    // LM358's Part is gone from the Library.
    std::fs::remove_file(f.library.join("parts/LM358.redapart")).unwrap();

    let status = f.status();
    let by = |c: &Value| {
        status
            .iter()
            .find(|s| s["component"] == c["component"]["uuid"])
            .unwrap()
            .clone()
    };
    let r = by(&r1);
    assert_eq!(r["status"], "library_differs");
    assert_eq!(r["aspects"], json!(["symbol", "fields"]));
    assert_eq!(r["part"], R_10K);
    assert_eq!(by(&c1)["status"], "up_to_date");
    let u = by(&u1);
    assert_eq!(u["status"], "source_missing", "{u}");
    assert!(u.get("aspects").is_none());
    assert!(
        u["reason"].as_str().unwrap().contains("LM358")
            || u["reason"].as_str().unwrap().contains(LM358),
        "{u}"
    );

    let only = f.engine.run_ok(
        "schematic.component.library_status",
        json!({"components": [r1["component"]["uuid"]]}),
    )["components"]
        .clone();
    assert_eq!(only.as_array().unwrap().len(), 1);
}

#[test]
fn a_footprint_change_is_the_footprints_aspect() {
    let mut f = fixture("status-fp");
    f.place(C_100N, "0mm");
    f.edit_library("footprints/R_0603.redafp", "950000", "1000000");
    assert_eq!(f.status()[0]["aspects"], json!(["footprints"]));
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
    assert_eq!(outdated[0]["aspects"], json!(["fields"]));
}

/// The `cell` named `name` of a preview row.
fn cell<'a>(row: &'a Value, name: &str) -> &'a Value {
    row["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cell"] == name)
        .unwrap_or_else(|| panic!("no cell {name} in {row}"))
}

/// The cells of a preview row as (cell, case, action).
fn cells(row: &Value) -> Vec<(String, String, String)> {
    let s = |v: &Value| v.as_str().unwrap().to_owned();
    row["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (s(&c["cell"]), s(&c["case"]), s(&c["action"])))
        .collect()
}

fn cell_row(cell: &str, case: &str, action: &str) -> (String, String, String) {
    (cell.to_owned(), case.to_owned(), action.to_owned())
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

    let preview = f.preview(json!({"components": [component]}));

    let components = preview["components"].as_array().unwrap();
    assert_eq!(components.len(), 1);
    let row = &components[0];
    assert_eq!(row["component"], component);
    assert_eq!(row["designator"], "R1");
    assert_eq!(row["status"], "library_differs");
    assert_eq!(row["differs"], json!(["fields", "footprints"]));
    assert_eq!(
        row["aspects"],
        json!(["fields", "footprints"]),
        "Full Replace"
    );
    assert_eq!(
        row["changes"],
        json!([
            {"kind": "footprint", "footprint": R_0603, "name": "R_0603", "change": "modified",
             "pads_resized": ["1", "2"]},
        ])
    );
    assert_eq!(
        row["cells"],
        json!([
            {"cell": "mpn", "case": "edited", "old": "RC0603FR-0710KL", "new": "RC0603FR-07NEW",
             "component": "MY-OWN-MPN", "action": "reject"},
            {"cell": "LCSC", "case": "changed", "old": "C25804", "new": "C99999",
             "component": "C25804", "action": "update"},
        ])
    );
    assert_eq!(preview["settings"]["locally_edited"], "keep");
    assert!(
        preview["preview_hash"]
            .as_str()
            .is_some_and(|h| !h.is_empty())
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

    let applied = f.apply(json!({"components": [component]}), json!({}));

    assert_eq!(applied["result"]["updated"], json!([component]));
    let changed: Vec<&String> = applied["changes"].as_object().unwrap().keys().collect();
    assert_eq!(
        changed,
        [SHEET],
        "only the sheet changes; Sync carries Footprints"
    );
    let unit = f.object(&r1["unit"]);
    assert_eq!(
        unit["fields"]["mpn"], "MY-OWN-MPN",
        "a rejected cell keeps its value"
    );
    assert_eq!(
        unit["fields"]["custom"],
        json!([{"name": "LCSC", "value": "C99999"}])
    );
    assert_ne!(unit["part"], before["part"]);
    assert_eq!(
        f.outdated(),
        Vec::<Value>::new(),
        "the baseline moved, so the rejected value reads as edited"
    );
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
fn an_update_fails_with_conflict_when_the_preview_is_stale() {
    let mut f = fixture("conflict");
    f.place(R_10K, "0mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    let preview = f.preview(json!({}));
    f.edit_library("parts/R_10k.redapart", "C99999", "C11111");

    let err = f.engine.run_err(
        "schematic.component.update_from_library",
        json!({"preview_hash": preview["preview_hash"]}),
    );

    assert_eq!(err.code, ErrorCode::Conflict);
    let err = f.engine.run_err(
        "schematic.component.update_from_library",
        json!({"preview_hash": preview["preview_hash"], "aspects": ["symbol"]}),
    );
    assert_eq!(
        err.code,
        ErrorCode::Conflict,
        "other parameters than previewed"
    );
}

#[test]
fn an_update_keeps_entries_and_copies_other_components_still_use() {
    let mut f = fixture("apply-shared");
    let r1 = f.place(R_10K, "0mm");
    let r2 = f.place(R_10K, "10mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");

    f.apply(json!({"components": [r1["component"]["uuid"]]}), json!({}));

    let (r1_unit, r2_unit) = (f.object(&r1["unit"]), f.object(&r2["unit"]));
    assert_ne!(r1_unit["part"], r2_unit["part"]);
    let r2_entry = f.object(&r2_unit["part"]);
    assert_eq!(r2_entry["part"]["fields"]["custom"][0]["value"], "C25804");
    assert_eq!(f.outdated().len(), 1);
    assert_eq!(f.outdated()[0]["designator"], "R2");
}

#[test]
fn excluded_components_stay_as_they_are() {
    let mut f = fixture("exclude");
    let r1 = f.place(R_10K, "0mm");
    let r2 = f.place(R_10K, "10mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");

    let applied = f.apply(
        json!({}),
        json!({"excluded_components": [r2["component"]["uuid"]]}),
    );

    assert_eq!(
        applied["result"]["updated"],
        json!([r1["component"]["uuid"]])
    );
    assert_eq!(
        applied["result"]["not_updated"],
        json!([r2["component"]["uuid"]])
    );
    let designators: Vec<Value> = f
        .outdated()
        .iter()
        .map(|c| c["designator"].clone())
        .collect();
    assert_eq!(designators, [json!("R2")]);
}

/// R_10k's fields and In CPL as a newer Library has them.
fn change_r10k_fields(f: &Fixture) {
    f.edit_library(
        "parts/R_10k.redapart",
        r#""fields": {"value":"10k","mpn":"RC0603FR-0710KL","custom":[{"name":"LCSC","value":"C25804"}]}"#,
        r#""fields": {"value":"22k","manufacturer":"Yageo","mpn":"RC0603FR-07NEW","custom":[{"name":"DigiKey","value":"311-10K"}]}"#,
    );
    f.edit_library(
        "parts/R_10k.redapart",
        r#""in_cpl":true"#,
        r#""in_cpl":false"#,
    );
}

#[test]
fn field_cells_start_from_the_three_way_compare_and_can_be_overridden() {
    let mut f = fixture("cells");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "value": "10k 1%", "fields": {"Note": "hand"}}),
    );
    change_r10k_fields(&f);

    let preview = f.preview(json!({}));

    let row = &preview["components"][0];
    assert_eq!(
        cells(row),
        [
            cell_row("value", "edited", "reject"),
            cell_row("manufacturer", "added", "update"),
            cell_row("mpn", "changed", "update"),
            cell_row("LCSC", "removed", "update"),
            cell_row("DigiKey", "added", "update"),
            cell_row("Note", "component_only", "reject"),
            cell_row("in_cpl", "changed", "update"),
        ]
    );
    assert_eq!(
        cell(row, "in_cpl"),
        &json!({"cell": "in_cpl", "case": "changed", "old": "true", "new": "false",
                "component": "true", "action": "update"})
    );

    f.apply(
        json!({}),
        json!({
            "excluded_cells": [{"component": component, "cell": "DigiKey"}],
            "updated_cells": [{"component": component, "cell": "value"}],
        }),
    );

    let unit = f.object(&r1["unit"]);
    assert_eq!(unit["fields"]["value"], "22k", "updated by override");
    assert_eq!(unit["fields"]["manufacturer"], "Yageo");
    assert_eq!(unit["fields"]["mpn"], "RC0603FR-07NEW");
    assert_eq!(
        unit["fields"]["custom"],
        json!([{"name": "Note", "value": "hand"}]),
        "LCSC removed, DigiKey rejected, Note kept"
    );
    assert_eq!(unit["in_cpl"], false);
    assert_eq!(
        f.outdated(),
        Vec::<Value>::new(),
        "every default moved, whichever way each cell went"
    );
}

#[test]
fn an_unknown_cell_is_invalid() {
    let mut f = fixture("cells-unknown");
    let r1 = f.place(R_10K, "0mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    let preview = f.preview(json!({}));
    let err = f.engine.run_err(
        "schematic.component.update_from_library",
        json!({"preview_hash": preview["preview_hash"],
               "excluded_cells": [{"component": r1["component"]["uuid"], "cell": "nope"}]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/excluded_cells/0");
}

#[test]
fn settings_change_how_cells_start_and_the_ones_used_are_saved_in_the_project() {
    let mut f = fixture("settings");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "value": "10k 1%", "fields": {"Note": "hand"}}),
    );
    change_r10k_fields(&f);
    let settings = json!({
        "locally_edited": "take_new",
        "library_field_missing": "skip",
        "component_field_missing": "remove",
        "fields_to_update": ["value", "manufacturer", "DigiKey", "Note", "LCSC"],
    });

    let preview = f.preview(json!({"settings": settings}));

    assert_eq!(
        cells(&preview["components"][0]),
        [
            cell_row("value", "edited", "update"),
            cell_row("manufacturer", "added", "reject"),
            cell_row("mpn", "changed", "reject"),
            cell_row("LCSC", "removed", "update"),
            cell_row("DigiKey", "added", "reject"),
            cell_row("Note", "component_only", "update"),
            cell_row("in_cpl", "changed", "reject"),
        ]
    );
    assert_eq!(f.preview(json!({}))["settings"]["locally_edited"], "keep");

    f.apply(json!({"settings": settings}), json!({}));

    let unit = f.object(&r1["unit"]);
    assert_eq!(unit["fields"]["value"], "22k");
    assert!(unit["fields"].get("custom").is_none(), "{unit}");
    let saved = f.preview(json!({}))["settings"].clone();
    assert_eq!(saved["locally_edited"], "take_new");
    assert_eq!(saved["fields_to_update"][0], "value");
    f.engine.run_ok("project.save", json!({}));
    let reda = find_reda(&f.project);
    assert!(
        reda.contains(r#""library_update""#) && reda.contains(r#""take_new""#),
        "{reda}"
    );
}

/// The text of the `.reda` file under `dir`.
fn find_reda(dir: &Path) -> String {
    fn find(dir: &Path) -> Option<PathBuf> {
        std::fs::read_dir(dir).ok()?.find_map(|entry| {
            let path = entry.ok()?.path();
            if path.is_dir() {
                find(&path)
            } else {
                path.extension()
                    .is_some_and(|e| e == "reda")
                    .then_some(path)
            }
        })
    }
    let path = find(dir).unwrap_or_else(|| panic!("no .reda file in {}", dir.display()));
    std::fs::read_to_string(path).unwrap()
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

    let preview = f.preview(json!({"components": [component]}));
    assert_eq!(
        cells(&preview["components"][0]),
        [
            cell_row("mpn", "changed", "update"),
            cell_row("LCSC", "locked", "none"),
        ]
    );
    let err = f.engine.run_err(
        "schematic.component.update_from_library",
        json!({"preview_hash": preview["preview_hash"], "components": [component],
               "updated_cells": [{"component": component, "cell": "LCSC"}]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "no override until unlocked");

    f.apply(json!({"components": [component]}), json!({}));
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
fn becoming_a_net_tie_clears_not_fitted_in_a_visible_row() {
    let mut f = fixture("net-tie");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "not_fitted": true}),
    );
    f.edit_library(
        "parts/R_10k.redapart",
        r#""component_type":"standard","in_bom":true,"in_cpl":true"#,
        r#""component_type":"net_tie","in_bom":false,"in_cpl":false"#,
    );

    let preview = f.preview(json!({}));

    let row = &preview["components"][0];
    assert_eq!(
        cells(row),
        [
            cell_row("component_type", "changed", "update"),
            cell_row("in_bom", "changed", "update"),
            cell_row("in_cpl", "changed", "update"),
            cell_row("not_fitted", "cleared", "update"),
        ]
    );
    assert_eq!(cell(row, "component_type")["new"], "net_tie");
    let err = f.engine.run_err(
        "schematic.component.update_from_library",
        json!({"preview_hash": preview["preview_hash"],
               "excluded_cells": [{"component": component, "cell": "not_fitted"}]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);

    f.apply(json!({}), json!({}));

    let unit = f.object(&r1["unit"]);
    assert_eq!(unit["component_type"], "net_tie");
    assert_eq!(unit["in_bom"], false);
    assert!(unit.get("not_fitted").is_none(), "{unit}");
}

#[test]
fn not_fitted_is_never_updated_otherwise() {
    let mut f = fixture("not-fitted");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "not_fitted": true}),
    );
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    f.apply(json!({}), json!({}));
    assert_eq!(f.object(&r1["unit"])["not_fitted"], true);
}

#[test]
fn a_partial_update_writes_a_mixed_entry_that_stays_library_differs_for_the_rest() {
    let mut f = fixture("mixed");
    let r1 = f.place(R_10K, "0mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    f.edit_library("symbols/R.redasym", r#""name":"R""#, r#""name":"Resistor""#);

    let preview = f.preview(json!({"aspects": ["fields"]}));
    assert_eq!(
        preview["components"][0]["differs"],
        json!(["symbol", "fields"])
    );
    assert_eq!(preview["components"][0]["aspects"], json!(["fields"]));
    assert!(
        preview["components"][0]["changes"]
            .as_array()
            .unwrap()
            .is_empty(),
        "no Symbol change shows when the Symbol is not taken"
    );
    f.apply(json!({"aspects": ["fields"]}), json!({}));

    let unit = f.object(&r1["unit"]);
    let entry = f.object(&unit["part"]);
    assert_eq!(entry["part"]["fields"]["custom"][0]["value"], "C99999");
    assert_eq!(entry["symbol"]["name"], "R", "the Symbol was not taken");
    assert!(
        entry["hash"].as_str().unwrap().starts_with("mixed-"),
        "{entry}"
    );
    let status = f.status();
    assert_eq!(status[0]["status"], "library_differs");
    assert_eq!(status[0]["aspects"], json!(["symbol"]));

    f.apply(json!({"aspects": ["symbol"]}), json!({}));

    let unit = f.object(&r1["unit"]);
    let entry = f.object(&unit["part"]);
    assert_eq!(entry["symbol"]["name"], "Resistor");
    assert!(!entry["hash"].as_str().unwrap().starts_with("mixed-"));
    assert_eq!(f.status()[0]["status"], "up_to_date");
}

#[test]
fn each_component_can_take_its_own_aspects() {
    let mut f = fixture("component-aspects");
    let r1 = f.place(R_10K, "0mm");
    let r2 = f.place(R_10K, "10mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    let scope = json!({"component_aspects": [
        {"component": r2["component"]["uuid"], "aspects": []},
    ]});

    let preview = f.preview(scope.clone());
    assert_eq!(preview["components"][1]["aspects"], json!([]));
    let applied = f.apply(scope, json!({}));

    assert_eq!(
        applied["result"]["updated"],
        json!([r1["component"]["uuid"]])
    );
    assert_eq!(f.outdated()[0]["designator"], "R2");
}

#[test]
fn the_scope_can_be_sheets_and_parts() {
    let mut f = fixture("scope");
    f.place(R_10K, "0mm");
    f.place(C_100N, "10mm");
    let added = f
        .engine
        .run_ok("document.add", json!({"kind": "schematic"}));
    let second = added["result"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.place",
        json!({"document": second, "part": R_10K, "at": {"x": 0, "y": 0}}),
    );
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    f.edit_library(
        "parts/C_100n.redapart",
        r#""value":"100n""#,
        r#""value":"100nF""#,
    );
    let designators = |preview: &Value| -> Vec<Value> {
        preview["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["designator"].clone())
            .collect()
    };

    assert_eq!(
        designators(&f.preview(json!({}))),
        [json!("C1"), json!("R1"), json!("R2")]
    );
    assert_eq!(
        designators(&f.preview(json!({"sheets": [second]}))),
        [json!("R2")]
    );
    assert_eq!(
        designators(&f.preview(json!({"parts": [R_10K]}))),
        [json!("R1"), json!("R2")]
    );
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
    assert_eq!(f.status()[0]["status"], "source_missing");
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

    let applied = f.apply(json!({}), json!({}));

    assert_eq!(f.object(&u1["unit"])["fields"]["value"], "LM358B");
    assert_eq!(f.object(&b["unit"])["fields"]["value"], "LM358B");
    assert_eq!(f.outdated(), Vec::<Value>::new());
    let mut sheets: Vec<&String> = applied["changes"].as_object().unwrap().keys().collect();
    sheets.sort();
    assert_eq!(sheets.len(), 2, "one undo entry per sheet: {sheets:?}");
}

#[test]
fn the_preview_defaults_to_every_component_not_up_to_date() {
    let mut f = fixture("preview-all");
    f.place(R_10K, "0mm");
    f.place(R_10K, "10mm");
    let c1 = f.place(C_100N, "20mm");
    f.place(LM358, "40mm");
    f.edit_library(
        "symbols/R.redasym",
        "\"name\":\"R\"",
        "\"name\":\"Resistor\"",
    );
    std::fs::remove_file(f.library.join("parts/C_100n.redapart")).unwrap();

    let preview = f.preview(json!({}));

    let rows: Vec<(&Value, &Value)> = preview["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (&c["designator"], &c["status"]))
        .collect();
    assert_eq!(
        rows,
        [
            (&json!("C1"), &json!("source_missing")),
            (&json!("R1"), &json!("library_differs")),
            (&json!("R2"), &json!("library_differs")),
        ]
    );
    assert_eq!(
        preview["components"][1]["changes"],
        json!([{"kind": "symbol", "name": "Resistor"}])
    );

    let applied = f.apply(json!({}), json!({}));
    assert_eq!(
        applied["result"]["not_updated"],
        json!([c1["component"]["uuid"]])
    );
    assert_eq!(f.status()[0]["status"], "source_missing");
}

#[test]
fn a_placed_unit_gone_from_the_symbol_blocks_that_component() {
    let mut f = fixture("apply-blocked");
    let u1 = f.place(LM358, "0mm");
    let component = u1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "at": {"x": "20mm", "y": 0}}),
    );
    f.place(R_10K, "40mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    // Unit B and its Pin are the middle rows of their tables, so no commas need fixing.
    let path = f.library.join("symbols/OpAmp_Dual.redasym");
    let symbol = std::fs::read_to_string(&path).unwrap();
    let kept: Vec<&str> = symbol
        .lines()
        .filter(|l| !l.contains(OPAMP_UNIT_B))
        .collect();
    std::fs::write(&path, kept.join("\n")).unwrap();

    let preview = f.preview(json!({}));
    let row = &preview["components"][1];
    assert_eq!(row["designator"], "U1");
    assert_eq!(row["blocked"], true);
    assert_eq!(
        row["problems"],
        json!([{"kind": "placed_unit_removed", "unit": "B"}])
    );
    assert!(
        row["changes"]
            .as_array()
            .unwrap()
            .contains(&json!({"kind": "unit_removed", "unit": "B", "placed": true})),
        "{row}"
    );

    let err = f.engine.run_err(
        "schematic.component.update_from_library",
        json!({"preview_hash": preview["preview_hash"]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("U1"), "{}", err.message);

    let applied = f.apply(json!({}), json!({"excluded_components": [component]}));
    assert_eq!(
        applied["result"]["updated"].as_array().unwrap().len(),
        1,
        "R1 updates"
    );
}

#[test]
fn a_new_unit_appears_as_unplaced() {
    let mut f = fixture("unit-added");
    f.place(LM358, "0mm");
    f.edit_library(
        "symbols/OpAmp_Dual.redasym",
        r#"{"uuid":"a0000000-0000-4000-8000-000000000033","name":"Power"}"#,
        r#"{"uuid":"a0000000-0000-4000-8000-000000000033","name":"Power"},
    {"uuid":"a0000000-0000-4000-8000-00000000003f","name":"C"}"#,
    );

    let preview = f.preview(json!({}));

    assert_eq!(
        preview["components"][0]["changes"],
        json!([
            {"kind": "symbol", "name": "OpAmp_Dual"},
            {"kind": "unit_added", "unit": "C"},
        ])
    );
    assert!(preview["components"][0].get("blocked").is_none());
}

#[test]
fn pin_changes_name_the_nets_they_leave_and_wires_never_move() {
    let mut f = fixture("pins");
    let r1 = f.place(R_10K, "0mm");
    // Pin 1 of R sits at (0, 3.81 mm): a Wire runs right from it to a Net Label.
    let wire = f.engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [{"x": 0, "y": "3.81mm"}, {"x": "10.16mm", "y": "3.81mm"}]}),
    )["result"]["wires"][0]
        .clone();
    f.engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SIG", "at": {"x": "10.16mm", "y": "3.81mm"}}),
    );
    let wire_before = f.object(&wire);
    f.edit_library(
        "symbols/R.redasym",
        r#""number":"1","electrical_type":"passive","at":{"x":0,"y":3810000}"#,
        r#""number":"1","electrical_type":"passive","at":{"x":0,"y":5080000}"#,
    );
    f.edit_library(
        "symbols/R.redasym",
        r#""name":"~","number":"2","electrical_type":"passive""#,
        r#""name":"B","number":"2","electrical_type":"input""#,
    );
    f.edit_library(
        "symbols/R.redasym",
        r#""rotation":90000,"length":1270000}"#,
        r#""rotation":90000,"length":1270000},
    {"uuid":"a0000000-0000-4000-8000-00000000001f","unit":"a0000000-0000-4000-8000-000000000011","name":"S","number":"3","electrical_type":"passive","at":{"x":2540000,"y":0},"rotation":180000,"length":1270000}"#,
    );

    let preview = f.preview(json!({}));

    let row = &preview["components"][0];
    assert_eq!(
        row["changes"],
        json!([
            {"kind": "symbol", "name": "R"},
            {"kind": "pin", "unit": "A", "number": "1", "moved": true, "leaves": "SIG"},
            {"kind": "pin", "unit": "A", "number": "2", "name": {"from": "~", "to": "B"},
             "electrical_type": {"from": "passive", "to": "input"}},
            {"kind": "pin", "unit": "A", "number": "3", "added": true},
        ])
    );
    assert_eq!(
        row["problems"],
        json!([{"kind": "pin_without_pad", "unit": "A", "number": "3"}]),
        "R_0603 has no Pad 3"
    );
    assert!(row.get("blocked").is_none(), "a problem row does not block");

    let applied = f.apply(json!({}), json!({}));

    assert_eq!(f.object(&wire), wire_before, "Wires never move");
    let sheet = &applied["changes"][SHEET];
    for list in ["created", "modified", "deleted"] {
        for uuid in sheet[list].as_array().into_iter().flatten() {
            assert_ne!(uuid, &wire, "the Wire is not touched");
        }
    }
    assert_eq!(f.status()[0]["status"], "up_to_date");
    let _ = r1;
}

#[test]
fn footprint_changes_list_alternates_the_pin_map_and_the_footprint_choice() {
    let mut f = fixture("footprints");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "footprint": R_0805}),
    );
    f.edit_library(
        "parts/R_10k.redapart",
        "\"a0000000-0000-4000-8000-000000000050\"",
        "\"a0000000-0000-4000-8000-000000000070\"",
    );
    f.edit_library(
        "parts/R_10k.redapart",
        r#""pin_map": {}"#,
        r#""pin_map": {"unconnected":["3"]}"#,
    );

    let preview = f.preview(json!({}));

    assert_eq!(
        preview["components"][0]["changes"],
        json!([
            {"kind": "footprint", "footprint": R_2512, "name": "R_2512_Kelvin", "change": "added"},
            {"kind": "footprint", "footprint": R_0805, "name": "R_0805", "change": "removed"},
            {"kind": "pin_map", "from": {}, "to": {"unconnected": ["3"]}},
            {"kind": "footprint_choice", "from": R_0805, "to": R_0603},
        ])
    );

    let keep =
        json!({"settings": {"add_new_footprints": false, "remove_missing_footprints": false}});
    let preview = f.preview(keep.clone());
    assert_eq!(
        preview["components"][0]["changes"],
        json!([{"kind": "pin_map", "from": {}, "to": {"unconnected": ["3"]}}])
    );
    f.apply(keep, json!({}));
    let unit = f.object(&r1["unit"]);
    assert_eq!(unit["footprint"], R_0805);
    let entry = f.object(&unit["part"]);
    assert_eq!(entry["part"]["footprints"], json!([R_0603, R_0805]));
    assert_eq!(entry["part"]["pin_map"], json!({"unconnected": ["3"]}));
    assert_eq!(f.status()[0]["aspects"], json!(["footprints"]));
}

#[test]
fn footprint_choice_default_moves_to_the_new_default() {
    let mut f = fixture("footprint-choice");
    let r1 = f.place(R_10K, "0mm");
    f.edit_library(
        "parts/R_10k.redapart",
        "\"a0000000-0000-4000-8000-000000000040\",\n    \"a0000000-0000-4000-8000-000000000050\"",
        "\"a0000000-0000-4000-8000-000000000050\",\n    \"a0000000-0000-4000-8000-000000000040\"",
    );
    let changes = |f: &mut Fixture, settings: Value| {
        f.preview(json!({"settings": settings}))["components"][0]["changes"].clone()
    };
    assert_eq!(
        changes(&mut f, json!({})),
        json!([{"kind": "footprint_order"}])
    );
    assert_eq!(
        changes(&mut f, json!({"footprint_choice": "default"})),
        json!([{"kind": "footprint_order"},
               {"kind": "footprint_choice", "from": R_0603, "to": R_0805}])
    );
    f.apply(
        json!({"settings": {"footprint_choice": "default"}}),
        json!({}),
    );
    assert_eq!(f.object(&r1["unit"])["footprint"], R_0805);
}

#[test]
fn field_positions_and_visibility_are_kept_unless_the_settings_say_otherwise() {
    let mut f = fixture("positions");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.field.move",
        json!({"unit": r1["unit"], "field": "value", "at": {"x": "5mm", "y": "5mm"}}),
    );
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "visible": {"mpn": true}}),
    );
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    let before = f.object(&r1["unit"]);
    assert!(before.get("field_text").is_some(), "{before}");

    f.apply(json!({}), json!({}));
    let kept = f.object(&r1["unit"]);
    assert_eq!(kept["field_text"], before["field_text"]);
    assert_eq!(kept["visible_fields"], before["visible_fields"]);

    f.edit_library("parts/R_10k.redapart", "C99999", "C11111");
    f.apply(
        json!({"settings": {"preserve_field_locations": false, "preserve_field_visibility": false}}),
        json!({}),
    );
    let reset = f.object(&r1["unit"]);
    assert!(reset.get("field_text").is_none(), "{reset}");
    assert!(reset.get("visible_fields").is_none(), "{reset}");
}

#[test]
fn erc_reports_library_differs_only_when_library_out_of_date_is_on() {
    let mut f = fixture("erc");
    let r1 = f.place(R_10K, "0mm");
    f.place(C_100N, "20mm");
    f.edit_library("parts/R_10k.redapart", "C25804", "C99999");
    let found = |f: &mut Fixture| -> Vec<Value> {
        f.engine.run_ok("erc.run", json!({}))["violations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["check"] == "library_out_of_date")
            .cloned()
            .collect()
    };
    assert_eq!(found(&mut f), Vec::<Value>::new(), "Off by default");

    f.engine.run_ok(
        "erc.set_severity",
        json!({"check": "library_out_of_date", "severity": "warning"}),
    );

    let violations = found(&mut f);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(violations[0]["severity"], "warning");
    assert_eq!(violations[0]["args"]["designator"], "R1");
    assert_eq!(violations[0]["args"]["aspects"], "fields");
    assert_eq!(violations[0]["objects"], json!([r1["unit"]]));

    f.apply(json!({}), json!({}));
    assert_eq!(found(&mut f), Vec::<Value>::new());
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

#[test]
fn an_update_matches_units_by_uuid() {
    let mut f = fixture("units-by-uuid");
    let u1 = f.place(LM358, "0mm");
    let component = u1["component"]["uuid"].clone();
    let unit_b = f.engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "at": {"x": "20mm", "y": 0}}),
    )["result"]["unit"]
        .clone();
    assert_eq!(f.object(&unit_b)["unit"], OPAMP_UNIT_B);
    // Unit B moves to the front of the Symbol and is renamed: by index the placed Unit A
    // would now be B, but Units match by UUID.
    f.edit_library(
        "symbols/OpAmp_Dual.redasym",
        r#"    {"uuid":"a0000000-0000-4000-8000-000000000031","name":"A"},
    {"uuid":"a0000000-0000-4000-8000-000000000032","name":"B"},"#,
        r#"    {"uuid":"a0000000-0000-4000-8000-000000000032","name":"Second"},
    {"uuid":"a0000000-0000-4000-8000-000000000031","name":"A"},"#,
    );

    let preview = f.preview(json!({}));
    let row = &preview["components"][0];
    assert!(row.get("blocked").is_none(), "{row}");
    assert!(
        !row["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "unit_removed" || c["kind"] == "unit_added"),
        "{row}"
    );
    f.apply(json!({}), json!({}));

    assert_eq!(f.object(&unit_b)["unit"], OPAMP_UNIT_B);
    assert_eq!(
        f.object(&u1["unit"])["unit"],
        "a0000000-0000-4000-8000-000000000031"
    );
    assert_eq!(f.status()[0]["status"], "up_to_date");
}
