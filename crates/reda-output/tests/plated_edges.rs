//! Plated Edges in the fab outputs (docs/spec/v0.2.5/plated-edges.md#outputs): the
//! Profile's plating, the plated-edges Gerber read back against the marked edges, the job
//! file and the Fab Notes line.

mod gerber_reader;

use gerber_reader::{Object, read};
use reda_model::{FileNaming, GerberSettings};
use reda_output::{OutputFile, Plot, fab_notes_file, gerber_files, gerber_job_file};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn mm(v: f64) -> i64 {
    (v * 1_000_000.0).round() as i64
}

fn net(engine: &mut TestEngine, name: &str) -> Value {
    engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"].clone()
}

/// A 30 × 20 mm board whose top right corner is a quarter arc, with a square Board Cutout.
/// Returns the engine, the outline and the cutout.
fn board(dir: &TempDir) -> (TestEngine, Value, Value) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let outline = engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": [
            {"x": 0, "y": 0}, {"x": "30mm", "y": 0}, {"x": "30mm", "y": "15mm", "sweep": "90deg"},
            {"x": "25mm", "y": "20mm"}, {"x": 0, "y": "20mm"}
        ]}),
    )["result"]["uuid"]
        .clone();
    let cutout = engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": [
            {"x": "10mm", "y": "5mm"}, {"x": "14mm", "y": "5mm"}, {"x": "14mm", "y": "9mm"},
            {"x": "10mm", "y": "9mm"}
        ]}),
    )["result"]["uuid"]
        .clone();
    (engine, outline, cutout)
}

fn with_plot<T>(
    engine: &TestEngine,
    set: impl FnOnce(&mut GerberSettings),
    f: impl FnOnce(&Plot<'_>) -> T,
) -> T {
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let mut gerber = project.fab_presets().next().unwrap().preset().gerber;
    set(&mut gerber);
    f(&Plot {
        project,
        instances: &[reda_output::BoardInstance::single(
            pcb,
            &fills,
            reda_model::FileOrigin::Absolute,
        )],
        gerber: &gerber,
        fonts: &reda_command::Fonts::default(),
        version: reda_testkit::TEST_VERSION,
        part: reda_output::Part::Single,
        not_fitted_paste: reda_model::NotFittedPaste::Keep,
    })
}

fn plot(engine: &TestEngine, naming: FileNaming) -> Vec<OutputFile> {
    with_plot(
        engine,
        |g| g.naming = naming,
        |plot| gerber_files(plot).unwrap().files,
    )
}

fn file<'a>(files: &'a [OutputFile], function: &str) -> Option<&'a OutputFile> {
    files.iter().find(|f| f.function == function)
}

/// The job file and the Fab Notes text of the board.
fn job_and_notes(engine: &TestEngine) -> (Value, String) {
    with_plot(
        engine,
        |_| {},
        |plot| {
            let mut files = gerber_files(plot).unwrap().files;
            let notes = fab_notes_file(plot, &files, &[]);
            files.push(notes.clone());
            let job = gerber_job_file(plot, &files);
            (
                serde_json::from_slice(&job.bytes).unwrap(),
                String::from_utf8(notes.bytes).unwrap(),
            )
        },
    )
}

fn mark(engine: &mut TestEngine, contour: &Value, edge: usize, plated: Value) {
    engine.run_ok(
        "pcb.contour.edge.set",
        json!({"contour": contour, "edge": edge, "plated": plated}),
    );
}

#[test]
fn without_plated_edges_the_profile_is_non_plated_and_there_is_no_plated_edges_file() {
    let dir = TempDir::new("plated-none");
    let (engine, _, _) = board(&dir);
    let files = plot(&engine, FileNaming::Classic);
    let profile = read(&file(&files, "profile").unwrap().bytes);
    assert_eq!(profile.file[".FileFunction"], "Profile,NP");
    assert!(file(&files, "plated_edges").is_none());
    let (job, notes) = job_and_notes(&engine);
    assert_eq!(job["GeneralSpecs"].get("EdgePlating"), None);
    assert!(!notes.contains("Edge plating"), "{notes}");
}

