//! The files written for the fab beside the Gerber and drill set: the Fab Notes, the
//! drill report and the Gerber job file, each checked line by line and against a
//! byte-exact golden.

use reda_model::GerberSettings;
use reda_output::{
    OutputFile, Plot, drill_report_file, fab_notes_file, gerber_files, gerber_job_file,
};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn mm(v: f64) -> i64 {
    (v * 1_000_000.0).round() as i64
}

fn pt(x: f64, y: f64) -> Value {
    json!({"x": mm(x), "y": mm(y)})
}

fn net(engine: &mut TestEngine, name: &str) -> Value {
    engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"].clone()
}

/// A 30 x 20 mm board, with only what a new Project has.
fn bare(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "30mm", "height": "20mm"}),
    );
    engine
}

/// A board with every kind of hole (two of them with a tolerance), a Net Tie, a full
/// Stackup and Fab Notes text.
fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = bare(dir);
    engine.run_ok("project.properties.set", json!({"revision": "B"}));
    let gnd = net(&mut engine, "GND");
    let agnd = net(&mut engine, "AGND");
    let sig = net(&mut engine, "SIG");
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(6.0, 10.0), "designator": "J1", "component_type": "standard", "pads": [
            {"at": pt(-1.27, 0.0), "width": "1.7mm", "height": "1.7mm", "shape": "circle",
             "kind": "plated_through_hole", "net": gnd,
             "hole": {"width": mm(1.0), "height": mm(1.0)}},
            {"at": pt(1.27, 0.0), "width": "1.7mm", "height": "1.7mm", "shape": "circle",
             "kind": "plated_through_hole", "net": sig,
             "hole": {"width": mm(1.0), "height": mm(1.0),
                      "tolerance": {"plus": mm(0.1), "minus": mm(0.05)}}}]}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(16.0, 10.0), "designator": "J2", "component_type": "standard", "pads": [
            {"at": pt(0.0, 0.0), "width": "3mm", "height": "1.6mm", "shape": "oval",
             "kind": "plated_through_hole", "net": sig,
             "hole": {"width": mm(2.2), "height": mm(0.8),
                      "tolerance": {"plus": mm(0.05), "minus": mm(0.05)}}}]}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(20.0, 4.0), "designator": "NT1", "component_type": "net_tie", "pads": [
            {"at": pt(-0.25, 0.0), "width": "0.5mm", "height": "0.5mm", "net": gnd},
            {"at": pt(0.25, 0.0), "width": "0.5mm", "height": "0.5mm", "net": agnd}]}),
    );
    engine.run_ok("pcb.via.add", json!({"at": pt(10.0, 6.0), "net": sig}));
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "circle", "at": pt(4.0, 16.0),
               "width": "3.2mm", "hole": {"width": "3.2mm",
               "tolerance": {"plus": "0.05mm", "minus": "0mm"}}}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "oval", "at": pt(26.0, 16.0),
               "width": "1mm", "height": "2.5mm",
               "hole": {"width": "1mm", "height": "2.5mm"}}),
    );
    engine.run_ok(
        "pcb.stackup.set",
        json!({"thickness": "1.2mm", "finish": "ENIG", "mask_color": "Matte black",
               "silk_color": "White"}),
    );
    engine.run_ok(
        "pcb.fab_notes.set",
        json!({"text": "Please keep the slot ends square.\nNo X-outs."}),
    );
    engine
}

/// Runs `f` with a Plot of the board, with the first Fab Preset's switches.
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

fn notes_file(plot: &Plot<'_>) -> OutputFile {
    let mut files = gerber_files(plot).unwrap().files;
    files.extend(reda_output::drill_files(plot).files);
    fab_notes_file(plot, &files, &[])
}

fn fab_notes(engine: &TestEngine) -> String {
    let file = with_plot(engine, |_| {}, notes_file);
    assert_eq!(file.name, "Demo-fab-notes.txt");
    String::from_utf8(file.bytes).unwrap()
}

fn golden(file: &OutputFile) {
    reda_testkit::assert_golden(
        format!(
            "{}/tests/golden/fab/{}",
            env!("CARGO_MANIFEST_DIR"),
            file.name
        ),
        &file.bytes,
    );
}

/// The lines of the section under `heading`, unindented, up to the next blank line.
fn section(text: &str, heading: &str) -> Vec<String> {
    let mut lines = text.lines().skip_while(|l| *l != heading);
    assert!(lines.next().is_some(), "no {heading} section in:\n{text}");
    lines
        .take_while(|l| !l.is_empty())
        .map(|l| l.trim_start().to_owned())
        .collect()
}

