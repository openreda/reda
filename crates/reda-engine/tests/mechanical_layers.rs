//! Mechanical Layers in the format-3 `.redapcb`: a new board's two, and what opening a
//! file refuses. Each refusal is a `project.open` that fails and writes nothing.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_engine::model::{LayerKind, Purpose};
use reda_testkit::{TempDir, TestEngine, read_tree, test_engine};
use serde_json::{Value, json};

/// A saved new Project, and the path of its PCB file.
fn saved(label: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new(label);
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok("project.save", json!({}));
    let path = dir.path().join("Demo/Demo.redapcb");
    (dir, path)
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn layers(file: &mut Value) -> &mut Vec<Value> {
    file["layers"]["layers"].as_array_mut().unwrap()
}

fn position(file: &mut Value, kind: &str) -> usize {
    layers(file).iter().position(|l| l["kind"] == kind).unwrap()
}

/// Rewrites the PCB file through `change` and opens the Project. A refusal must leave
/// every file in the folder as it was.
fn open_after(path: &Path, change: impl FnOnce(&mut Value)) -> Result<TestEngine, String> {
    let mut file = read(path);
    change(&mut file);
    std::fs::write(path, serde_json::to_vec_pretty(&file).unwrap()).unwrap();
    let folder = path.parent().unwrap();
    let before = read_tree(folder);
    let mut engine = test_engine();
    match engine.run("project.open", json!({"path": folder})) {
        Ok(_) => Ok(engine),
        Err(err) => {
            assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
            assert_eq!(read_tree(folder), before, "a refused open wrote something");
            Err(err.message)
        }
    }
}

fn refused(label: &str, change: impl FnOnce(&mut Value)) -> String {
    let (_dir, path) = saved(label);
    open_after(&path, change).err().expect("refused")
}

#[test]
fn a_new_board_starts_with_unsided_dimensions_and_reference_layers_after_the_outline() {
    let (_dir, path) = saved("new-board-mechanical");
    let mut file = read(&path);
    assert_eq!(file["format"], 3);
    let outline = position(&mut file, "board_outline");
    let tail: Vec<Value> = layers(&mut file)[outline + 1..]
        .iter()
        .map(|l| {
            let mut l = l.clone();
            l.as_object_mut().unwrap().remove("uuid");
            l
        })
        .collect();
    assert_eq!(
        tail,
        [
            json!({"kind":"mechanical","name":"Dimensions","purpose":"dimensions"}),
            json!({"kind":"mechanical","name":"Reference","purpose":"reference"}),
        ]
    );
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("documentation")
    );

    let engine = open_after(&path, |_| {}).unwrap();
    let pcb = engine.project().unwrap().pcb().unwrap();
    let mechanical: Vec<_> = pcb
        .layers()
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Mechanical)
        .map(|l| (l.name.as_deref(), l.purpose, l.side))
        .collect();
    assert_eq!(
        mechanical,
        [
            (Some("Dimensions"), Purpose::Dimensions, None),
            (Some("Reference"), Purpose::Reference, None),
        ]
    );
}

#[test]
fn a_board_may_have_no_mechanical_layer() {
    let (_dir, path) = saved("no-mechanical");
    let engine = open_after(&path, |f| layers(f).retain(|l| l["kind"] != "mechanical")).unwrap();
    let pcb = engine.project().unwrap().pcb().unwrap();
    assert!(
        pcb.layers()
            .layers
            .iter()
            .all(|l| l.kind != LayerKind::Mechanical)
    );
}

#[test]
fn a_sided_general_mechanical_layer_opens_and_saves_without_a_purpose() {
    let (_dir, path) = saved("sided-general");
    let engine = open_after(&path, |f| {
        layers(f).push(
            json!({"uuid":"00000000-0000-4000-8000-0000000fff01","kind":"mechanical",
            "name":"Glue","side":"bottom"}),
        );
    })
    .unwrap();
    let pcb = engine.project().unwrap().pcb().unwrap();
    let glue = pcb.layers().layers.last().unwrap();
    assert_eq!(glue.purpose, Purpose::General);
    assert_eq!(glue.side, Some(reda_engine::model::Side::Bottom));
    let files = reda_engine::store::encode(engine.project().unwrap()).unwrap();
    let (_, bytes) = files.iter().find(|(f, _)| f == "Demo.redapcb").unwrap();
    let saved: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(
        saved["layers"]["layers"]
            .as_array()
            .unwrap()
            .last()
            .unwrap(),
        &json!({"uuid":"00000000-0000-4000-8000-0000000fff01","kind":"mechanical","side":"bottom","name":"Glue"})
    );
}

#[test]
fn purpose_is_refused_on_a_layer_that_is_not_mechanical() {
    for kind in [
        "copper",
        "silkscreen",
        "courtyard",
        "assembly",
        "board_outline",
    ] {
        let err = refused("purpose-elsewhere", |f| {
            let i = position(f, kind);
            layers(f)[i]["purpose"] = json!("dimensions");
        });
        assert!(err.contains("purpose"), "{kind}: {err}");
    }
}

#[test]
fn general_is_left_out_rather_than_written_on_a_layer() {
    let err = refused("purpose-general", |f| {
        let i = position(f, "mechanical");
        layers(f)[i]["purpose"] = json!("general");
    });
    assert!(err.contains("general"), "{err}");
}

#[test]
fn a_duplicate_mechanical_layer_name_is_refused() {
    let err = refused("duplicate-name", |f| {
        let i = position(f, "mechanical");
        layers(f)[i + 1]["name"] = json!("Dimensions");
    });
    assert!(err.contains("Dimensions"), "{err}");
}

#[test]
fn a_mechanical_layer_needs_a_name() {
    for name in [None, Some("  ")] {
        let err = refused("unnamed", |f| {
            let i = position(f, "mechanical");
            let layer = layers(f)[i].as_object_mut().unwrap();
            match name {
                None => layer.remove("name"),
                Some(n) => layer.insert("name".into(), json!(n)),
            };
        });
        assert!(err.contains("name"), "{name:?}: {err}");
    }
}

#[test]
fn mechanical_layers_sit_after_the_board_outline() {
    let err = refused("before-outline", |f| {
        let i = position(f, "mechanical");
        let layer = layers(f).remove(i);
        layers(f).insert(0, layer);
    });
    assert!(err.contains("Board Outline"), "{err}");
    let err = refused("after-mechanical", |f| {
        let i = position(f, "board_outline");
        let outline = layers(f).remove(i);
        layers(f).push(outline);
    });
    assert!(err.contains("Board Outline"), "{err}");
}

#[test]
fn the_documentation_kind_is_gone() {
    let err = refused("documentation-kind", |f| {
        let i = position(f, "mechanical");
        layers(f)[i + 1]["kind"] = json!("documentation");
        layers(f)[i + 1].as_object_mut().unwrap().remove("purpose");
    });
    assert!(err.contains("documentation"), "{err}");
}