#[test]
fn the_plated_edges_file_reads_back_as_exactly_the_marked_edges() {
    let dir = TempDir::new("plated-readback");
    let (mut engine, outline, cutout) = board(&dir);
    mark(&mut engine, &outline, 0, json!({}));
    mark(&mut engine, &outline, 2, json!({}));
    engine.run_ok(
        "pcb.contour.edge.set_all",
        json!({"contour": cutout, "plated": {}}),
    );
    let files = plot(&engine, FileNaming::Classic);
    let profile = read(&file(&files, "profile").unwrap().bytes);
    assert_eq!(profile.file[".FileFunction"], "Profile,P");
    let plated = file(&files, "plated_edges").unwrap();
    assert_eq!(plated.name, "Demo-plated_edges.gbr");
    let layer = read(&plated.bytes);
    assert!(layer.ended);
    assert_eq!(layer.file[".FileFunction"], "OtherDrawing,Plated edges");
    assert_eq!(layer.file[".FilePolarity"], "Positive");
    assert_eq!(layer.flashes().count() + layer.regions().count(), 0);
    let mut draws: Vec<_> = layer
        .draws()
        .map(|d| match d {
            Object::Draw { from, to, arc, .. } => (*from, *to, arc.map(|a| (a.centre, a.ccw))),
            _ => unreachable!(),
        })
        .collect();
    draws.sort();
    let p = |x: f64, y: f64| (mm(x), mm(y));
    let mut expected = vec![
        // The outline's bottom edge and its quarter arc about (25, 15).
        (p(0.0, 0.0), p(30.0, 0.0), None),
        (p(30.0, 15.0), p(25.0, 20.0), Some((p(25.0, 15.0), true))),
        // Every edge of the cutout.
        (p(10.0, 5.0), p(14.0, 5.0), None),
        (p(14.0, 5.0), p(14.0, 9.0), None),
        (p(14.0, 9.0), p(10.0, 9.0), None),
        (p(10.0, 9.0), p(10.0, 5.0), None),
    ];
    expected.sort();
    assert_eq!(draws, expected);
}

#[test]
fn the_plated_edges_file_keeps_its_name_under_both_naming_schemes() {
    let dir = TempDir::new("plated-names");
    let (mut engine, outline, _) = board(&dir);
    mark(&mut engine, &outline, 0, json!({}));
    for naming in [FileNaming::Classic, FileNaming::Descriptive] {
        let files = plot(&engine, naming);
        assert_eq!(
            file(&files, "plated_edges").unwrap().name,
            "Demo-plated_edges.gbr",
            "{naming:?}"
        );
    }
}

#[test]
fn the_plated_edges_file_holds_no_outline_even_when_every_plot_gets_one() {
    let dir = TempDir::new("plated-outline-every");
    let (mut engine, outline, _) = board(&dir);
    mark(&mut engine, &outline, 0, json!({}));
    let files = with_plot(
        &engine,
        |g| g.outline_on_every_plot = true,
        |plot| gerber_files(plot).unwrap().files,
    );
    let layer = read(&file(&files, "plated_edges").unwrap().bytes);
    assert_eq!(layer.draws().count(), 1);
}

#[test]
fn the_job_file_sets_edge_plating_and_lists_the_plated_edges_file() {
    let dir = TempDir::new("plated-job");
    let (mut engine, outline, _) = board(&dir);
    mark(&mut engine, &outline, 0, json!({}));
    let (job, _) = job_and_notes(&engine);
    assert_eq!(job["GeneralSpecs"]["EdgePlating"], json!(true));
    let entry = job["FilesAttributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["Path"] == "Demo-plated_edges.gbr")
        .expect("the job file lists the plated edges file");
    assert_eq!(entry["FileFunction"], "OtherDrawing,Plated edges");
    assert_eq!(entry["FilePolarity"], "Positive");
}

#[test]
fn the_fab_notes_name_the_file_and_the_nets() {
    let dir = TempDir::new("plated-notes");
    let (mut engine, outline, cutout) = board(&dir);
    let gnd = net(&mut engine, "GND");
    mark(&mut engine, &cutout, 1, json!({"net": gnd}));
    let (_, notes) = job_and_notes(&engine);
    assert!(
        notes.contains(
            "Edge plating on the edges drawn in Demo-plated_edges.gbr; plated edges carry net GND"
        ),
        "{notes}"
    );
    assert!(notes.contains("  Demo-plated_edges.gbr\n"), "{notes}");
    // The whole outline plated, and nothing else.
    engine.run_ok(
        "pcb.contour.edge.set",
        json!({"contour": cutout, "edge": 1, "plated": null}),
    );
    engine.run_ok(
        "pcb.contour.edge.set_all",
        json!({"contour": outline, "plated": {"net": gnd}}),
    );
    let (_, notes) = job_and_notes(&engine);
    assert!(
        notes.contains("Edge plating on the whole outline, drawn in Demo-plated_edges.gbr; plated edges carry net GND"),
        "{notes}"
    );
}
