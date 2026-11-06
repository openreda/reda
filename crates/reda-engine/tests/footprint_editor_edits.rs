//! The Footprint editor's edits: moving and deleting any mix of Pads, graphics and texts
//! in one Command, so each is one undo step; the field texts; the closed courtyard; and
//! the height.

use reda_engine::command::ErrorCode;
use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{FootprintLayer, LibraryElement};
use reda_testkit::{TempDir, TestEngine, test_engine_in};
use serde_json::{Value, json};

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

fn new_footprint(home: &TempDir, work: &TempDir) -> (TestEngine, String) {
    let lib = work.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let info = reda_engine::model::LibraryInfo {
        uuid: LIBRARY.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    let mut e = test_engine_in(home.path());
    e.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [lib]}),
    );
    let out = e.run_ok(
        "library.element.new",
        json!({"library": LIBRARY, "kind": "footprint", "name": "Test"}),
    );
    (e, out["element"].as_str().unwrap().to_owned())
}

fn add_pad(e: &mut TestEngine, fp: &str, x: &str) -> String {
    let out = e.run_ok(
        "library.footprint.pad.add",
        json!({"footprint": fp, "number": "1", "kind": "smd", "shape": "rect",
               "at": {"x": x, "y": "0mm"}, "width": "1mm", "height": "1mm"}),
    );
    out["result"]["pad"].as_str().unwrap().to_owned()
}

fn footprint(e: &TestEngine, fp: &str) -> reda_engine::model::Footprint {
    match e.element(fp.parse().unwrap()) {
        Some(LibraryElement::Footprint(f)) => f.clone(),
        _ => panic!("open"),
    }
}

/// The UUID of the new Footprint's text showing `field`.
fn text_of(e: &TestEngine, fp: &str, field: &str) -> String {
    footprint(e, fp)
        .texts
        .iter()
        .find(|t| t.field == field)
        .unwrap_or_else(|| panic!("a {field} text"))
        .uuid
        .to_string()
}

#[test]
fn moving_pads_graphics_and_texts_is_one_undo_step() {
    let (home, work) = (TempDir::new("fpe-home"), TempDir::new("fpe-work"));
    let (mut e, fp) = new_footprint(&home, &work);
    let pad = add_pad(&mut e, &fp, "1mm");
    let line = e.run_ok(
        "library.footprint.graphic.add",
        json!({"footprint": fp, "layer": "silkscreen_top",
               "shape": {"kind": "line", "start": {"x": 0, "y": 0}, "end": {"x": "1mm", "y": 0}}}),
    )["result"]["graphic"]
        .clone();

    let designator = text_of(&e, &fp, "Designator");
    e.run_ok(
        "library.footprint.move",
        json!({"footprint": fp, "objects": [pad, line, designator],
               "by": {"x": "0.5mm", "y": "-1mm"}}),
    );
    let f = footprint(&e, &fp);
    assert_eq!(f.pads[0].at.x.0, 1_500_000);
    assert_eq!(f.pads[0].at.y.0, -1_000_000);
    let at = |field: &str| f.texts.iter().find(|t| t.field == field).unwrap().at;
    assert_eq!(at("Designator").y.0, -1_000_000);
    assert_eq!(at("Value").y.0, 0, "the value did not move");
    match &f.graphics[0].shape {
        reda_engine::model::Shape::Line { start, end } => {
            assert_eq!((start.x.0, end.x.0), (500_000, 1_500_000));
        }
        other => panic!("{other:?}"),
    }
    e.run_ok("edit.undo", json!({"document": fp}));
    assert_eq!(footprint(&e, &fp).pads[0].at.x.0, 1_000_000);
}

