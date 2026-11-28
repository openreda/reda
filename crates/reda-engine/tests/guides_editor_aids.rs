//! Guides are editor aids (docs/spec/v0.3/grids-guides.md#the-four-things): no output,
//! export or rendered image shows one, and no check reads one.

use std::path::Path;

use reda_engine::model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

/// The Output Job `project.new` makes, on seed 0.
const JOB: &str = "00000000-0000-4000-8000-00000000002f";

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "30mm", "height": "20mm"}),
    );
    engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": "5mm", "y": "5mm"}, "diameter": "0.6mm", "hole": "0.3mm"}),
    );
    engine
}

fn add_guides(engine: &mut TestEngine) -> Vec<Uuid> {
    [
        json!({"kind": "line", "at": {"x": "5mm", "y": "5mm"}, "angle": 0}),
        json!({"kind": "line", "at": {"x": "10mm", "y": "0mm"}, "angle": 45_000}),
        json!({"kind": "point", "at": {"x": "15mm", "y": "10mm"}}),
    ]
    .into_iter()
    .map(|p| {
        let out = engine.run_ok("pcb.guide.add", p);
        out["result"]["uuid"].as_str().unwrap().parse().unwrap()
    })
    .collect()
}

/// Every file a run of the job wrote under `fab/`, by name, with its bytes.
fn fab_files(engine: &mut TestEngine) -> Vec<(String, Vec<u8>)> {
    let out = engine.run_ok("output.job.run", json!({"job": JOB}));
    let folder = Path::new(out["destination"].as_str().unwrap()).to_owned();
    let mut files: Vec<(String, Vec<u8>)> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_owned())
        .filter(|p| p.starts_with("fab/") && !p.ends_with(".gbrjob"))
        .map(|p| {
            let bytes = std::fs::read(folder.join(&p)).unwrap();
            (p, bytes)
        })
        .collect();
    files.sort();
    files
}

fn render(engine: &mut TestEngine) -> Value {
    let pcb = engine.project().unwrap().pcb().unwrap().uuid;
    engine.run_ok(
        "document.render",
        json!({"document": pcb, "format": "svg", "size": 256}),
    )["svg"]
        .clone()
}

#[test]
fn the_fab_outputs_and_a_render_are_the_same_with_and_without_guides() {
    let dir = TempDir::new("guides-outputs");
    let mut engine = board(&dir);
    let before = fab_files(&mut engine);
    let image = render(&mut engine);
    assert!(image.as_str().is_some_and(|svg| svg.contains("<svg")));
    assert!(!before.is_empty());
    add_guides(&mut engine);
    assert_eq!(fab_files(&mut engine), before);
    assert_eq!(render(&mut engine), image);
}

#[test]
fn the_board_scene_holds_no_guide() {
    let dir = TempDir::new("guides-scene");
    let mut engine = board(&dir);
    let guides = add_guides(&mut engine);
    let pcb = engine.project().unwrap().pcb().unwrap().uuid;
    let scene = engine.board_scene(pcb, &Default::default()).unwrap();
    assert!(!scene.items.is_empty());
    assert!(scene.items.iter().all(|i| !guides.contains(&i.object)));
}

#[test]
fn drc_reads_no_guide() {
    let dir = TempDir::new("guides-drc");
    let mut engine = board(&dir);
    let before = engine.run_ok("drc.run", json!({}))["violations"].clone();
    assert!(before.is_array(), "{before}");
    add_guides(&mut engine);
    let after = engine.run_ok("drc.run", json!({}))["violations"].clone();
    assert_eq!(after, before);
}