/// A plain-text table in the notes, in the same order the fab sees it.
fn table(text: &str, heading: &str) -> Vec<Vec<String>> {
    section(text, heading)
        .iter()
        .map(|line| {
            line.trim_matches('|')
                .split('|')
                .map(|s| s.trim().to_owned())
                .collect()
        })
        .collect()
}

#[test]
fn fab_notes_list_the_board_and_its_stackup_values() {
    let dir = TempDir::new("fab-notes-board");
    let engine = demo(&dir);
    let notes = fab_notes(&engine);
    assert!(
        notes.starts_with("Fab Notes for Demo\nRevision: B\n"),
        "{notes}"
    );
    assert_eq!(
        section(&notes, "Board"),
        [
            "Size: 30 x 20 mm",
            "Copper layers: 2",
            "Thickness: 1.2 mm",
            "Symmetry: Symmetric",
            "Thickness check: Outside 10% tolerance",
            "Finish: ENIG",
            "Solder mask colour: Matte black",
            "Silkscreen colour: White",
        ]
    );
    assert_eq!(
        table(&notes, "Stackup"),
        [
            ["Layer", "Kind", "Role", "Thickness", "Material", "Dk"],
            ["L1 (top)", "Copper", "Signal", "1 oz (35 µm)", "", ""],
            ["", "Core", "", "1.53 mm", "", ""],
            ["L2 (bottom)", "Copper", "Signal", "1 oz (35 µm)", "", ""],
        ]
    );
}

#[test]
fn fab_notes_leave_out_stackup_values_that_are_unset_and_show_odd_copper_in_um() {
    let dir = TempDir::new("fab-notes-unset");
    let mut engine = bare(&dir);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    let sheet = pcb.stackup().dielectrics().next().unwrap().uuid;
    engine.run_ok(
        "pcb.stackup.set",
        json!({"copper": [{"layer": layers[0], "thickness": "18um"},
                          {"layer": layers[1], "thickness": "2.5oz"}],
               "dielectrics": [{"uuid":sheet,"thickness":null}]}),
    );
    let notes = fab_notes(&engine);
    assert_eq!(
        section(&notes, "Board"),
        [
            "Size: 30 x 20 mm",
            "Copper layers: 2",
            "Thickness: 1.6 mm",
            "Symmetry: Not symmetric",
            "Thickness check: Within 10% tolerance"
        ]
    );
    let stack = table(&notes, "Stackup");
    assert_eq!(stack[1][3], "18 µm");
    assert_eq!(stack[2], ["", "Core", "", "", "", ""]);
    assert_eq!(stack[3][3], "2.5 oz (87.5 µm)");
    for absent in ["Hole tolerances", "Deliberate shorts", "\nNotes"] {
        assert!(!notes.contains(absent), "{absent} in:\n{notes}");
    }
}

#[test]
fn fab_notes_identify_the_unmodified_build_and_print_its_current_checks() {
    let dir = TempDir::new("fab-notes-build");
    let mut engine = bare(&dir);
    let original = fab_notes(&engine);
    let board = section(&original, "Board");
    for line in [
        "Construction: 2 Layer",
        "Symmetry: Symmetric",
        "Thickness check: Within 10% tolerance",
    ] {
        assert!(board.iter().any(|row| row == line), "{original}");
    }

    let pcb = engine.project().unwrap().pcb().unwrap();
    let top = pcb.copper_layers().next().unwrap().uuid;
    let first = pcb.stackup().dielectrics().next().unwrap().uuid;
    let second =
        engine.run_ok("pcb.stackup.dielectric.add", json!({"beside":first}))["result"]["uuid"]
            .clone();
    engine.run_ok("pcb.layer.set", json!({"layer":top, "role":"mixed"}));
    engine.run_ok("pcb.stackup.set", json!({"dielectrics":[
        {"uuid":first,"kind":"prepreg","name":"Bond","material":"2116","dk":4180,"thickness":"0.13mm"},
        {"uuid":second,"kind":"core","thickness":"0.5mm"}]}));
    let modified = fab_notes(&engine);
    assert!(!modified.contains("Construction:"), "{modified}");
    assert!(modified.contains("Symmetry: Not symmetric"), "{modified}");
    assert!(
        modified.contains("Thickness check: Outside 10% tolerance"),
        "{modified}"
    );
    let stack = table(&modified, "Stackup");
    assert_eq!(stack[1][2], "Mixed");
    assert_eq!(stack[2], ["Bond", "Prepreg", "", "0.13 mm", "2116", "4.18"]);
    assert_eq!(stack[3], ["", "Core", "", "0.5 mm", "", ""]);
}