#[test]
fn deleting_removes_pads_graphics_and_texts() {
    let (home, work) = (TempDir::new("fpd-home"), TempDir::new("fpd-work"));
    let (mut e, fp) = new_footprint(&home, &work);
    let a = add_pad(&mut e, &fp, "0mm");
    let b = add_pad(&mut e, &fp, "2mm");
    let value = text_of(&e, &fp, "Value");
    e.run_ok(
        "library.footprint.delete",
        json!({"footprint": fp, "objects": [a, value]}),
    );
    let f = footprint(&e, &fp);
    assert_eq!(f.pads.len(), 1);
    assert_eq!(f.pads[0].uuid.to_string(), b);
    let fields: Vec<&str> = f.texts.iter().map(|t| t.field.as_str()).collect();
    assert_eq!(fields, ["Designator"], "deleting the value text removes it");

    let err = e.run_err(
        "library.footprint.delete",
        json!({"footprint": fp, "objects": ["00000000-0000-4000-8000-0000000000ff"]}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    let _: Value = e.run_ok("edit.undo", json!({"document": fp}));
    assert_eq!(footprint(&e, &fp).pads.len(), 2);
}

#[test]
fn a_new_footprint_has_a_silkscreen_designator_and_an_assembly_value() {
    let (home, work) = (TempDir::new("fpt-home"), TempDir::new("fpt-work"));
    let (e, fp) = new_footprint(&home, &work);
    let f = footprint(&e, &fp);
    let mut texts: Vec<(&str, FootprintLayer)> = f
        .texts
        .iter()
        .map(|t| (t.field.as_str(), t.layer))
        .collect();
    texts.sort_by_key(|(field, _)| *field);
    assert_eq!(
        texts,
        [
            ("Designator", FootprintLayer::SilkscreenTop),
            ("Value", FootprintLayer::AssemblyTop)
        ]
    );
    assert_ne!(f.texts[0].uuid, f.texts[1].uuid);
}

#[test]
fn field_texts_are_added_edited_and_removed() {
    let (home, work) = (TempDir::new("fpx-home"), TempDir::new("fpx-work"));
    let (mut e, fp) = new_footprint(&home, &work);
    let added = e.run_ok(
        "library.footprint.text.add",
        json!({"footprint": fp, "field": "Designator", "layer": "assembly_top",
               "at": {"x": 0, "y": "-1mm"}, "height": "0.5mm"}),
    )["result"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    let f = footprint(&e, &fp);
    assert_eq!(f.texts.len(), 3, "a Silkscreen and an Assembly Designator");
    let text = f
        .texts
        .iter()
        .find(|t| t.uuid.to_string() == added)
        .unwrap();
    assert_eq!(text.layer, FootprintLayer::AssemblyTop);
    assert_eq!(text.style.height, Nm::from_um(500));
    assert_eq!(text.style.thickness, Nm::from_um(75), "15% of the height");
    assert!(text.visible);

    e.run_ok(
        "library.footprint.text.edit",
        json!({"footprint": fp, "text": added, "field": "MPN", "rotation": "90deg",
               "thickness": "0.1mm", "visible": false}),
    );
    let f = footprint(&e, &fp);
    let text = f
        .texts
        .iter()
        .find(|t| t.uuid.to_string() == added)
        .unwrap();
    assert_eq!(text.field, "MPN");
    assert_eq!(text.rotation, Millideg::from_deg(90));
    assert_eq!(text.style.thickness, Nm::from_um(100));
    assert_eq!(text.style.height, Nm::from_um(500), "kept");
    assert!(!text.visible);

    e.run_ok(
        "library.footprint.text.edit",
        json!({"footprint": fp, "text": added, "height": "1mm"}),
    );
    let f = footprint(&e, &fp);
    let text = f
        .texts
        .iter()
        .find(|t| t.uuid.to_string() == added)
        .unwrap();
    assert_eq!(
        text.style.thickness,
        Nm::from_um(200),
        "the pen keeps its proportion to the height"
    );

    let refused = e.run_err(
        "library.footprint.text.edit",
        json!({"footprint": fp, "text": added, "layer": "courtyard_top"}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);
    let refused = e.run_err(
        "library.footprint.text.add",
        json!({"footprint": fp, "field": " ", "at": {"x": 0, "y": 0}}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);

    e.run_ok(
        "library.footprint.text.remove",
        json!({"footprint": fp, "text": added}),
    );
    assert_eq!(footprint(&e, &fp).texts.len(), 2);
    let missing = e.run_err(
        "library.footprint.text.remove",
        json!({"footprint": fp, "text": added}),
    );
    assert_eq!(missing.code, ErrorCode::NotFound);
    e.run_ok("edit.undo", json!({"document": fp}));
    assert_eq!(footprint(&e, &fp).texts.len(), 3);
}

#[test]
fn a_courtyard_layer_refuses_open_shapes() {
    let (home, work) = (TempDir::new("fpc-home"), TempDir::new("fpc-work"));
    let (mut e, fp) = new_footprint(&home, &work);
    let line = json!({"kind": "line", "start": {"x": 0, "y": 0}, "end": {"x": "1mm", "y": 0}});
    let arc = json!({"kind": "arc", "center": {"x": 0, "y": 0}, "start": {"x": "1mm", "y": 0},
                     "sweep": "90deg"});
    let text = json!({"kind": "text", "at": {"x": 0, "y": 0}, "text": "X", "size": "1mm"});
    for shape in [&line, &arc, &text] {
        let refused = e.run_err(
            "library.footprint.graphic.add",
            json!({"footprint": fp, "layer": "courtyard_bottom", "shape": shape}),
        );
        assert_eq!(refused.code, ErrorCode::InvalidParams, "{shape}");
    }
    let rect = json!({"kind": "rect", "start": {"x": "-1mm", "y": "-1mm"},
                      "end": {"x": "1mm", "y": "1mm"}});
    let circle = json!({"kind": "circle", "center": {"x": 0, "y": 0}, "radius": "1mm"});
    let polygon = json!({"kind": "polygon",
                         "points": [{"x": 0, "y": 0}, {"x": "1mm", "y": 0}, {"x": 0, "y": "1mm"}]});
    for shape in [&rect, &circle, &polygon] {
        e.run_ok(
            "library.footprint.graphic.add",
            json!({"footprint": fp, "layer": "courtyard_top", "shape": shape}),
        );
    }

    // Editing can't open a courtyard either: not by its shape, nor by moving a line onto it.
    let courtyard = footprint(&e, &fp).graphics[0].uuid.to_string();
    let refused = e.run_err(
        "library.footprint.graphic.edit",
        json!({"footprint": fp, "graphic": courtyard, "shape": line}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);
    let silk = e.run_ok(
        "library.footprint.graphic.add",
        json!({"footprint": fp, "layer": "silkscreen_top", "shape": line}),
    )["result"]["graphic"]
        .clone();
    let refused = e.run_err(
        "library.footprint.graphic.edit",
        json!({"footprint": fp, "graphic": silk, "layer": "courtyard_top"}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);
    assert_eq!(footprint(&e, &fp).graphics.len(), 4);
}

#[test]
fn the_height_is_set_and_cleared() {
    let (home, work) = (TempDir::new("fph-home"), TempDir::new("fph-work"));
    let (mut e, fp) = new_footprint(&home, &work);
    assert_eq!(footprint(&e, &fp).height, None);
    e.run_ok(
        "library.footprint.edit",
        json!({"footprint": fp, "height": "1.1mm"}),
    );
    assert_eq!(footprint(&e, &fp).height, Some(Nm::from_um(1100)));
    let refused = e.run_err(
        "library.footprint.edit",
        json!({"footprint": fp, "height": 0}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);
    e.run_ok(
        "library.footprint.edit",
        json!({"footprint": fp, "height": null}),
    );
    assert_eq!(footprint(&e, &fp).height, None);
}
