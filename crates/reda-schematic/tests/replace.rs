//! Replace Part: `schematic.component.replace_preview` and `replace_part` run the Update
//! from Library compare with a chosen Part as the new one, for every aspect.

mod update_fixture;

use reda_command::ErrorCode;
use serde_json::{Value, json};
use update_fixture::{Fixture, SHEET, fixture};

const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const HOLE: &str = "a0000000-0000-4000-8000-000000000130";
const NT_0603: &str = "a0000000-0000-4000-8000-000000000170";
const R_0603: &str = "a0000000-0000-4000-8000-000000000040";
const R_0805: &str = "a0000000-0000-4000-8000-000000000050";
const R_2512: &str = "a0000000-0000-4000-8000-000000000070";

impl Fixture {
    fn replace_preview(&mut self, params: Value) -> Value {
        self.engine
            .run_ok("schematic.component.replace_preview", params)
    }

    fn replace(&mut self, params: Value) -> Value {
        self.engine
            .run_ok("schematic.component.replace_part", params)
    }

    /// The part-table entry a Unit uses.
    fn entry_of(&mut self, unit: &Value) -> Value {
        let entry = self.object(unit)["part"].clone();
        self.object(&entry)
    }
}

fn sync_changes(f: &mut Fixture) -> Vec<Value> {
    let preview = f.engine.run_ok("sync.preview", json!({}));
    preview["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["changes"].as_array().unwrap().clone())
        .collect()
}

fn sync(f: &mut Fixture) {
    let preview = f.engine.run_ok("sync.preview", json!({}));
    f.engine
        .run_ok("sync.apply", json!({"hash": preview["hash"]}));
}

#[test]
fn replacing_keeps_the_component_so_sync_sees_a_footprint_change() {
    let mut f = fixture("replace-keeps");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    let unit = r1["unit"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "footprint": R_0805, "not_fitted": true}),
    );
    f.engine.run_ok(
        "schematic.field.move",
        json!({"unit": unit, "field": "value", "at": {"x": "5mm", "y": "1mm"}}),
    );
    sync(&mut f);
    let before = f.object(&unit);

    let done = f.replace(json!({"components": [component], "part": C_100N}));

    assert_eq!(done["result"]["replaced"], json!([component]));
    let after = f.object(&unit);
    assert_eq!(after["uuid"], before["uuid"]);
    assert_eq!(after["component"], before["component"]);
    assert_eq!(after["designator"], "R1", "the Designator prefix stays");
    assert_eq!(after["at"], before["at"]);
    assert_eq!(after["rotation"], before["rotation"]);
    assert_eq!(after["field_text"], before["field_text"]);
    assert_eq!(after["not_fitted"], true);
    assert_eq!(f.entry_of(&unit)["part"]["uuid"], C_100N);
    // C_100n has only R_0603: no same UUID, no same name, so its default.
    assert_eq!(after["footprint"], R_0603);

    let changes = sync_changes(&mut f);
    let kinds: Vec<&Value> = changes.iter().map(|c| &c["kind"]).collect();
    assert!(kinds.contains(&&json!("change_footprint")), "{changes:?}");
    assert!(!kinds.contains(&&json!("add_component")), "{changes:?}");
    assert!(!kinds.contains(&&json!("remove_component")), "{changes:?}");
}

/// R1 from R_10k with its value edited and a field of its own, replaced by NT_0603
/// (value `NetTie`, no MPN or LCSC, a Net Tie not in the BOM) under `keep`.
fn replaced_fields(label: &str, keep: &str) -> Value {
    let mut f = fixture(label);
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "value": "4k7", "fields": {"Note": "hand"}}),
    );
    f.replace(json!({"components": [component], "part": NT_0603, "keep_fields": keep}));
    f.object(&r1["unit"])
}