#[test]
fn fab_notes_list_the_minimum_features_of_the_board_wide_rules() {
    let dir = TempDir::new("fab-notes-rules");
    let engine = bare(&dir);
    let notes = fab_notes(&engine);
    let pcb = engine.project().unwrap().pcb().unwrap();
    use reda_model::RuleKind::*;
    let v = |k| &pcb.board_rule(k).values;
    let expected = [
        format!(
            "Clearance: {} mm",
            v(Clearance).value.unwrap().0 as f64 / 1e6
        ),
        format!(
            "Track width: {} mm",
            v(TrackWidth).min.unwrap().0 as f64 / 1e6
        ),
    ];
    let features = section(&notes, "Minimum features");
    assert_eq!(features[..2], expected, "{notes}");
    let labels: Vec<&str> = features
        .iter()
        .map(|l| l.split(':').next().unwrap())
        .collect();
    assert_eq!(
        labels,
        [
            "Clearance",
            "Track width",
            "Via diameter",
            "Via hole",
            "Hole size",
            "Annular ring",
            "Hole to hole",
            "Copper to board edge"
        ]
    );
}

#[test]
fn fab_notes_list_holes_with_a_tolerance() {
    let dir = TempDir::new("fab-notes-tolerance");
    let engine = demo(&dir);
    assert_eq!(
        section(&fab_notes(&engine), "Hole tolerances"),
        [
            "J1 pad 2: 1 mm hole, plated, +0.1/-0.05 mm",
            "J2 pad 1: 2.2 x 0.8 mm slot, plated, +0.05/-0.05 mm",
            "Free Pad at (4, 16) mm: 3.2 mm hole, non-plated, +0.05/-0 mm",
        ]
    );
}

#[test]
fn fab_notes_list_each_net_tie_and_its_nets_as_a_deliberate_short() {
    let dir = TempDir::new("fab-notes-ties");
    let engine = demo(&dir);
    assert_eq!(
        section(&fab_notes(&engine), "Deliberate shorts (Net Ties)"),
        ["NT1: joins AGND and GND on purpose; this short is deliberate"]
    );
}

#[test]
fn fab_notes_list_the_zip_and_end_with_the_boards_own_text() {
    let dir = TempDir::new("fab-notes-text");
    let engine = demo(&dir);
    let notes = fab_notes(&engine);
    assert_eq!(
        section(&notes, "Files in the zip"),
        [
            "Demo.gtl",
            "Demo.gbl",
            "Demo.gts",
            "Demo.gbs",
            "Demo.gtp",
            "Demo.gto",
            "Demo.gm1",
            "Demo-PTH.drl",
            "Demo-NPTH.drl",
            "Demo-fab-notes.txt"
        ]
    );
    assert!(
        notes.ends_with("\n\nNotes\nPlease keep the slot ends square.\nNo X-outs.\n"),
        "{notes}"
    );
}

#[test]
fn fab_notes_list_the_written_copper_files_in_physical_order() {
    let dir = TempDir::new("fab-notes-copper-files");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4}),
    );
    let notes = with_plot(
        &engine,
        |g| g.file_names.inner_copper = Some("layer-${inner}.ger".into()),
        |plot| {
            let files = gerber_files(plot).unwrap().files;
            String::from_utf8(fab_notes_file(plot, &files, &[]).bytes).unwrap()
        },
    );
    assert_eq!(
        section(&notes, "Copper files in order"),
        ["Demo.gtl", "layer-1.ger", "layer-2.ger", "Demo.gbl"]
    );
}

