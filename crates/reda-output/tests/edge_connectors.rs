//! Edge Connector Pads and Castellated Pads in the fab outputs
//! (docs/spec/v0.2.5/edge-connectors.md): `ConnectorPad` and `CastellatedPad` on copper,
//! `CastellatedDrill` in the drill file, and the job file's `EdgeConnector` and
//! `Castellated` keys.

mod gerber_reader;

use gerber_reader::{Layer, Object, read};
use reda_output::{OutputFile, Plot, drill_files, gerber_files, gerber_job_file};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn mm(v: f64) -> i64 {
    (v * 1_000_000.0).round() as i64
}

fn pt(x: f64, y: f64) -> Value {
    json!({"x": mm(x), "y": mm(y)})
}

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine
}

/// Fingers on both sides of the left edge, a normal SMD Pad, a castellated Footprint Pad
/// and a castellated Free Pad on the right edge.
fn card(dir: &TempDir) -> TestEngine {
    let mut engine = board(dir);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(0.0, 0.0), "designator": "J1", "pads": [
            {"at": pt(0.5, 5.0), "width": "1mm", "height": "3mm", "role": "edge_connector"},
            {"at": pt(0.5, 5.0), "width": "1mm", "height": "3mm", "role": "edge_connector",
             "side": "bottom"},
            {"at": pt(5.0, 5.0), "width": "1mm", "height": "1mm"},
            {"at": pt(30.0, 5.0), "width": "1.6mm", "height": "1.6mm", "shape": "circle",
             "kind": "plated_through_hole", "castellated": true,
             "hole": {"width": mm(1.0), "height": mm(1.0)}},
            {"at": pt(20.0, 5.0), "width": "1.6mm", "height": "1.6mm", "shape": "circle",
             "kind": "plated_through_hole", "hole": {"width": mm(1.0), "height": mm(1.0)}}]}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "at": pt(30.0, 15.0), "width": "1.6mm",
               "hole": {"width": "0.8mm"}, "castellated": true}),
    );
    engine
}