#[test]
fn keep_fields_edited_keeps_values_that_differ_from_the_old_defaults() {
    let unit = replaced_fields("replace-edited", "edited");
    assert_eq!(unit["fields"]["value"], "4k7", "edited, so kept");
    assert!(unit["fields"].get("mpn").is_none(), "the old default goes");
    assert_eq!(
        unit["fields"]["custom"],
        json!([{"name": "Note", "value": "hand"}]),
        "LCSC (the old default) goes, Note (the Component's own) stays"
    );
    assert_eq!(unit["component_type"], "net_tie", "always the new Part's");
    assert_eq!(
        unit["in_bom"], false,
        "held the old default, so takes the new"
    );
    assert_eq!(unit["in_cpl"], false);
}

#[test]
fn keep_fields_all_keeps_every_value_but_the_component_type() {
    let unit = replaced_fields("replace-all", "all");
    assert_eq!(unit["fields"]["value"], "4k7");
    assert_eq!(unit["fields"]["mpn"], "RC0603FR-0710KL");
    assert_eq!(
        unit["fields"]["custom"],
        json!([{"name": "LCSC", "value": "C25804"}, {"name": "Note", "value": "hand"}])
    );
    assert_eq!(unit["component_type"], "net_tie", "always the new Part's");
    assert_eq!(unit["in_bom"], true, "In BOM follows keep_fields");
    assert_eq!(unit["in_cpl"], true);
}

#[test]
fn keep_fields_none_takes_every_new_default() {
    let unit = replaced_fields("replace-none", "none");
    assert_eq!(unit["fields"]["value"], "NetTie");
    assert!(unit["fields"].get("mpn").is_none());
    assert!(unit["fields"].get("custom").is_none(), "{unit}");
    assert_eq!(unit["component_type"], "net_tie");
    assert_eq!(unit["in_bom"], false);
    assert_eq!(unit["in_cpl"], false);
}

