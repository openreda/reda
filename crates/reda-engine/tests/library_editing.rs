//! Library element editing: opening, creating, editing and saving Symbols, Footprints and
//! Parts through Commands, each element on its own undo stack.

use std::path::Path;

use reda_engine::command::ErrorCode;
use reda_engine::format::library::write_library_info;
use reda_engine::model::{LibraryInfo, Uuid};
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::{Value, json};

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

/// Writes an empty Library called `name` into `folder`.
fn write_library(folder: &Path, uuid: &str, name: &str) {
    std::fs::create_dir_all(folder).unwrap();
    let info = LibraryInfo {
        uuid: uuid.parse::<Uuid>().unwrap(),
        name: name.into(),
        description: String::new(),
    };
    std::fs::write(
        folder.join(format!("{name}.redalib")),
        write_library_info(&info).unwrap(),
    )
    .unwrap();
}

/// A new Project in `dir/Demo` with the empty Project Library `dir/Demo/lib`.
fn engine_with_library(dir: &TempDir) -> TestEngine {
    let project = dir.path().join("Demo");
    let mut e = test_engine();
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    write_library(&project.join("lib"), LIBRARY, "Mine");
    e.run_ok("project.library.add", json!({"path": "lib"}));
    e
}

fn new_element(e: &mut TestEngine, kind: &str, name: &str) -> String {
    let out = e.run_ok(
        "library.element.new",
        json!({"library": LIBRARY, "kind": kind, "name": name}),
    );
    out["element"].as_str().unwrap().to_owned()
}

fn object(e: &mut TestEngine, uuid: &str) -> Value {
    e.run_ok("object.get", json!({ "uuid": uuid }))["object"].clone()
}

#[test]
fn a_new_symbol_has_one_unit_and_saves_to_its_library() {
    let dir = TempDir::new("lib-new-symbol");
    let mut e = engine_with_library(&dir);
    let symbol = new_element(&mut e, "symbol", "Resistor");

    let obj = object(&mut e, &symbol);
    assert_eq!(obj["type"], "symbol");
    assert_eq!(obj["units"].as_array().unwrap().len(), 1);
    assert_eq!(obj["units"][0]["name"], "A");

    let file = dir.path().join("Demo/lib/symbols/Resistor.redasym");
    assert!(!file.exists(), "a new element is not written until saved");
    e.run_ok("library.element.save", json!({ "element": symbol }));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"name\":\"Resistor\""), "{text}");
}