#[test]
fn fab_notes_count_vias_by_span_and_name_only_the_drill_files_in_the_job() {
    let dir = TempDir::new("fab-notes-vias");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let document = pcb.uuid;
    let layers: Vec<_> = pcb.copper_layers().map(|layer| layer.uuid).collect();
    engine.run_ok("pcb.via_type.add", json!({"document":document, "name":"Laser", "from":layers[1],"to":layers[0],"micro":true,"hole":"0.1mm","diameter":{"start":"0.22mm","end":"0.18mm"}}));
    engine.run_ok(
        "pcb.via_type.add",
        json!({"document":document, "from":layers[2],"to":layers[3]}),
    );
    for (x, from, to) in [
        (1.0, 1, 4),
        (2.0, 2, 1),
        (3.0, 1, 2),
        (4.0, 3, 2),
        (5.0, 2, 3),
    ] {
        engine.run_ok(
            "pcb.via.add",
            json!({"at":pt(x,8.0),"from":layers[from-1],"to":layers[to-1]}),
        );
    }
    for (merge, file) in [(false, "Demo-PTH.drl"), (true, "Demo.drl")] {
        let notes = with_plot(
            &engine,
            |g| g.merge_drill = merge,
            |plot| String::from_utf8(notes_file(plot).bytes).unwrap(),
        );
        let rows = table(&notes, "Via types");
        assert_eq!(
            rows,
            [
                [
                    "Name",
                    "Span",
                    "Micro",
                    "Drill (mm)",
                    "Pad (mm)",
                    "Vias",
                    "Drill file"
                ],
                ["Through", "L1-L4", "No", "0.3", "0.7", "1", file],
                [
                    "Laser",
                    "L2-L1",
                    "Yes",
                    "0.1",
                    "0.22 (end 0.18)",
                    "2",
                    "Demo-L1-L2.drl"
                ],
                ["Blind 3:4", "L3-L4", "No", "0.3", "0.7", "0", ""],
                ["Unmatched", "L2-L3", "No", "", "", "2", "Demo-L2-L3.drl"],
            ]
        );
    }
    let notes = with_plot(
        &engine,
        |_| {},
        |plot| String::from_utf8(fab_notes_file(plot, &[], &[]).bytes).unwrap(),
    );
    assert!(
        table(&notes, "Via types")
            .iter()
            .skip(1)
            .all(|row| row[6].is_empty()),
        "{notes}"
    );
    assert!(!notes.contains("Copper files in order"), "{notes}");
}

#[test]
fn fab_notes_match_their_golden_byte_for_byte() {
    let dir = TempDir::new("fab-notes-golden");
    let engine = demo(&dir);
    golden(&with_plot(&engine, |_| {}, notes_file));
}

fn drill_report(engine: &TestEngine, set: impl FnOnce(&mut GerberSettings)) -> String {
    let file = with_plot(engine, set, drill_report_file);
    assert_eq!(file.name, "Demo-drill-report.txt");
    String::from_utf8(file.bytes).unwrap()
}

/// The table rows of drill file `name`'s section, split on whitespace.
fn rows(report: &str, name: &str) -> Vec<Vec<String>> {
    section(report, name)
        .iter()
        .skip(1)
        .map(|l| l.split_whitespace().map(String::from).collect())
        .collect()
}

#[test]
fn the_drill_report_lists_every_tool_of_each_file_with_its_counts_and_the_totals() {
    let dir = TempDir::new("drill-report-tools");
    let engine = demo(&dir);
    let report = drill_report(&engine, |_| {});
    let row = |r: &[&str]| r.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        rows(&report, "Demo-PTH.drl (L1-L2, through)"),
        [
            // The new board's Via hole, then J1's two holes (one with a tolerance), J2's slot.
            row(&["T1", "0.300", "plated", "via", "1", "0", "-"]),
            row(&[
                "T2",
                "0.800",
                "plated",
                "component",
                "0",
                "1",
                "+0.050/-0.050"
            ]),
            row(&["T3", "1.000", "plated", "component", "1", "0", "-"]),
            row(&[
                "T4",
                "1.000",
                "plated",
                "component",
                "1",
                "0",
                "+0.100/-0.050"
            ]),
            row(&["Total", "3", "1"]),
        ]
    );
    assert_eq!(
        rows(&report, "Demo-NPTH.drl (L1-L2, through)"),
        [
            row(&["T1", "1.000", "non-plated", "mechanical", "0", "1", "-"]),
            row(&[
                "T2",
                "3.200",
                "non-plated",
                "mechanical",
                "1",
                "0",
                "+0.050/-0.000"
            ]),
            row(&["Total", "1", "1"]),
        ]
    );
    assert!(
        report.ends_with("\nAll files: 4 holes, 2 slots\n"),
        "{report}"
    );
}

#[test]
fn the_drill_report_leaves_out_tool_travel() {
    let dir = TempDir::new("drill-report-travel");
    let engine = demo(&dir);
    let report = drill_report(&engine, |_| {}).to_lowercase();
    assert!(report.contains("tool travel is left out"), "{report}");
    for word in ["travel:", "path length", "distance"] {
        assert!(!report.contains(word), "{word} in:\n{report}");
    }
}