#[test]
fn keep_fields_defaults_to_edited_and_rejects_other_values() {
    let mut f = fixture("replace-keep-default");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    let preview = f.replace_preview(json!({"components": [component], "part": C_100N}));
    let value = preview["components"][0]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cell"] == "value")
        .unwrap()
        .clone();
    assert_eq!(
        value,
        json!({"cell": "value", "case": "changed", "old": "10k", "new": "100n",
               "component": "10k", "action": "update"})
    );
    let err = f.engine.run_err(
        "schematic.component.replace_part",
        json!({"components": [component], "part": C_100N, "keep_fields": "some"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

const R_ALT: &str = "a0000000-0000-4000-8000-0000000001a0";

impl Fixture {
    /// Adds Part R_Alt to the Library, with these alternates.
    fn add_r_alt(&self, footprints: &[&str]) {
        let footprints: Vec<String> = footprints.iter().map(|f| format!("\"{f}\"")).collect();
        let part = format!(
            r#"{{
  "format": 1,
  "part": {{"uuid":"{R_ALT}","name":"R_Alt","component_type":"standard","in_bom":true,"in_cpl":true,"symbol":"a0000000-0000-4000-8000-000000000010"}},
  "footprints": [{}],
  "fields": {{"value":"10k"}},
  "pin_map": {{}}
}}
"#,
            footprints.join(", ")
        );
        std::fs::write(self.library.join("parts/R_Alt.redapart"), part).unwrap();
    }

    /// R1 from R_10k on its R_0805 alternate.
    fn r1_on_0805(&mut self) -> Value {
        let r1 = self.place(R_10K, "0mm");
        self.engine.run_ok(
            "schematic.component.set",
            json!({"components": [r1["component"]["uuid"]], "footprint": R_0805}),
        );
        r1
    }
}

#[test]
fn the_footprint_stays_the_same_by_uuid_first() {
    let mut f = fixture("replace-fp-uuid");
    let r1 = f.r1_on_0805();
    f.add_r_alt(&[R_2512, R_0805]);
    let component = r1["component"]["uuid"].clone();
    let preview = f.replace_preview(json!({"components": [component], "part": R_ALT}));
    assert_eq!(preview["components"][0]["footprint"], R_0805);
    f.replace(json!({"components": [component], "part": R_ALT}));
    assert_eq!(
        f.object(&r1["unit"])["footprint"],
        R_0805,
        "not the default R_2512"
    );
}

#[test]
fn the_footprint_stays_the_same_by_name_next() {
    let mut f = fixture("replace-fp-name");
    let r1 = f.r1_on_0805();
    // Another Footprint called R_0805, with a new UUID.
    let other = "a0000000-0000-4000-8000-0000000001b0";
    f.edit_library("footprints/R_0805.redafp", R_0805, other);
    f.add_r_alt(&[R_2512, other]);
    let component = r1["component"]["uuid"].clone();
    f.replace(json!({"components": [component], "part": R_ALT}));
    assert_eq!(f.object(&r1["unit"])["footprint"], other);
}

#[test]
fn the_footprint_is_the_new_default_otherwise_or_the_one_named() {
    let mut f = fixture("replace-fp-default");
    let r1 = f.r1_on_0805();
    let r2 = f.r1_on_0805();
    f.add_r_alt(&[R_2512, R_0603]);
    let (c1, c2) = (
        r1["component"]["uuid"].clone(),
        r2["component"]["uuid"].clone(),
    );
    f.replace(json!({"components": [c1], "part": R_ALT}));
    assert_eq!(f.object(&r1["unit"])["footprint"], R_2512);
    f.replace(json!({"components": [c2], "part": R_ALT, "footprint": R_0603}));
    assert_eq!(f.object(&r2["unit"])["footprint"], R_0603);

    let err = f.engine.run_err(
        "schematic.component.replace_part",
        json!({"components": [c1], "part": R_ALT, "footprint": R_0805}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details, Some(json!({"pointer": "/footprint"})));
}

const OPAMP_ALT: &str = "a0000000-0000-4000-8000-0000000001c0";
const ALT_UNIT_X: &str = "a0000000-0000-4000-8000-0000000001d1";
const ALT_UNIT_Y: &str = "a0000000-0000-4000-8000-0000000001d2";
const OPAMP_UNIT_A: &str = "a0000000-0000-4000-8000-000000000031";
const OPAMP_UNIT_B: &str = "a0000000-0000-4000-8000-000000000032";

impl Fixture {
    /// Adds Part OpAmp_Alt: a Symbol with two Units, X and Y, whose UUIDs are new.
    fn add_opamp_alt(&self) {
        let symbol = format!(
            r#"{{
  "format": 1,
  "symbol": {{"uuid":"a0000000-0000-4000-8000-0000000001d0","name":"OpAmp_Alt","designator_prefix":"U"}},
  "units": [
    {{"uuid":"{ALT_UNIT_X}","name":"X"}},
    {{"uuid":"{ALT_UNIT_Y}","name":"Y"}}
  ],
  "pins": [
    {{"uuid":"a0000000-0000-4000-8000-0000000001d3","unit":"{ALT_UNIT_X}","name":"OUT","number":"1","electrical_type":"output","at":{{"x":7620000,"y":0}},"rotation":180000,"length":2540000}},
    {{"uuid":"a0000000-0000-4000-8000-0000000001d4","unit":"{ALT_UNIT_Y}","name":"OUT","number":"7","electrical_type":"output","at":{{"x":7620000,"y":0}},"rotation":180000,"length":2540000}}
  ]
}}
"#
        );
        std::fs::write(self.library.join("symbols/OpAmp_Alt.redasym"), symbol).unwrap();
        let part = format!(
            r#"{{
  "format": 1,
  "part": {{"uuid":"{OPAMP_ALT}","name":"OpAmp_Alt","component_type":"standard","in_bom":true,"in_cpl":true,"symbol":"a0000000-0000-4000-8000-0000000001d0"}},
  "footprints": ["a0000000-0000-4000-8000-000000000060"],
  "fields": {{"value":"Alt"}},
  "pin_map": {{}}
}}
"#
        );
        std::fs::write(self.library.join("parts/OpAmp_Alt.redapart"), part).unwrap();
    }

    /// Places the next unplaced Unit of `component`.
    fn place_unit(&mut self, component: &Value, x: &str) -> Value {
        self.engine.run_ok(
            "schematic.unit.place",
            json!({"document": SHEET, "component": component, "at": {"x": x, "y": "30mm"}}),
        )["result"]["unit"]
            .clone()
    }
}

