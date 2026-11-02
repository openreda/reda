//! Excellon drill files: read back (with a reader of our own) to the board's holes, and
//! byte-exact goldens.

mod drill_reader;

use drill_reader::{Hit, read};
use reda_command::Timestamp;
use reda_output::{OutputFile, Plot, drill_files};
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

fn net(engine: &mut TestEngine, name: &str) -> Value {
    engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"].clone()
}

fn plot_with(engine: &TestEngine, x2_attributes: bool) -> Vec<OutputFile> {
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let mut gerber = project.fab_presets().next().unwrap().preset().gerber;
    gerber.x2_attributes = x2_attributes;
    drill_files(&Plot {
        project,
        pcb,
        fills: &fills,
        gerber,
        created: Timestamp(1_767_225_600_000),
    })
}

fn plot(engine: &TestEngine) -> Vec<OutputFile> {
    plot_with(engine, true)
}

fn file<'a>(files: &'a [OutputFile], name: &str) -> &'a OutputFile {
    files.iter().find(|f| f.name == name).unwrap_or_else(|| {
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        panic!("no {name} in {names:?}")
    })
}

fn hit(diameter: f64, at: (f64, f64), function: &str) -> Hit {
    Hit {
        diameter: mm(diameter),
        at: (mm(at.0), mm(at.1)),
        to: None,
        function: Some(function.into()),
    }
}

fn slot(width: f64, from: (f64, f64), to: (f64, f64), function: &str) -> Hit {
    Hit {
        to: Some((mm(to.0), mm(to.1))),
        ..hit(width, from, function)
    }
}

fn sorted(mut hits: Vec<Hit>) -> Vec<Hit> {
    hits.sort();
    hits
}

#[test]
fn plated_and_non_plated_holes_go_to_separate_files() {
    let dir = TempDir::new("drill-split");
    let mut engine = board(&dir);
    let gnd = net(&mut engine, "GND");
    engine.run_ok(
        "pcb.via.add",
        json!({"at": pt(5.0, 5.0), "net": gnd, "diameter": "0.6mm", "hole": "0.3mm"}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "pads": [
            {"at": pt(-1.27, 0.0), "width": "1.6mm", "height": "1.6mm", "shape": "circle",
             "kind": "plated_through_hole", "hole": {"width": mm(0.8), "height": mm(0.8)}},
            {"at": pt(1.27, 0.0), "width": "1mm", "height": "1mm"}]}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "circle", "at": pt(25.0, 5.0),
               "width": "1.6mm", "hole": {"width": "1mm"}, "net": gnd}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "circle", "at": pt(3.5, 16.5),
               "width": "3.2mm", "hole": {"width": "3.2mm"}}),
    );
    let files = plot(&engine);
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Demo-PTH.drl", "Demo-NPTH.drl"]);
    assert_eq!(files[0].function, "drill_plated");
    assert_eq!(files[1].function, "drill_non_plated");

    let pth = read(&file(&files, "Demo-PTH.drl").bytes);
    assert!(pth.metric && pth.ended);
    assert_eq!(pth.file["FileFunction"], "Plated,1,2,PTH");
    assert_eq!(
        sorted(pth.hits),
        sorted(vec![
            hit(0.3, (5.0, 5.0), "Plated,PTH,ViaDrill"),
            hit(0.8, (8.73, 10.0), "Plated,PTH,ComponentDrill"),
            hit(1.0, (25.0, 5.0), "Plated,PTH,MechanicalDrill"),
        ])
    );

    let npth = read(&file(&files, "Demo-NPTH.drl").bytes);
    assert!(npth.metric && npth.ended);
    assert_eq!(npth.file["FileFunction"], "NonPlated,1,2,NPTH");
    assert_eq!(
        npth.hits,
        [hit(3.2, (3.5, 16.5), "NonPlated,NPTH,MechanicalDrill")]
    );
}