#[test]
fn the_drill_report_follows_merge_drill_and_says_when_a_file_has_no_holes() {
    let dir = TempDir::new("drill-report-merge");
    let engine = demo(&dir);
    let merged = drill_report(&engine, |g| g.merge_drill = true);
    assert!(!merged.contains("Demo-PTH.drl"), "{merged}");
    assert_eq!(
        rows(&merged, "Demo.drl (L1-L2, through)").last().unwrap(),
        &["Total", "4", "2"]
    );

    let dir = TempDir::new("drill-report-empty");
    let mut engine = bare(&dir);
    engine.run_ok("pcb.via.add", json!({"at": pt(10.0, 6.0)}));
    let report = drill_report(&engine, |_| {});
    assert_eq!(
        section(&report, "Demo-NPTH.drl (L1-L2, through)"),
        ["No holes: the file is not written."]
    );
}

#[test]
fn the_drill_report_matches_its_golden_byte_for_byte() {
    let dir = TempDir::new("drill-report-golden");
    let engine = demo(&dir);
    golden(&with_plot(&engine, |_| {}, drill_report_file));
}

/// The job file of the demo board, listing its Gerber and drill files.
fn job(engine: &TestEngine) -> OutputFile {
    with_plot(
        engine,
        |_| {},
        |plot| {
            let mut files = gerber_files(plot).unwrap().files;
            files.extend(reda_output::drill_files(plot).files);
            files.push(fab_notes_file(plot, &files, &[]));
            gerber_job_file(plot, &files)
        },
    )
}

#[test]
fn the_job_file_is_minimal_json_in_the_job_spec_form() {
    let dir = TempDir::new("job-file");
    let engine = demo(&dir);
    let file = job(&engine);
    assert_eq!(file.name, "Demo-job.gbrjob");
    let value: Value = serde_json::from_slice(&file.bytes).unwrap();
    let project = engine.project().unwrap().props().uuid;
    assert_eq!(
        value["Header"],
        json!({"GenerationSoftware": {"Vendor": "openreda", "Application": "REDA",
                                      "Version": reda_testkit::TEST_VERSION}})
    );
    assert_eq!(
        value["GeneralSpecs"],
        json!({"ProjectId": {"Name": "Demo", "GUID": project.to_string(), "Revision": "B"},
               "Size": {"X": 30.0, "Y": 20.0}, "LayerNumber": 2, "BoardThickness": 1.2,
               "Finish": "ENIG"})
    );
    let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "FilesAttributes",
            "GeneralSpecs",
            "Header",
            "MaterialStackup"
        ]
    );
    let entries = value["FilesAttributes"].as_array().unwrap();
    assert_eq!(
        entries[0],
        json!({"Path": "Demo.gtl", "FileFunction": "Copper,L1,Top,Signal", "FilePolarity": "Positive"})
    );
    let find = |path: &str| entries.iter().find(|e| e["Path"] == path).cloned();
    assert_eq!(
        find("Demo.gts"),
        Some(
            json!({"Path": "Demo.gts", "FileFunction": "Soldermask,Top", "FilePolarity": "Negative"})
        )
    );
    assert_eq!(
        find("Demo-NPTH.drl"),
        Some(
            json!({"Path": "Demo-NPTH.drl", "FileFunction": "NonPlated,1,2,NPTH,Mixed",
                    "FilePolarity": "Positive"})
        )
    );
    assert_eq!(
        find("Demo-fab-notes.txt"),
        None,
        "only Gerber and drill files"
    );
}

#[test]
fn the_job_file_leaves_out_a_finish_the_job_spec_does_not_name() {
    let dir = TempDir::new("job-file-finish");
    let mut engine = demo(&dir);
    engine.run_ok("pcb.stackup.set", json!({"finish": "Carbon ink"}));
    let value: Value = serde_json::from_slice(&job(&engine).bytes).unwrap();
    assert_eq!(value["GeneralSpecs"].get("Finish"), None);
    engine.run_ok("pcb.stackup.set", json!({"finish": "HASL (lead-free)"}));
    let value: Value = serde_json::from_slice(&job(&engine).bytes).unwrap();
    assert_eq!(value["GeneralSpecs"]["Finish"], "HAL lead-free");
}

#[test]
fn the_job_file_matches_its_golden_byte_for_byte() {
    let dir = TempDir::new("job-file-golden");
    let engine = demo(&dir);
    golden(&job(&engine));
}