#[test]
fn units_match_by_index() {
    let mut f = fixture("replace-units");
    f.add_opamp_alt();
    let u1 = f.place(LM358, "0mm");
    let component = u1["component"]["uuid"].clone();
    let unit_b = f.place_unit(&component, "20mm");
    assert_eq!(f.object(&u1["unit"])["unit"], OPAMP_UNIT_A);
    assert_eq!(f.object(&unit_b)["unit"], OPAMP_UNIT_B);

    let preview = f.replace_preview(json!({"components": [component], "part": OPAMP_ALT}));
    let row = &preview["components"][0];
    assert_eq!(
        row["units"],
        json!([
            {"unit": u1["unit"], "from": "A", "to": "X"},
            {"unit": unit_b, "from": "B", "to": "Y"},
        ])
    );
    assert!(row.get("blocked").is_none(), "{row}");
    assert!(
        row["changes"]
            .as_array()
            .unwrap()
            .contains(&json!({"kind": "unit_removed", "unit": "Power"})),
        "the unplaced Power Unit has no match: {row}"
    );

    f.replace(json!({"components": [component], "part": OPAMP_ALT}));
    assert_eq!(f.object(&u1["unit"])["unit"], ALT_UNIT_X);
    assert_eq!(f.object(&unit_b)["unit"], ALT_UNIT_Y);
    assert_eq!(f.object(&u1["unit"])["designator"], "U1");
}

