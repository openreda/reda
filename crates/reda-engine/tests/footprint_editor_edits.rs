//! The Footprint editor's selection edits: moving and deleting any mix of Pads, graphics
//! and the Designator and value text in one Command, so each is one undo step.

use reda_engine::command::ErrorCode;
use reda_engine::footprint_scene::{DESIGNATOR_TEXT, VALUE_TEXT};
use reda_engine::model::LibraryElement;
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

    e.run_ok(
        "library.footprint.move",
        json!({"footprint": fp, "objects": [pad, line, DESIGNATOR_TEXT.to_string()],
               "by": {"x": "0.5mm", "y": "-1mm"}}),
    );
    let f = footprint(&e, &fp);
    assert_eq!(f.pads[0].at.x.0, 1_500_000);
    assert_eq!(f.pads[0].at.y.0, -1_000_000);
    assert_eq!(f.designator.at.y.0, -1_000_000);
    assert_eq!(f.value.at.y.0, 0, "the value did not move");
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
fn deleting_removes_pads_and_graphics_and_hides_texts() {
    let (home, work) = (TempDir::new("fpd-home"), TempDir::new("fpd-work"));
    let (mut e, fp) = new_footprint(&home, &work);
    let a = add_pad(&mut e, &fp, "0mm");
    let b = add_pad(&mut e, &fp, "2mm");
    e.run_ok(
        "library.footprint.delete",
        json!({"footprint": fp, "objects": [a, VALUE_TEXT.to_string()]}),
    );
    let f = footprint(&e, &fp);
    assert_eq!(f.pads.len(), 1);
    assert_eq!(f.pads[0].uuid.to_string(), b);
    assert!(!f.value.visible, "deleting the value text hides it");

    let err = e.run_err(
        "library.footprint.delete",
        json!({"footprint": fp, "objects": ["00000000-0000-4000-8000-0000000000ff"]}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    let _: Value = e.run_ok("edit.undo", json!({"document": fp}));
    assert_eq!(footprint(&e, &fp).pads.len(), 2);
}