#[test]
fn a_new_element_needs_a_registered_library() {
    let dir = TempDir::new("lib-new-unknown");
    let mut e = engine_with_library(&dir);
    let err = e.run_err(
        "library.element.new",
        json!({"library": "000000bb-0000-4000-8000-000000000000", "kind": "part", "name": "X"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

fn edit(e: &mut TestEngine, id: &str, params: Value) -> Value {
    e.run_ok(id, params)["result"].clone()
}

#[test]
fn symbol_units_pins_and_graphics_are_edited_by_commands() {
    let dir = TempDir::new("lib-symbol-edit");
    let mut e = engine_with_library(&dir);
    let symbol = new_element(&mut e, "symbol", "OpAmp");
    edit(
        &mut e,
        "library.symbol.edit",
        json!({"symbol": symbol, "designator_prefix": "U", "description": "Dual op-amp"}),
    );
    let b = edit(&mut e, "library.symbol.unit.add", json!({"symbol": symbol}));
    assert_eq!(b["name"], "B");
    let b = b["unit"].as_str().unwrap().to_owned();
    edit(
        &mut e,
        "library.symbol.unit.edit",
        json!({"symbol": symbol, "unit": b, "name": "Power"}),
    );

    let pin = edit(
        &mut e,
        "library.symbol.pin.add",
        json!({"symbol": symbol, "unit": b, "name": "V+", "number": "8",
               "electrical_type": "power_in", "at": {"x": "0mil", "y": "300mil"},
               "rotation": "270deg"}),
    )["pin"]
        .as_str()
        .unwrap()
        .to_owned();
    edit(
        &mut e,
        "library.symbol.pin.edit",
        json!({"symbol": symbol, "pin": pin, "number_visible": false, "length": "150mil"}),
    );
    let graphic = edit(
        &mut e,
        "library.symbol.graphic.add",
        json!({"symbol": symbol, "shape": {"kind": "rect",
               "start": {"x": "-200mil", "y": "-200mil"}, "end": {"x": "200mil", "y": "200mil"}},
               "width": "10mil", "fill": true}),
    )["graphic"]
        .as_str()
        .unwrap()
        .to_owned();
    edit(
        &mut e,
        "library.symbol.graphic.edit",
        json!({"symbol": symbol, "graphic": graphic,
               "shape": {"kind": "circle", "center": {"x": 0, "y": 0}, "radius": "100mil"}}),
    );

    let obj = object(&mut e, &symbol);
    assert_eq!(obj["description"], "Dual op-amp");
    assert_eq!(obj["units"][1]["name"], "Power");
    let pins = obj["pins"].as_array().unwrap();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0]["electrical_type"], "power_in");
    assert_eq!(pins[0]["at"], json!({"x": 0, "y": 7_620_000}));
    assert_eq!(pins[0]["rotation"], 270_000);
    assert_eq!(pins[0]["length"], 3_810_000);
    assert_eq!(pins[0]["number_visible"], false);
    assert_eq!(
        pins[0]["name_visible"],
        Value::Null,
        "visible is the default"
    );
    let graphics = obj["graphics"].as_array().unwrap();
    assert_eq!(
        graphics[0]["unit"], obj["units"][0]["uuid"],
        "defaults to Unit A"
    );
    assert_eq!(
        graphics[0]["shape"],
        json!({"kind": "circle", "center": {"x": 0, "y": 0}, "radius": 2_540_000})
    );
    assert_eq!(graphics[0]["fill"], true);

    edit(
        &mut e,
        "library.symbol.pin.remove",
        json!({"symbol": symbol, "pin": pin}),
    );
    edit(
        &mut e,
        "library.symbol.graphic.remove",
        json!({"symbol": symbol, "graphic": graphic}),
    );
    let obj = object(&mut e, &symbol);
    assert!(obj["pins"].as_array().unwrap().is_empty());
    assert!(obj["graphics"].as_array().unwrap().is_empty());
}

#[test]
fn removing_a_unit_removes_its_pins_but_the_last_unit_stays() {
    let dir = TempDir::new("lib-unit-remove");
    let mut e = engine_with_library(&dir);
    let symbol = new_element(&mut e, "symbol", "Gate");
    let b = edit(&mut e, "library.symbol.unit.add", json!({"symbol": symbol}))["unit"].clone();
    edit(
        &mut e,
        "library.symbol.pin.add",
        json!({"symbol": symbol, "unit": b, "name": "A", "number": "1", "at": {"x": 0, "y": 0}}),
    );
    edit(
        &mut e,
        "library.symbol.unit.remove",
        json!({"symbol": symbol, "unit": b}),
    );
    let obj = object(&mut e, &symbol);
    assert_eq!(obj["units"].as_array().unwrap().len(), 1);
    assert!(obj["pins"].as_array().unwrap().is_empty());

    let a = obj["units"][0]["uuid"].clone();
    let err = e.run_err(
        "library.symbol.unit.remove",
        json!({"symbol": symbol, "unit": a}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn pin_numbers_are_unique_across_the_symbol() {
    let dir = TempDir::new("lib-pin-numbers");
    let mut e = engine_with_library(&dir);
    let symbol = new_element(&mut e, "symbol", "R");
    let add = |n: &str| json!({"symbol": symbol, "name": "~", "number": n, "at": {"x": 0, "y": 0}});
    edit(&mut e, "library.symbol.pin.add", add("1"));
    let err = e.run_err("library.symbol.pin.add", add("1"));
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn each_element_has_its_own_undo_stack() {
    let dir = TempDir::new("lib-undo");
    let mut e = engine_with_library(&dir);
    let r = new_element(&mut e, "symbol", "R");
    let c = new_element(&mut e, "symbol", "C");
    let out = e.run_ok(
        "library.symbol.edit",
        json!({"symbol": r, "designator_prefix": "R"}),
    );
    assert_eq!(out["changes"][&r]["modified"], json!([r]));
    e.run_ok(
        "library.symbol.edit",
        json!({"symbol": c, "designator_prefix": "C"}),
    );

    let out = e.run_ok("edit.undo", json!({"document": r}));
    assert_eq!(out["result"]["label"], "Edit Symbol");
    assert_eq!(object(&mut e, &r)["designator_prefix"], "U");
    assert_eq!(object(&mut e, &c)["designator_prefix"], "C");
    e.run_ok("edit.redo", json!({"document": r}));
    assert_eq!(object(&mut e, &r)["designator_prefix"], "R");
}

#[test]
fn footprint_pads_cover_every_kind_shape_and_hole() {
    let dir = TempDir::new("lib-pads");
    let mut e = engine_with_library(&dir);
    let fp = new_element(&mut e, "footprint", "Pads");
    let mut pad = |params: Value| {
        let mut params = params;
        params["footprint"] = json!(fp);
        e.run_ok("library.footprint.pad.add", params)["result"]["pad"].clone()
    };
    pad(
        json!({"number": "1", "kind": "smd", "shape": "rounded_rect",
               "at": {"x": "-1mm", "y": 0}, "width": "1mm", "height": "0.8mm",
               "paste_margin": "-0.05mm"}),
    );
    pad(
        json!({"number": "2", "kind": "plated_through_hole", "shape": "circle",
               "at": {"x": "1mm", "y": 0}, "width": "1.6mm", "height": "1.6mm",
               "hole": {"width": "0.8mm"}, "mask_margin": "0.05mm"}),
    );
    pad(
        json!({"number": "3", "kind": "plated_through_hole", "shape": "oval",
               "at": {"x": "3mm", "y": 0}, "width": "1.6mm", "height": "2.4mm",
               "hole": {"width": "0.8mm", "height": "1.6mm"}}),
    );
    let npth = pad(
        json!({"number": "", "kind": "non_plated", "shape": "circle",
               "at": {"x": 0, "y": "3mm"}, "width": "3.2mm", "height": "3.2mm",
               "hole": {"width": "3.2mm"}}),
    );
    pad(json!({"number": "4", "kind": "smd", "shape": "rect",
               "at": {"x": 0, "y": "-3mm"}, "width": "1mm", "height": "1mm"}));

    let obj = object(&mut e, &fp);
    let pads = obj["pads"].as_array().unwrap();
    let by_number = |n: &str| pads.iter().find(|p| p["number"] == n).unwrap().clone();
    assert_eq!(
        by_number("1")["corner_ratio"],
        50_000,
        "a radius of a quarter of the shorter side"
    );
    assert_eq!(by_number("1")["paste_margin"], -50_000);
    assert_eq!(
        by_number("2")["hole"],
        json!({"width": 800_000, "height": 800_000})
    );
    assert_eq!(by_number("2")["mask_margin"], 50_000);
    assert_eq!(
        by_number("3")["hole"],
        json!({"width": 800_000, "height": 1_600_000})
    );
    assert_eq!(by_number("")["kind"], "non_plated");

    e.run_ok(
        "library.footprint.pad.edit",
        json!({"footprint": fp, "pad": npth, "mask_margin": "0.1mm", "number": "MH"}),
    );
    let pad1 = by_number("1")["uuid"].clone();
    e.run_ok(
        "library.footprint.pad.edit",
        json!({"footprint": fp, "pad": pad1, "paste_margin": null, "shape": "rect"}),
    );
    let obj = object(&mut e, &fp);
    let pads = obj["pads"].as_array().unwrap();
    let edited = pads.iter().find(|p| p["uuid"] == pad1).unwrap();
    assert_eq!(
        edited["paste_margin"],
        Value::Null,
        "null clears the override"
    );
    assert_eq!(
        edited["corner_ratio"],
        Value::Null,
        "a rect has no corner ratio"
    );
    assert!(
        pads.iter()
            .any(|p| p["number"] == "MH" && p["mask_margin"] == 100_000)
    );

    e.run_ok(
        "library.footprint.pad.remove",
        json!({"footprint": fp, "pad": npth}),
    );
    assert_eq!(object(&mut e, &fp)["pads"].as_array().unwrap().len(), 4);
}

#[test]
fn pads_that_cannot_be_made_are_refused() {
    let dir = TempDir::new("lib-bad-pads");
    let mut e = engine_with_library(&dir);
    let fp = new_element(&mut e, "footprint", "Bad");
    let base = json!({"footprint": fp, "number": "1", "shape": "circle",
                      "at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"});
    let with = |extra: Value| {
        let mut params = base.clone();
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        params
    };
    for (params, pointer) in [
        (
            with(json!({"kind": "smd", "hole": {"width": "0.5mm"}})),
            "/hole",
        ),
        (with(json!({"kind": "plated_through_hole"})), "/hole"),
        (
            with(json!({"kind": "plated_through_hole", "hole": {"width": "2mm"}})),
            "/hole",
        ),
        (with(json!({"kind": "smd", "width": 0})), "/width"),
        (
            with(json!({"kind": "smd", "corner_ratio": "10%"})),
            "/corner_ratio",
        ),
        (
            with(json!({"kind": "smd", "shape": "chamfered_rect"})),
            "/chamfer_ratio",
        ),
        (
            with(json!({"kind": "smd", "shape": "rounded_rect", "corner_ratio": "120%"})),
            "/corner_ratio",
        ),
        (
            with(
                json!({"kind": "plated_through_hole", "hole": {"width": "0.5mm"},
                        "side": "bottom"}),
            ),
            "/side",
        ),
        (
            with(json!({"kind": "smd", "zone_connection": {"angle": "30deg"}})),
            "/zone_connection/angle",
        ),
    ] {
        let err = e.run_err("library.footprint.pad.add", params.clone());
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
        assert_eq!(err.details.unwrap()["pointer"], pointer, "{params}");
    }
}

#[test]
fn footprint_graphics_text_and_origin_are_edited_by_commands() {
    let dir = TempDir::new("lib-fp-graphics");
    let mut e = engine_with_library(&dir);
    let fp = new_element(&mut e, "footprint", "Box");
    e.run_ok(
        "library.footprint.edit",
        json!({"footprint": fp, "description": "A box"}),
    );
    let pad = e.run_ok(
        "library.footprint.pad.add",
        json!({"footprint": fp, "number": "1", "kind": "smd", "shape": "rect",
               "at": {"x": "1mm", "y": "1mm"}, "width": "1mm", "height": "1mm"}),
    )["result"]["pad"]
        .clone();
    let mut graphics = vec![];
    for layer in [
        "silkscreen_top",
        "silkscreen_bottom",
        "courtyard_top",
        "courtyard_bottom",
        "assembly_top",
        "assembly_bottom",
    ] {
        // A Courtyard Layer takes only closed shapes.
        let shape = if layer.starts_with("courtyard") {
            json!({"kind": "rect", "start": {"x": 0, "y": 0}, "end": {"x": "2mm", "y": "2mm"}})
        } else {
            json!({"kind": "line", "start": {"x": 0, "y": 0}, "end": {"x": "2mm", "y": 0}})
        };
        let out = e.run_ok(
            "library.footprint.graphic.add",
            json!({"footprint": fp, "layer": layer, "shape": shape}),
        );
        graphics.push(out["result"]["graphic"].as_str().unwrap().to_owned());
    }
    e.run_ok(
        "library.footprint.graphic.edit",
        json!({"footprint": fp, "graphic": graphics[0], "width": "0.2mm",
               "shape": {"kind": "polygon", "points": [
                   {"x": 0, "y": 0}, {"x": "2mm", "y": 0}, {"x": "2mm", "y": "2mm"}]}}),
    );
    e.run_ok(
        "library.footprint.graphic.remove",
        json!({"footprint": fp, "graphic": graphics[5]}),
    );
    let texts = object(&mut e, &fp)["texts"].clone();
    let text = |field: &str| {
        texts
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["field"] == field)
            .unwrap()["uuid"]
            .clone()
    };
    e.run_ok(
        "library.footprint.text.edit",
        json!({"footprint": fp, "text": text("Designator"), "at": {"x": 0, "y": "-2mm"},
               "height": "0.8mm"}),
    );
    e.run_ok(
        "library.footprint.text.edit",
        json!({"footprint": fp, "text": text("Value"), "visible": false,
               "layer": "assembly_bottom"}),
    );
    e.run_ok(
        "library.footprint.origin.set",
        json!({"footprint": fp, "at": {"x": "1mm", "y": "1mm"}}),
    );

    let obj = object(&mut e, &fp);
    assert_eq!(obj["description"], "A box");
    assert_eq!(obj["pads"][0]["uuid"], pad);
    assert_eq!(
        obj["pads"][0]["at"],
        json!({"x": 0, "y": 0}),
        "the origin moved"
    );
    let graphics = obj["graphics"].as_array().unwrap();
    assert_eq!(graphics.len(), 5);
    let courtyard = graphics
        .iter()
        .find(|g| g["layer"] == "courtyard_top")
        .unwrap();
    assert_eq!(courtyard["width"], 50_000, "the Courtyard default");
    assert_eq!(
        courtyard["shape"],
        json!({"kind": "rect", "start": {"x": -1_000_000, "y": -1_000_000},
               "end": {"x": 1_000_000, "y": 1_000_000}})
    );
    let silk = graphics
        .iter()
        .find(|g| g["layer"] == "silkscreen_top")
        .unwrap();
    assert_eq!(silk["width"], 200_000);
    assert_eq!(silk["shape"]["kind"], "polygon");
    let field = |name: &str| {
        obj["texts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["field"] == name)
            .unwrap()
            .clone()
    };
    let (designator, value) = (field("Designator"), field("Value"));
    assert_eq!(designator["at"], json!({"x": -1_000_000, "y": -3_000_000}));
    assert_eq!(designator["style"]["height"], 800_000);
    assert_eq!(value["visible"], false);
    assert_eq!(value["layer"], "assembly_bottom");
}

#[test]
fn part_commands_pick_symbol_footprints_pin_map_fields_and_type() {
    let dir = TempDir::new("lib-part");
    let mut e = engine_with_library(&dir);
    let part = new_element(&mut e, "part", "Q");
    let symbol = new_element(&mut e, "symbol", "NPN");
    let (sot23, to92) = (
        new_element(&mut e, "footprint", "SOT-23"),
        new_element(&mut e, "footprint", "TO-92"),
    );
    e.run_ok(
        "library.part.symbol.set",
        json!({"part": part, "symbol": symbol}),
    );
    e.run_ok(
        "library.part.footprints.set",
        json!({"part": part, "footprints": [to92, sot23]}),
    );
    e.run_ok(
        "library.part.pin_map.set",
        json!({"part": part, "pins": [{"pin": "3", "pads": ["1"]}, {"pin": "1", "pads": ["2", "3"]}],
               "unconnected": ["5", "4"]}),
    );
    e.run_ok(
        "library.part.fields.set",
        json!({"part": part, "value": "BC847", "mpn": "BC847B",
               "custom": [{"name": "LCSC", "value": "C8574"}]}),
    );
    e.run_ok(
        "library.part.edit",
        json!({"part": part, "name": "BC847", "component_type": "standard"}),
    );

    let obj = object(&mut e, &part);
    assert_eq!(obj["name"], "BC847");
    assert_eq!(obj["symbol"], json!(symbol));
    assert_eq!(obj["footprints"], json!([to92, sot23]), "the order is kept");
    assert_eq!(
        obj["pin_map"],
        json!({"pins": [{"pin": "1", "pads": ["2", "3"]}, {"pin": "3", "pads": ["1"]}],
               "unconnected": ["4", "5"]})
    );
    assert_eq!(obj["fields"]["value"], "BC847");
    assert_eq!(
        obj["fields"]["custom"],
        json!([{"name": "LCSC", "value": "C8574"}])
    );

    e.run_ok(
        "library.part.edit",
        json!({"part": part, "component_type": "mechanical"}),
    );
    e.run_ok(
        "library.part.symbol.set",
        json!({"part": part, "symbol": null}),
    );
    let obj = object(&mut e, &part);
    assert_eq!(obj["component_type"], "mechanical");
    assert_eq!(obj["symbol"], Value::Null);

    for (params, pointer) in [
        (
            json!({"part": part, "footprints": [to92, to92]}),
            "/footprints/1",
        ),
        (
            json!({"part": part, "custom": [{"name": "mpn", "value": "x"}]}),
            "/custom/0/name",
        ),
        (
            json!({"part": part, "pins": [], "unconnected": ["1", "1"]}),
            "/unconnected/1",
        ),
    ] {
        let id = if params.get("footprints").is_some() {
            "library.part.footprints.set"
        } else if params.get("custom").is_some() {
            "library.part.fields.set"
        } else {
            "library.part.pin_map.set"
        };
        let err = e.run_err(id, params.clone());
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
        assert_eq!(err.details.unwrap()["pointer"], pointer, "{params}");
    }
}

#[test]
fn a_new_type_resets_in_bom_and_in_cpl_to_its_defaults() {
    let dir = TempDir::new("lib-part-flags");
    let mut e = engine_with_library(&dir);
    let part = new_element(&mut e, "part", "Tie");
    let flags = |e: &mut TestEngine| {
        let obj = object(e, &part);
        (
            obj["component_type"].as_str().unwrap().to_owned(),
            obj["in_bom"].as_bool().unwrap(),
            obj["in_cpl"].as_bool().unwrap(),
        )
    };
    assert_eq!(flags(&mut e), ("standard".into(), true, true));

    for (kind, in_bom, in_cpl) in [
        ("net_tie", false, false),
        ("mechanical", true, false),
        ("standard", true, true),
    ] {
        e.run_ok(
            "library.part.edit",
            json!({"part": part, "component_type": kind}),
        );
        assert_eq!(flags(&mut e), (kind.into(), in_bom, in_cpl), "{kind}");
    }

    // The flags are edited on their own, and given with a new type they win over its
    // defaults.
    e.run_ok("library.part.edit", json!({"part": part, "in_cpl": false}));
    assert_eq!(flags(&mut e), ("standard".into(), true, false));
    e.run_ok("library.part.edit", json!({"part": part, "in_cpl": true}));
    e.run_ok(
        "library.part.edit",
        json!({"part": part, "component_type": "mechanical", "in_cpl": true}),
    );
    assert_eq!(flags(&mut e), ("mechanical".into(), true, true));
    // Setting the type it already has keeps the edited flags.
    e.run_ok(
        "library.part.edit",
        json!({"part": part, "component_type": "mechanical"}),
    );
    assert_eq!(flags(&mut e), ("mechanical".into(), true, true));
}

/// The Demo Library of the file-format goldens: LM358 and MountingHole_M3.
fn demo_library() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/library/Demo")
}

const DEMO_SYMBOL: &str = "00000010-0000-4000-8000-000000000000";
const DEMO_FOOTPRINT: &str = "00000030-0000-4000-8000-000000000000";
const DEMO_PART: &str = "00000050-0000-4000-8000-000000000000";

#[test]
fn editing_a_starter_library_element_is_rejected() {
    let dir = TempDir::new("lib-starter");
    let mut e = TestEngine(
        reda_engine::Engine::builder()
            .ids(reda_engine::command::SeededIds::new(0))
            .clock(reda_engine::command::FixedClock::EPOCH)
            .user_dirs(None)
            .starter_library(Some(demo_library()))
            .build(),
    );
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let out = e.run_ok("library.element.open", json!({"element": DEMO_SYMBOL}));
    assert_eq!(out["read_only"], true);
    assert_eq!(out["kind"], "symbol");
    for (id, params) in [
        (
            "library.symbol.edit",
            json!({"symbol": DEMO_SYMBOL, "name": "X"}),
        ),
        ("library.symbol.unit.add", json!({"symbol": DEMO_SYMBOL})),
    ] {
        let err = e.run_err(id, params);
        assert_eq!(err.code, ErrorCode::Rejected, "{id}");
        assert!(err.message.contains("read-only"), "{}", err.message);
    }
    e.run_ok("library.element.open", json!({"element": DEMO_FOOTPRINT}));
    let err = e.run_err(
        "library.footprint.edit",
        json!({"footprint": DEMO_FOOTPRINT, "name": "X"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    e.run_ok("library.element.open", json!({"element": DEMO_PART}));
    let err = e.run_err(
        "library.part.edit",
        json!({"part": DEMO_PART, "component_type": "mechanical"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    let err = e.run_err("library.element.save", json!({"element": DEMO_PART}));
    assert_eq!(err.code, ErrorCode::Rejected);
    let err = e.run_err(
        "library.element.new",
        json!({"library": "00000001-0000-4000-8000-000000000000", "kind": "symbol",
               "name": "Mine"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

/// Copies the Demo Library into `folder`, where it is an ordinary, writable Library.
fn copy_demo(folder: &Path) {
    for (rel, bytes) in reda_testkit::read_tree(&demo_library()) {
        let path = folder.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

#[test]
fn an_opened_element_saves_back_to_its_file_and_tracks_dirty_state() {
    let dir = TempDir::new("lib-open-save");
    let mut e = test_engine();
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    copy_demo(&dir.path().join("Demo/parts-lib"));
    e.run_ok("project.library.add", json!({"path": "parts-lib"}));

    let out = e.run_ok("library.element.open", json!({"element": DEMO_SYMBOL}));
    assert_eq!(out["read_only"], false);
    let scope = reda_engine::model::Scope::Element(DEMO_SYMBOL.parse().unwrap());
    assert!(
        !e.dirty_scopes().contains(&scope),
        "an opened element is clean"
    );
    e.run_ok(
        "library.symbol.edit",
        json!({"symbol": DEMO_SYMBOL, "name": "OpAmp_Renamed"}),
    );
    assert!(e.dirty_scopes().contains(&scope));
    e.run_ok("edit.undo", json!({"document": DEMO_SYMBOL}));
    assert!(
        !e.dirty_scopes().contains(&scope),
        "undone back to the saved state"
    );
    e.run_ok("edit.redo", json!({"document": DEMO_SYMBOL}));

    e.run_ok("project.save", json!({}));
    assert!(
        e.dirty_scopes().contains(&scope),
        "Save All saves the Project, not open Library elements"
    );
    let out = e.run_ok("library.element.save", json!({}));
    // One component at a time: the engine reports native separators, and the path is
    // compared as a JSON string.
    let file = dir
        .path()
        .join("Demo")
        .join("parts-lib")
        .join("symbols")
        .join("OpAmp_Dual.redasym");
    assert_eq!(
        out["written"],
        json!([file]),
        "a renamed element keeps its file"
    );
    assert!(!e.dirty_scopes().contains(&scope));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"name\":\"OpAmp_Renamed\""), "{text}");

    e.run_ok("library.element.close", json!({"element": DEMO_SYMBOL}));
    let err = e.run_err(
        "library.symbol.edit",
        json!({"symbol": DEMO_SYMBOL, "name": "X"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = e.run_err("edit.undo", json!({"document": DEMO_SYMBOL}));
    assert_eq!(
        err.code,
        ErrorCode::Unavailable,
        "closing forgets the stack"
    );
}

/// Builds a resistor Part from nothing: a Symbol, an 0603 Footprint and the Part binding
/// them, saved to a Library and compared with goldens, then found by `library.search`.
#[test]
fn scenario_build_a_resistor_part_end_to_end() {
    let dir = TempDir::new("lib-resistor");
    let mut e = engine_with_library(&dir);

    let symbol = new_element(&mut e, "symbol", "R");
    e.run_ok(
        "library.symbol.edit",
        json!({"symbol": symbol, "designator_prefix": "R", "description": "Resistor"}),
    );
    for (number, y, rotation) in [("1", "100mil", "270deg"), ("2", "-100mil", "90deg")] {
        e.run_ok(
            "library.symbol.pin.add",
            json!({"symbol": symbol, "name": "~", "number": number, "at": {"x": 0, "y": y},
                   "rotation": rotation, "length": "40mil", "name_visible": false}),
        );
    }
    e.run_ok(
        "library.symbol.graphic.add",
        json!({"symbol": symbol, "shape": {"kind": "rect",
               "start": {"x": "-20mil", "y": "-60mil"}, "end": {"x": "20mil", "y": "60mil"}}}),
    );

    let footprint = new_element(&mut e, "footprint", "R_0603_1608Metric");
    e.run_ok(
        "library.footprint.edit",
        json!({"footprint": footprint, "description": "Resistor 0603 (1608 metric), IPC-7351 nominal"}),
    );
    for (number, x) in [("1", "-0.775mm"), ("2", "0.775mm")] {
        e.run_ok(
            "library.footprint.pad.add",
            json!({"footprint": footprint, "number": number, "kind": "smd",
                   "shape": "rounded_rect", "at": {"x": x, "y": 0},
                   "width": "0.9mm", "height": "0.95mm"}),
        );
    }
    e.run_ok(
        "library.footprint.graphic.add",
        json!({"footprint": footprint, "layer": "courtyard_top", "shape": {"kind": "rect",
               "start": {"x": "-1.48mm", "y": "-0.73mm"}, "end": {"x": "1.48mm", "y": "0.73mm"}}}),
    );
    for y in ["-0.5225mm", "0.5225mm"] {
        e.run_ok(
            "library.footprint.graphic.add",
            json!({"footprint": footprint, "layer": "silkscreen_top", "shape": {"kind": "line",
                   "start": {"x": "-0.237mm", "y": y}, "end": {"x": "0.237mm", "y": y}}}),
        );
    }
    e.run_ok(
        "library.footprint.graphic.add",
        json!({"footprint": footprint, "layer": "assembly_top", "shape": {"kind": "rect",
               "start": {"x": "-0.8mm", "y": "-0.4mm"}, "end": {"x": "0.8mm", "y": "0.4mm"}}}),
    );
    let designator = object(&mut e, &footprint)["texts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["field"] == "Designator")
        .unwrap()["uuid"]
        .clone();
    e.run_ok(
        "library.footprint.text.edit",
        json!({"footprint": footprint, "text": designator, "at": {"x": 0, "y": "-1.43mm"}}),
    );

    let part = new_element(&mut e, "part", "Resistor");
    e.run_ok(
        "library.part.symbol.set",
        json!({"part": part, "symbol": symbol}),
    );
    e.run_ok(
        "library.part.footprints.set",
        json!({"part": part, "footprints": [footprint]}),
    );
    e.run_ok(
        "library.part.fields.set",
        json!({"part": part, "value": "10k", "description": "Generic resistor",
               "keywords": "resistor R"}),
    );

    let out = e.run_ok("library.element.save", json!({}));
    assert_eq!(out["written"].as_array().unwrap().len(), 3);
    assert!(
        e.dirty_scopes()
            .iter()
            .all(|s| !matches!(s, reda_engine::model::Scope::Element(_)))
    );
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/library_editing/resistor");
    let files = reda_testkit::read_tree(&dir.path().join("Demo/lib"));
    let elements: Vec<_> = files.keys().filter(|k| !k.ends_with(".redalib")).collect();
    assert_eq!(
        elements,
        [
            "footprints/R_0603_1608Metric.redafp",
            "parts/Resistor.redapart",
            "symbols/R.redasym"
        ]
    );
    for rel in elements {
        reda_testkit::assert_golden(golden.join(rel), &files[rel]);
    }

    let found = e.run_ok("library.search", json!({"text": "10k"}));
    assert_eq!(found["parts"][0]["name"], "Resistor");
    assert_eq!(found["parts"][0]["symbol"], json!(symbol));
}

#[test]
fn an_element_command_needs_an_element_of_its_kind() {
    let dir = TempDir::new("lib-kind");
    let mut e = engine_with_library(&dir);
    let part = new_element(&mut e, "part", "P");
    let err = e.run_err("library.symbol.edit", json!({"symbol": part, "name": "X"}));
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = e.run_err(
        "library.element.new",
        json!({"library": LIBRARY, "kind": "part", "name": "a/b"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}
