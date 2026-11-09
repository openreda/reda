//! Excellon drill files: read back (with a reader of our own) to the board's holes, and
//! byte-exact goldens.

mod drill_reader;

use drill_reader::{Hit, read};
use reda_model::{GerberSettings, SlotCommand};
use reda_output::{Drills, OutputFile, Plot, drill_files};
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

/// The drill files with the Project's first Fab Preset's switches, changed by `set`.
fn plot_with(engine: &TestEngine, set: impl FnOnce(&mut GerberSettings)) -> Drills {
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let mut gerber = project.fab_presets().next().unwrap().preset().gerber;
    set(&mut gerber);
    drill_files(&Plot {
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

fn plot(engine: &TestEngine) -> Vec<OutputFile> {
    plot_with(engine, |_| {}).files
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
        tolerance: None,
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
    assert_eq!(pth.file["FileFunction"], "Plated,1,2,PTH,Drill");
    assert_eq!(pth.file["FilePolarity"], "Positive");
    assert_eq!(
        sorted(pth.hits),
        sorted(vec![
            hit(0.3, (5.0, 5.0), "ViaDrill"),
            hit(0.8, (8.73, 10.0), "ComponentDrill"),
            // A plated Free Pad's hole is a mechanical one.
            hit(1.0, (25.0, 5.0), "MechanicalDrill"),
        ])
    );

    let npth = read(&file(&files, "Demo-NPTH.drl").bytes);
    assert!(npth.metric && npth.ended);
    assert_eq!(npth.file["FileFunction"], "NonPlated,1,2,NPTH,Drill");
    assert_eq!(npth.file["FilePolarity"], "Positive");
    assert_eq!(npth.hits, [hit(3.2, (3.5, 16.5), "MechanicalDrill")]);
}

/// A small board with every kind of hole: Vias, round and slotted Footprint Pads (one
/// turned off the axes, with a hole tolerance), plated and non-plated Free Pads.
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
             "hole": {"width": mm(2.2), "height": mm(0.8),
                      "tolerance": {"plus": mm(0.05), "minus": mm(0.05)}}}]}),
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
    let without = plot_with(&engine, |g| g.x2_attributes = false).files;
    assert_eq!(without.len(), with.len());
    for (x1, x2) in without.iter().zip(&with) {
        let text = String::from_utf8(x1.bytes.clone()).unwrap();
        assert!(!text.contains("#@!"), "{} has attributes", x1.name);
        let (a, b) = (read(&x1.bytes), read(&x2.bytes));
        assert!(a.file.is_empty());
        let strip = |hits: Vec<Hit>| -> Vec<Hit> {
            hits.into_iter()
                .map(|h| Hit {
                    function: None,
                    tolerance: None,
                    ..h
                })
                .collect()
        };
        assert_eq!(strip(a.hits), strip(b.hits), "{}", x1.name);
    }
}

#[test]
fn a_drill_file_with_no_holes_is_skipped() {
    let dir = TempDir::new("drill-empty");
    let mut engine = board(&dir);
    let skipped = |drills: &Drills| -> Vec<(String, &str)> {
        drills
            .skipped
            .iter()
            .map(|f| (f.name.clone(), f.function))
            .collect()
    };
    let none = plot_with(&engine, |_| {});
    assert!(none.files.is_empty());
    assert_eq!(
        skipped(&none),
        [
            ("Demo-PTH.drl".into(), "drill_plated"),
            ("Demo-NPTH.drl".into(), "drill_non_plated")
        ]
    );
    let merged = plot_with(&engine, |g| g.merge_drill = true);
    assert!(merged.files.is_empty());
    assert_eq!(skipped(&merged), [("Demo.drl".into(), "drill_mixed")]);

    // Only a non-plated hole: the PTH file is left out, the NPTH one written.
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "circle", "at": pt(3.5, 16.5),
               "width": "3.2mm", "hole": {"width": "3.2mm"}}),
    );
    let some = plot_with(&engine, |_| {});
    let names: Vec<&str> = some.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Demo-NPTH.drl"]);
    assert_eq!(skipped(&some), [("Demo-PTH.drl".into(), "drill_plated")]);
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
    // The slot's own rotation turns it in the Pad's frame: a quarter turn in a Pad turned a
    // quarter brings it back along the board's x. Mirrored on the bottom, a slot turned
    // +45° in the Footprint runs at -45° on the board.
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(30.0, 10.0), "rotation": "90deg", "pads": [{
            "at": pt(0.0, 0.0), "width": "2.4mm", "height": "2.4mm", "shape": "circle",
            "kind": "plated_through_hole",
            "hole": {"width": mm(2.0), "height": mm(0.8), "rotation": 90_000}}]}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(40.0, 10.0), "side": "bottom", "pads": [{
            "at": pt(0.0, 0.0), "width": "2.4mm", "height": "2.4mm", "shape": "circle",
            "kind": "plated_through_hole",
            "hole": {"width": mm(1.2), "height": mm(0.6), "rotation": 45_000}}]}),
    );
    // Taller than wide: the slot runs along the Pad's y.
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "oval", "at": pt(20.0, 5.0),
               "width": "1mm", "height": "3mm", "hole": {"width": "1mm", "height": "3mm"}}),
    );
    let files = plot(&engine);
    let pth = read(&file(&files, "Demo-PTH.drl").bytes);
    let component = "ComponentDrill";
    assert_eq!(
        sorted(pth.hits),
        sorted(vec![
            slot(0.8, (10.0, 9.4), (10.0, 10.6), component),
            slot(0.6, (18.5, 15.0), (19.5, 15.0), component),
            slot(0.8, (30.6, 10.0), (29.4, 10.0), component),
            slot(
                0.6,
                // 39.787868, 10.212132 to 40.212132, 9.787868, rounded to 1 µm.
                (39.788, 10.212),
                (40.212, 9.788),
                component
            ),
        ])
    );
    let npth = read(&file(&files, "Demo-NPTH.drl").bytes);
    assert_eq!(
        npth.hits,
        [slot(1.0, (20.0, 4.0), (20.0, 6.0), "MechanicalDrill")]
    );
}