#[test]
fn a_placed_unit_with_no_match_blocks_that_component() {
    let mut f = fixture("replace-units-blocked");
    f.add_opamp_alt();
    let u1 = f.place(LM358, "0mm");
    let component = u1["component"]["uuid"].clone();
    f.place_unit(&component, "20mm");
    let power = f.place_unit(&component, "40mm");
    let r1 = f.place(R_10K, "60mm");
    let r = r1["component"]["uuid"].clone();

    let preview = f.replace_preview(json!({"components": [component, r], "part": OPAMP_ALT}));
    let row = preview["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["designator"] == "U1")
        .unwrap()
        .clone();
    assert_eq!(row["blocked"], true);
    assert_eq!(
        row["problems"],
        json!([{"kind": "placed_unit_removed", "unit": "Power"}])
    );
    assert_eq!(row["units"][2], json!({"unit": power, "from": "Power"}));
    assert!(
        row["changes"]
            .as_array()
            .unwrap()
            .contains(&json!({"kind": "unit_removed", "unit": "Power", "placed": true})),
        "{row}"
    );

    let err = f.engine.run_err(
        "schematic.component.replace_part",
        json!({"components": [component, r], "part": OPAMP_ALT}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("U1"), "{}", err.message);
    assert_eq!(
        f.entry_of(&r1["unit"])["part"]["uuid"],
        R_10K,
        "nothing changed"
    );

    // Once the Power Unit is deleted, the Replace goes ahead.
    f.engine
        .run_ok("schematic.delete", json!({"objects": [power]}));
    f.replace(json!({"components": [component], "part": OPAMP_ALT}));
    assert_eq!(f.entry_of(&u1["unit"])["part"]["uuid"], OPAMP_ALT);
}

#[test]
fn new_units_appear_as_unplaced() {
    let mut f = fixture("replace-units-new");
    let r1 = f.place(R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();
    let preview = f.replace_preview(json!({"components": [component], "part": LM358}));
    let changes = preview["components"][0]["changes"]
        .as_array()
        .unwrap()
        .clone();
    for unit in ["B", "Power"] {
        assert!(
            changes.contains(&json!({"kind": "unit_added", "unit": unit})),
            "{changes:?}"
        );
    }
    f.replace(json!({"components": [component], "part": LM358}));
    assert_eq!(f.object(&r1["unit"])["unit"], OPAMP_UNIT_A);
    // The next Unit placed is B, which was unplaced.
    let next = f.place_unit(&component, "20mm");
    assert_eq!(f.object(&next)["unit"], OPAMP_UNIT_B);
    assert_eq!(
        f.object(&next)["designator"],
        "R1",
        "the Designator prefix stays"
    );
}

#[test]
fn a_part_with_no_symbol_is_refused() {
    let mut f = fixture("replace-no-symbol");
    let r1 = f.place(R_10K, "0mm");
    let err = f.engine.run_err(
        "schematic.component.replace_part",
        json!({"components": [r1["component"]["uuid"]], "part": HOLE}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("no Symbol"), "{}", err.message);
}

#[test]
fn the_preview_changes_nothing_and_the_replace_is_one_undo_step() {
    let mut f = fixture("replace-undo");
    let r1 = f.place(R_10K, "0mm");
    let r2 = f.place(R_10K, "20mm");
    let components = json!([r1["component"]["uuid"], r2["component"]["uuid"]]);
    let before = f.entry_of(&r1["unit"]);
    let params = json!({"components": components, "part": C_100N});
    let preview = f.replace_preview(params.clone());
    assert_eq!(preview["part"], C_100N);
    assert_eq!(preview["part_name"], "C_100n");
    let rows: Vec<&Value> = (preview["components"].as_array().unwrap().iter())
        .map(|c| &c["designator"])
        .collect();
    assert_eq!(rows, [&json!("R1"), &json!("R2")]);
    assert_eq!(preview["components"][0]["from_part_name"], "R_10k");
    assert!(
        preview["components"][0]["changes"]
            .as_array()
            .unwrap()
            .contains(&json!({"kind": "name", "from": "R_10k", "to": "C_100n"}))
    );
    assert_eq!(
        f.entry_of(&r1["unit"]),
        before,
        "the preview changes nothing"
    );

    // A preview that no longer matches is a conflict.
    f.engine.run_ok(
        "schematic.component.set",
        json!({"components": [r1["component"]["uuid"]], "value": "4k7"}),
    );
    let mut stale = params.clone();
    stale["preview_hash"] = preview["preview_hash"].clone();
    let err = f.engine.run_err("schematic.component.replace_part", stale);
    assert_eq!(err.code, ErrorCode::Conflict);

    let mut fresh = params.clone();
    fresh["preview_hash"] = f.replace_preview(params)["preview_hash"].clone();
    let done = f.replace(fresh);
    assert_eq!(done["result"]["replaced"], components);
    assert_eq!(f.entry_of(&r2["unit"])["part"]["uuid"], C_100N);
    // The R_10k entry is gone with both Components off it.
    assert!((done["changes"][SHEET]["deleted"].as_array().unwrap()).contains(&before["uuid"]));

    f.engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(f.entry_of(&r1["unit"]), before);
    assert_eq!(f.entry_of(&r2["unit"]), before);
}

#[test]
fn pin_changes_name_the_nets_they_leave_and_wires_never_move() {
    let mut f = fixture("replace-pins");
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
    // C's Symbol has its Pin 1 elsewhere.
    f.edit_library(
        "symbols/C.redasym",
        r#""number":"1","electrical_type":"passive","at":{"x":0,"y":3810000}"#,
        r#""number":"1","electrical_type":"passive","at":{"x":0,"y":5080000}"#,
    );
    let component = r1["component"]["uuid"].clone();
    let preview = f.replace_preview(json!({"components": [component], "part": C_100N}));
    let changes = preview["components"][0]["changes"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        changes.contains(&json!({"kind": "pin", "unit": "A", "number": "1",
                                 "moved": true, "leaves": "SIG"})),
        "{changes:?}"
    );
    let done = f.replace(json!({"components": [component], "part": C_100N}));
    assert_eq!(f.object(&wire), wire_before, "Wires never move");
    for list in ["created", "modified", "deleted"] {
        for uuid in done["changes"][SHEET][list]
            .as_array()
            .into_iter()
            .flatten()
        {
            assert_ne!(uuid, &wire, "the Wire is not touched");
        }
    }
}