/// A small board with every kind of hole: Vias, round and slotted Footprint Pads (one
/// turned off the axes), plated and non-plated Free Pads.
fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo",
               "board": {"width": "30mm", "height": "20mm", "corner_radius": "1mm"}}),
    );
    engine.run_ok("project.properties.set", json!({"revision": "A, first"}));
    let gnd = net(&mut engine, "GND");
    let sig = net(&mut engine, "SIG");
    let round = |x: f64, net: &Value| {
        json!({"at": pt(x, 0.0), "width": "1.7mm", "height": "1.7mm", "shape": "circle",
               "kind": "plated_through_hole", "net": net,
               "hole": {"width": mm(1.0), "height": mm(1.0)}})
    };
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(6.0, 10.0), "pads": [round(-1.27, &gnd), round(1.27, &sig)]}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(16.0, 10.0), "rotation": "30deg", "pads": [
            {"at": pt(0.0, 0.0), "width": "3mm", "height": "1.6mm", "shape": "oval",
             "kind": "plated_through_hole", "net": sig,
             "hole": {"width": mm(2.2), "height": mm(0.8)}}]}),
    );
    engine.run_ok("pcb.via.add", json!({"at": pt(10.0, 6.0), "net": sig}));
    engine.run_ok("pcb.via.add", json!({"at": pt(22.5, 12.25), "net": gnd}));
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "circle", "at": pt(26.0, 4.0),
               "width": "5.5mm", "hole": {"width": "3.2mm"}, "net": gnd}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "circle", "at": pt(4.0, 16.0),
               "width": "3.2mm", "hole": {"width": "3.2mm"}}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "oval", "at": pt(26.0, 16.0),
               "width": "1mm", "height": "2.5mm",
               "hole": {"width": "1mm", "height": "2.5mm"}}),
    );
    engine
}

#[test]
fn the_files_match_their_goldens_byte_for_byte() {
    let dir = TempDir::new("drill-golden");
    let engine = demo(&dir);
    for f in plot(&engine) {
        reda_testkit::assert_golden(
            format!(
                "{}/tests/golden/drill/{}",
                env!("CARGO_MANIFEST_DIR"),
                f.name
            ),
            &f.bytes,
        );
    }
}

#[test]
fn without_x2_attributes_the_files_carry_no_attribute_comments() {
    let dir = TempDir::new("drill-x1");
    let engine = demo(&dir);
    let with = plot(&engine);
    for (x1, x2) in plot_with(&engine, false).iter().zip(&with) {
        let text = String::from_utf8(x1.bytes.clone()).unwrap();
        assert!(!text.contains("#@!"), "{} has attributes", x1.name);
        let (a, b) = (read(&x1.bytes), read(&x2.bytes));
        assert!(a.file.is_empty());
        let strip = |hits: Vec<Hit>| -> Vec<Hit> {
            hits.into_iter()
                .map(|h| Hit {
                    function: None,
                    ..h
                })
                .collect()
        };
        assert_eq!(strip(a.hits), strip(b.hits), "{}", x1.name);
    }
}

#[test]
fn a_board_without_holes_still_gets_both_files() {
    let dir = TempDir::new("drill-empty");
    let engine = board(&dir);
    let files = plot(&engine);
    assert_eq!(files.len(), 2);
    for f in &files {
        let drill = read(&f.bytes);
        assert!(drill.hits.is_empty() && drill.ended, "{}", f.name);
    }
}

#[test]
fn slots_are_routed_along_their_long_side_turned_with_the_pad() {
    let dir = TempDir::new("drill-slots");
    let mut engine = board(&dir);
    let oval = |at: Value, hole: (f64, f64)| {
        json!({"at": at, "width": "2.4mm", "height": "1.2mm", "shape": "oval",
               "kind": "plated_through_hole",
               "hole": {"width": mm(hole.0), "height": mm(hole.1)}})
    };
    // Turned a quarter: the slot along the Pad's x runs up the board.
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "rotation": "90deg",
               "pads": [oval(pt(0.0, 0.0), (2.0, 0.8))]}),
    );
    // On the bottom the Footprint is mirrored: its Pad at x = 1 lands at x = -1.
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(20.0, 15.0), "side": "bottom",
               "pads": [oval(pt(1.0, 0.0), (1.6, 0.6))]}),
    );
    // Taller than wide: the slot runs along the Pad's y.
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "oval", "at": pt(20.0, 5.0),
               "width": "1mm", "height": "3mm", "hole": {"width": "1mm", "height": "3mm"}}),
    );
    let files = plot(&engine);
    let pth = read(&file(&files, "Demo-PTH.drl").bytes);
    let component = "Plated,PTH,ComponentDrill";
    assert_eq!(
        sorted(pth.hits),
        sorted(vec![
            slot(0.8, (10.0, 9.4), (10.0, 10.6), component),
            slot(0.6, (18.5, 15.0), (19.5, 15.0), component),
        ])
    );
    let npth = read(&file(&files, "Demo-NPTH.drl").bytes);
    assert_eq!(
        npth.hits,
        [slot(
            1.0,
            (20.0, 4.0),
            (20.0, 6.0),
            "NonPlated,NPTH,MechanicalDrill"
        )]
    );
}
