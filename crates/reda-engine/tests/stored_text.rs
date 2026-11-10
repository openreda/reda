//! Stored text through Commands and files: an unescaped `${` is refused by Commands and on
//! load, and `$${` round-trips and displays as `${`.

use std::path::Path;

use reda_engine::command::ErrorCode;
use reda_engine::format::library::{
    read_footprint, read_part, read_symbol, write_footprint, write_part, write_symbol,
};
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{
    ComponentType, Fields, Footprint, FootprintGraphic, FootprintLayer, Part, PinMap, Shape,
    Symbol, SymbolGraphic, Unit, Uuid,
};
use reda_engine::render::{Primitive, Scene};
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn new_project(dir: &Path) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    engine
}

fn add_sheet_text(
    engine: &mut TestEngine,
    words: &str,
) -> Result<Value, reda_engine::command::CommandError> {
    engine.run(
        "schematic.text.add",
        json!({"document": SHEET, "text": words, "at": {"x": "10mm", "y": "10mm"}}),
    )
}

fn assert_refused(err: &reda_engine::command::CommandError, pointer: &str) {
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
    assert_eq!(err.details.as_ref().unwrap()["pointer"], pointer, "{err:?}");
    assert!(err.message.contains("\"${\""), "{}", err.message);
    assert!(err.message.contains("\"$${\""), "{}", err.message);
}

#[test]
fn commands_refuse_an_unescaped_dollar_brace_with_a_clear_error() {
    let dir = TempDir::new("text-commands");
    let mut engine = new_project(dir.path());

    let err = add_sheet_text(&mut engine, "V${CC}").unwrap_err();
    assert_refused(&err, "/text");
    assert!(err.message.contains("character 2"), "{}", err.message);

    let err = engine.run_err(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "${x}", "x": "1mm", "y": "1mm"}),
    );
    assert_refused(&err, "/text");

    let err = engine.run_err("project.properties.set", json!({"title": "Board ${rev}"}));
    assert_refused(&err, "/title");

    let text = add_sheet_text(&mut engine, "ok").unwrap()["result"]["text"].clone();
    let err = engine.run_err("schematic.text.set", json!({"uuid": text, "text": "${"}));
    assert_refused(&err, "/text");
}

fn sheet_texts(scene: &Scene) -> Vec<String> {
    scene
        .items
        .iter()
        .filter_map(|item| match &item.primitive {
            Primitive::Glyphs(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_escaped_dollar_brace_round_trips_and_displays_as_dollar_brace() {
    let dir = TempDir::new("text-escape");
    let mut engine = new_project(dir.path());
    add_sheet_text(&mut engine, "V$${CC} costs $5").unwrap();
    engine.run_ok("project.properties.set", json!({"title": "Board $${rev}"}));
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let sheet = std::fs::read_to_string(folder.join("Demo.redasch")).unwrap();
    assert!(sheet.contains("V$${CC} costs $5"), "{sheet}");

    let mut reopened = test_engine();
    reopened.run_ok("project.open", json!({"path": folder}));
    let texts = sheet_texts(&reopened.document_scene(SHEET.parse().unwrap()).unwrap());
    assert!(texts.iter().any(|t| t == "V${CC} costs $5"), "{texts:?}");
    assert!(
        texts.iter().any(|t| t == "Title: Board ${rev}"),
        "{texts:?}"
    );
    assert!(!texts.iter().any(|t| t.contains("$${")), "{texts:?}");
}

fn saved_project(label: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new(label);
    let mut engine = new_project(dir.path());
    add_sheet_text(&mut engine, "MARK").unwrap();
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "PCBMARK", "x": "1mm", "y": "1mm"}),
    );
    engine.run_ok("project.properties.set", json!({"title": "TITLEMARK"}));
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    (dir, folder)
}

fn edit(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{text}");
    std::fs::write(path, text.replace(from, to)).unwrap();
}

#[test]
fn opening_refuses_any_file_with_an_unescaped_dollar_brace() {
    for (file, from, field) in [
        ("Demo.redasch", "MARK", "sheet_text"),
        ("Demo.redapcb", "PCBMARK", "pcb_text"),
        ("Demo.reda", "TITLEMARK", "title"),
    ] {
        let (_dir, folder) = saved_project("text-load");
        edit(&folder.join(file), &format!("\"{from}\""), "\"${x}\"");
        let err = test_engine().run_err("project.open", json!({"path": folder}));
        assert_eq!(err.code, ErrorCode::Rejected, "{file}: {err:?}");
        assert!(err.message.contains(file), "{file}: {}", err.message);
        assert!(err.message.contains(field), "{file}: {}", err.message);
        assert!(err.message.contains("\"$${\""), "{file}: {}", err.message);
    }
}

fn id(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

fn text_shape(words: &str) -> Shape {
    Shape::Text {
        at: Point::new(Nm(0), Nm(0)),
        text: words.into(),
        size: Nm(1_000_000),
        rotation: Millideg(0),
    }
}

fn symbol(words: &str) -> Symbol {
    Symbol {
        uuid: id(1),
        name: "S".into(),
        description: String::new(),
        designator_prefix: "U".into(),
        units: vec![Unit {
            uuid: id(2),
            name: "A".into(),
        }],
        pins: vec![],
        graphics: vec![SymbolGraphic {
            uuid: id(3),
            unit: id(2),
            shape: text_shape(words),
            width: Nm(100_000),
            fill: false,
        }],
        provenance: None,
    }
}

fn footprint(words: &str) -> Footprint {
    Footprint {
        uuid: id(4),
        name: "F".into(),
        description: String::new(),
        height: None,
        pads: vec![],
        graphics: vec![FootprintGraphic {
            uuid: id(5),
            layer: FootprintLayer::SilkscreenTop,
            shape: text_shape(words),
            width: Nm(100_000),
            fill: false,
        }],
        texts: vec![],
        generator: None,
        provenance: None,
    }
}

fn part(value: &str) -> Part {
    Part {
        uuid: id(6),
        name: "P".into(),
        component_type: ComponentType::Standard,
        in_bom: true,
        in_cpl: true,
        symbol: None,
        footprints: vec![],
        fields: Fields {
            value: value.into(),
            ..Fields::default()
        },
        pin_map: PinMap::default(),
        provenance: None,
    }
}

#[test]
fn library_files_refuse_an_unescaped_dollar_brace_and_keep_an_escaped_one() {
    let path = Path::new("x");
    let ok = read_symbol(path, &write_symbol(&symbol("$${A}")).unwrap()).unwrap();
    assert_eq!(ok, symbol("$${A}"));
    let err = read_symbol(path, &write_symbol(&symbol("${A}")).unwrap()).unwrap_err();
    assert!(err.to_string().contains("reserved"), "{err}");

    let ok = read_footprint(path, &write_footprint(&footprint("$${A}")).unwrap()).unwrap();
    assert_eq!(ok, footprint("$${A}"));
    let err = read_footprint(path, &write_footprint(&footprint("${A}")).unwrap()).unwrap_err();
    assert!(err.to_string().contains("reserved"), "{err}");

    let ok = read_part(path, &write_part(&part("$${A}")).unwrap()).unwrap();
    assert_eq!(ok, part("$${A}"));
    let err = read_part(path, &write_part(&part("${A}")).unwrap()).unwrap_err();
    assert!(err.to_string().contains("field value"), "{err}");
}