#[test]
fn coordinates_and_tool_sizes_have_three_decimals_rounded_to_a_micrometre() {
    let dir = TempDir::new("drill-round");
    let mut engine = board(&dir);
    // 0.3004 mm rounds down, 5.0005 mm (a half) away from zero.
    engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": 5_000_500, "y": 7_123_499}, "diameter": "0.6mm",
               "hole": 300_400}),
    );
    let files = plot(&engine);
    let text = String::from_utf8(file(&files, "Demo-PTH.drl").bytes.clone()).unwrap();
    assert!(text.contains("\nT1C0.300\n"), "{text}");
    assert!(text.contains("\nX5.001Y7.123\n"), "{text}");
}

#[test]
fn the_file_function_label_says_drill_rout_or_mixed() {
    let dir = TempDir::new("drill-label");
    let mut engine = board(&dir);
    // Plated: round holes only. Non-plated: a slot only, until a round hole joins it.
    engine.run_ok(
        "pcb.via.add",
        json!({"at": pt(5.0, 5.0), "diameter": "0.6mm", "hole": "0.3mm"}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "oval", "at": pt(20.0, 5.0),
               "width": "1mm", "height": "3mm", "hole": {"width": "1mm", "height": "3mm"}}),
    );
    let function = |files: &[OutputFile], name: &str| {
        read(&file(files, name).bytes).file["FileFunction"].clone()
    };
    let files = plot(&engine);
    assert_eq!(function(&files, "Demo-PTH.drl"), "Plated,1,2,PTH,Drill");
    assert_eq!(function(&files, "Demo-NPTH.drl"), "NonPlated,1,2,NPTH,Rout");
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "circle", "at": pt(3.5, 16.5),
               "width": "3.2mm", "hole": {"width": "3.2mm"}}),
    );
    let files = plot(&engine);
    assert_eq!(
        function(&files, "Demo-NPTH.drl"),
        "NonPlated,1,2,NPTH,Mixed"
    );
}

#[test]
fn a_tool_whose_holes_have_a_tolerance_states_it() {
    let dir = TempDir::new("drill-tolerance");
    let mut engine = board(&dir);
    let pad = |x: f64, tolerance: Value| {
        let mut hole = json!({"width": mm(1.0), "height": mm(1.0)});
        if !tolerance.is_null() {
            hole["tolerance"] = tolerance;
        }
        json!({"at": pt(x, 0.0), "width": "1.7mm", "height": "1.7mm", "shape": "circle",
               "kind": "plated_through_hole", "hole": hole})
    };
    // Three 1 mm holes: two share a tolerance, one has none. Same size, two tools.
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "pads": [
            pad(-2.54, json!({"plus": mm(0.05), "minus": mm(0.025)})),
            pad(0.0, json!({"plus": mm(0.05), "minus": mm(0.025)})),
            pad(2.54, Value::Null)]}),
    );
    let files = plot(&engine);
    let text = String::from_utf8(file(&files, "Demo-PTH.drl").bytes.clone()).unwrap();
    assert!(
        text.contains(
            "; #@! TA.AperFunction,ComponentDrill\nT1C1.000\n\
             ; #@! TA.AperFunction,ComponentDrill\n; #@! TA.DrillTolerance,0.050,0.025\nT2C1.000\n"
        ),
        "{text}"
    );
    let hits = read(&file(&files, "Demo-PTH.drl").bytes).hits;
    let tolerances: Vec<Option<&str>> = hits.iter().map(|h| h.tolerance.as_deref()).collect();
    assert_eq!(tolerances, [None, Some("0.050,0.025"), Some("0.050,0.025")]);
}