#[test]
fn drill_report_labels_the_actual_micro_and_unmatched_spans() {
    let dir = TempDir::new("span-report");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let document = pcb.uuid;
    let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    engine.run_ok(
        "pcb.via_type.add",
        json!({"document":document,"from":layers[1],"to":layers[0],"micro":true}),
    );
    for (from, to) in [(1, 0), (1, 2)] {
        engine.run_ok(
            "pcb.via.add",
            json!({"at":pt(5.0,7.0),"from":layers[from],"to":layers[to]}),
        );
    }
    let report = drill_report(&engine, |_| {});
    assert!(report.contains("Demo-PTH.drl (L1-L4, through)"), "{report}");
    assert!(
        report.contains("Demo-L1-L2.drl (L1-L2, blind, micro)"),
        "{report}"
    );
    assert!(
        report.contains("Demo-L2-L3.drl (L2-L3, buried)"),
        "{report}"
    );
    assert!(report.contains("All files: 2 holes, 0 slots"));
}

mod job_reader;

#[test]
fn the_job_reader_matches_all_physical_sheets_and_actual_outer_files() {
    use reda_model::StackupEntry;
    let dir = TempDir::new("multilayer-job-readback");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Four","layers":4}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let first = pcb.stackup().dielectrics().next().unwrap().uuid;
    let inner = pcb.copper_layers().nth(1).unwrap().uuid;
    let extra =
        engine.run_ok("pcb.stackup.dielectric.add", json!({"beside":first}))["result"]["uuid"]
            .clone();
    engine.run_ok("pcb.layer.set", json!({"layer":inner,"name":"Return"}));
    engine.run_ok("pcb.stackup.set",json!({
        "mask_color":"Black","silk_color":"White",
        "dielectrics":[{"uuid":first,"name":"Bond","material":"1080","thickness":"0.10mm","dk":4210,"df":19500},
                       {"uuid":extra,"thickness":null}]
    }));
    engine.run_ok(
        "pcb.text.add",
        json!({"layer":"silkscreen_top","text":"REDA","x":"1mm","y":"3mm"}),
    );
    with_plot(
        &engine,
        |_| {},
        |plot| {
            let pcb = plot.instances[0].pcb;
            let mut files = gerber_files(plot).unwrap().files;
            files.extend(reda_output::drill_files(plot).files);
            let read = job_reader::read(&gerber_job_file(plot, &files).bytes);
            let files_written: Vec<_> = files
                .iter()
                .filter_map(|f| {
                    f.attributes
                        .as_ref()
                        .map(|a| (f.name.as_str(), a.function.as_str()))
                })
                .collect();
            assert_eq!(
                read.files_attributes
                    .iter()
                    .map(|f| (f.path.as_str(), f.file_function.as_str()))
                    .collect::<Vec<_>>(),
                files_written
            );
            assert_eq!(read.general_specs.layer_number, pcb.copper_layers().count());
            let rows = read.material_stackup;
            assert_eq!(rows.len(), pcb.stackup().entries.len() + 3);
            assert_eq!(
                (&*rows[0].kind, rows[0].color.as_deref()),
                ("Legend", Some("White"))
            );
            for i in [1, rows.len() - 1] {
                assert_eq!(
                    (&*rows[i].kind, rows[i].color.as_deref()),
                    ("SolderMask", Some("Black"))
                );
            }
            for (entry, row) in pcb.stackup().entries.iter().zip(&rows[2..]) {
                match entry {
                    StackupEntry::Copper(copper) => {
                        assert_eq!(row.kind, "Copper");
                        assert_eq!(row.thickness, Some(copper.thickness.0 as f64 / 1e6));
                        if copper.layer == inner {
                            assert_eq!(row.name.as_deref(), Some("Return"));
                        }
                    }
                    StackupEntry::Dielectric(sheet) => {
                        assert_eq!(row.kind, "Dielectric");
                        assert_eq!(row.name, sheet.name);
                        assert_eq!(row.material, sheet.material);
                        assert_eq!(row.thickness, sheet.thickness.map(|v| v.0 as f64 / 1e6));
                        assert_eq!(
                            row.dielectric_constant,
                            sheet.dk.map(|v| f64::from(v) / 1000.0)
                        );
                        assert_eq!(row.loss_tangent, sheet.df.map(|v| f64::from(v) / 1e6));
                    }
                }
            }
        },
    );
}