fn with_plot<T>(engine: &TestEngine, f: impl FnOnce(&Plot<'_>) -> T) -> T {
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let gerber = project.fab_presets().next().unwrap().preset().gerber;
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

fn files(engine: &TestEngine) -> Vec<OutputFile> {
    with_plot(engine, |plot| {
        let mut files = gerber_files(plot).unwrap().files;
        files.extend(drill_files(plot).files);
        let job = gerber_job_file(plot, &files);
        files.push(job);
        files
    })
}

fn layer(files: &[OutputFile], name: &str) -> Layer {
    let file = files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no {name}"));
    read(&file.bytes)
}

fn function_at(layer: &Layer, x: f64, y: f64) -> Option<String> {
    layer.flashes().find_map(|o| match o {
        Object::Flash { at, function, .. } if *at == (mm(x), mm(y)) => function.clone(),
        _ => None,
    })
}

#[test]
fn edge_connector_pads_plot_as_connector_pads_on_both_outer_layers() {
    let dir = TempDir::new("edge-connector-gerber");
    let files = files(&card(&dir));
    for name in ["Demo.gtl", "Demo.gbl"] {
        let copper = layer(&files, name);
        assert_eq!(
            function_at(&copper, 0.5, 5.0).as_deref(),
            Some("ConnectorPad"),
            "{name}"
        );
    }
    let top = layer(&files, "Demo.gtl");
    assert_eq!(function_at(&top, 5.0, 5.0).as_deref(), Some("SMDPad,CuDef"));
}

#[test]
fn castellated_pads_plot_as_castellated_copper_and_drill() {
    let dir = TempDir::new("castellated-gerber");
    let files = files(&card(&dir));
    for name in ["Demo.gtl", "Demo.gbl"] {
        let copper = layer(&files, name);
        assert_eq!(
            function_at(&copper, 30.0, 5.0).as_deref(),
            Some("CastellatedPad"),
            "{name}: Footprint Pad"
        );
        assert_eq!(
            function_at(&copper, 30.0, 15.0).as_deref(),
            Some("CastellatedPad"),
            "{name}: Free Pad"
        );
        assert_eq!(
            function_at(&copper, 20.0, 5.0).as_deref(),
            Some("ComponentPad"),
            "{name}: a plain plated Pad"
        );
    }
    let drill = files
        .iter()
        .find(|f| f.function == "drill_plated")
        .expect("a plated drill file");
    let text = String::from_utf8(drill.bytes.clone()).unwrap();
    // The castellated holes get tools of their own, apart from the plain Pad's of the
    // same 1.0 mm size.
    let tools: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("; #@! TA.AperFunction,"))
        .collect();
    assert_eq!(
        tools,
        ["CastellatedDrill", "CastellatedDrill", "ComponentDrill"],
        "{text}"
    );
}

fn general_specs(files: &[OutputFile]) -> Value {
    let job = files.iter().find(|f| f.name == "Demo-job.gbrjob").unwrap();
    serde_json::from_slice::<Value>(&job.bytes).unwrap()["GeneralSpecs"].clone()
}

#[test]
fn the_job_file_says_when_the_board_has_an_edge_connector_or_castellations() {
    let dir = TempDir::new("edge-connector-job");
    let specs = general_specs(&files(&card(&dir)));
    assert_eq!(specs["EdgeConnector"], true);
    assert_eq!(specs["Castellated"], true);

    let plain = TempDir::new("plain-job");
    let specs = general_specs(&files(&board(&plain)));
    assert!(specs.get("EdgeConnector").is_none(), "{specs}");
    assert!(specs.get("Castellated").is_none(), "{specs}");
}

fn fab_notes(engine: &TestEngine) -> String {
    with_plot(engine, |plot| {
        let mut files = gerber_files(plot).unwrap().files;
        files.extend(drill_files(plot).files);
        String::from_utf8(reda_output::fab_notes_file(plot, &files, &[]).bytes).unwrap()
    })
}

/// The card with `edge_connector {bevel {20°, 1.30}, hard_gold}` and an ENIG finish.
fn bevelled_card(dir: &TempDir) -> TestEngine {
    let mut engine = card(dir);
    engine.run_ok("pcb.stackup.set", json!({"finish": "ENIG"}));
    engine.run_ok(
        "pcb.edge_connector.set",
        json!({"edge_connector": {"bevel": {"angle": "20deg", "depth": "1.3mm"},
                                  "hard_gold": true}}),
    );
    engine
}

#[test]
fn the_job_file_carries_the_bevel_in_its_notes_and_the_hard_gold_area() {
    let dir = TempDir::new("edge-connector-bevel-job");
    let specs = general_specs(&files(&bevelled_card(&dir)));
    assert_eq!(specs["EdgeConnector"], true);
    assert_eq!(specs["EdgeConnectorBevelled"], true);
    assert_eq!(
        specs["Notes"], "Bevel edge-connector edge: 20°, 1.30 mm",
        "{specs}"
    );
    // Two 1 × 3 mm fingers, one on each side.
    assert_eq!(specs["HardGoldArea"], 6.0, "{specs}");
    // Hard gold is selective: the board finish stays ENIG.
    assert_eq!(specs["Finish"], "ENIG");

    let plain = TempDir::new("edge-connector-plain-job");
    let specs = general_specs(&files(&card(&plain)));
    for key in ["EdgeConnectorBevelled", "Notes", "HardGoldArea"] {
        assert!(specs.get(key).is_none(), "{key}: {specs}");
    }
}

#[test]
fn hard_gold_alone_gives_its_area_and_no_bevel() {
    let dir = TempDir::new("edge-connector-gold-job");
    let mut engine = card(&dir);
    engine.run_ok(
        "pcb.edge_connector.set",
        json!({"edge_connector": {"hard_gold": true}}),
    );
    let specs = general_specs(&files(&engine));
    assert_eq!(specs["HardGoldArea"], 6.0, "{specs}");
    assert!(specs.get("EdgeConnectorBevelled").is_none(), "{specs}");
    assert!(specs.get("Notes").is_none(), "{specs}");
    let notes = fab_notes(&engine);
    assert!(
        notes.contains("  Hard gold on edge-connector Pads, 6.00 mm²\n"),
        "{notes}"
    );
    assert!(!notes.contains("Bevel"), "{notes}");
}

#[test]
fn fab_notes_name_the_bevel_and_hard_gold_beside_the_unchanged_finish() {
    let dir = TempDir::new("edge-connector-fab-notes");
    let notes = fab_notes(&bevelled_card(&dir));
    let board: Vec<&str> = notes
        .lines()
        .skip_while(|l| *l != "Board")
        .take_while(|l| !l.is_empty())
        .map(str::trim_start)
        .collect();
    let finish = board
        .iter()
        .position(|l| *l == "Finish: ENIG")
        .expect("the finish line");
    assert_eq!(
        board[finish + 1..finish + 3],
        [
            "Bevel edge-connector edge: 20°, 1.30 mm",
            "Hard gold on edge-connector Pads, 6.00 mm²"
        ],
        "{notes}"
    );
}

#[test]
fn hard_gold_writes_no_goldmask_gerber() {
    let dir = TempDir::new("edge-connector-no-goldmask");
    let plain = TempDir::new("edge-connector-no-goldmask-plain");
    let names = |engine: &TestEngine| -> Vec<String> {
        files(engine).into_iter().map(|f| f.name).collect()
    };
    let with_gold = names(&bevelled_card(&dir));
    assert_eq!(names(&card(&plain)), with_gold);
    assert!(
        !with_gold.iter().any(|n| n.to_lowercase().contains("gold")),
        "{with_gold:?}"
    );
}