#[test]
fn attribute_values_are_printable_ascii_with_other_characters_escaped() {
    let dir = TempDir::new("drill-ascii");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Bühne", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine.run_ok("project.properties.set", json!({"revision": "Ω;1 🙂"}));
    engine.run_ok(
        "pcb.via.add",
        json!({"at": pt(5.0, 5.0), "diameter": "0.6mm", "hole": "0.3mm"}),
    );
    let files = plot(&engine);
    // The reader refuses any line outside printable ASCII.
    let drill = read(&file(&files, "Bühne-PTH.drl").bytes);
    let id = &drill.file["ProjectId"];
    assert!(id.starts_with("B\\u00FChne,"), "{id}");
    assert!(id.ends_with(",\\u03A9\\u003B1 \\U0001F642"), "{id}");
}

#[test]
fn slots_can_be_written_as_g85_canned_slots() {
    let dir = TempDir::new("drill-g85");
    let engine = demo(&dir);
    let routed = plot(&engine);
    let g85 = plot_with(&engine, |g| g.slot_command = SlotCommand::G85).files;
    let pth = String::from_utf8(file(&g85, "Demo-PTH.drl").bytes.clone()).unwrap();
    assert!(pth.contains("\nX15.394Y9.650G85X16.606Y10.350\n"), "{pth}");
    for f in &g85 {
        let text = String::from_utf8(f.bytes.clone()).unwrap();
        assert!(!text.contains("M15") && !text.contains("G01"), "{}", f.name);
        assert_eq!(
            sorted(read(&f.bytes).hits),
            sorted(read(&file(&routed, &f.name).bytes).hits),
            "{}",
            f.name
        );
    }
}

#[test]
fn merge_drill_writes_one_file_where_plated_and_non_plated_never_share_a_tool() {
    let dir = TempDir::new("drill-merge");
    let mut engine = board(&dir);
    // A plated and a non-plated 1 mm hole, and a non-plated slot.
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "circle", "at": pt(5.0, 5.0),
               "width": "1.6mm", "hole": {"width": "1mm"}}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "circle", "at": pt(10.0, 5.0),
               "width": "1mm", "hole": {"width": "1mm"}}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "oval", "at": pt(20.0, 5.0),
               "width": "1mm", "height": "3mm", "hole": {"width": "1mm", "height": "3mm"}}),
    );
    let drills = plot_with(&engine, |g| g.merge_drill = true);
    assert!(drills.skipped.is_empty());
    let names: Vec<(&str, &str)> = drills
        .files
        .iter()
        .map(|f| (f.name.as_str(), f.function))
        .collect();
    assert_eq!(names, [("Demo.drl", "drill_mixed")]);
    let text = String::from_utf8(drills.files[0].bytes.clone()).unwrap();
    let tools = text.lines().filter(|l| l.contains("C1.000")).count();
    assert_eq!(tools, 2, "{text}");
    let drill = read(&drills.files[0].bytes);
    assert_eq!(drill.file["FileFunction"], "MixedPlating,1,2,Mixed");
    assert_eq!(drill.file["FilePolarity"], "Positive");
    assert_eq!(
        sorted(drill.hits),
        sorted(vec![
            hit(1.0, (5.0, 5.0), "MechanicalDrill"),
            hit(1.0, (10.0, 5.0), "MechanicalDrill"),
            slot(1.0, (20.0, 4.0), (20.0, 6.0), "MechanicalDrill"),
        ])
    );
    // The plated hole has a tool of its own.
    let t1 = text.split("\nT1\n").nth(1).unwrap();
    let t1 = t1.split("\nT2\n").next().unwrap();
    assert_eq!(t1, "X5.000Y5.000", "{text}");
}

#[test]
fn non_plated_slots_stay_in_the_npth_file_in_every_shipped_preset() {
    let dir = TempDir::new("drill-presets");
    let mut engine = board(&dir);
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "oval", "at": pt(20.0, 5.0),
               "width": "1mm", "height": "3mm", "hole": {"width": "1mm", "height": "3mm"}}),
    );
    let presets = reda_output::shipped();
    assert!(presets.len() >= 3);
    for preset in presets {
        let drills = plot_with(&engine, |g| *g = preset.gerber.clone());
        let npth = read(&file(&drills.files, "Demo-NPTH.drl").bytes);
        assert_eq!(
            npth.hits,
            [slot(1.0, (20.0, 4.0), (20.0, 6.0), "MechanicalDrill")],
            "{}",
            preset.name
        );
    }
}
