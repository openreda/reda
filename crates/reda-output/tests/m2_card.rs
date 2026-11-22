//! Read the fab files of the `m2_card` reference Project back against its PCB
//! (docs/spec/v0.2.5/testing.md#read-back): the M.2 template's card, key M 2280, with the
//! fixture bevel {20°, 1.30} and hard gold. The fingers plot as `ConnectorPad` on both
//! outer Layers, the half-moon as a `CastellatedPad` on a `CastellatedDrill`, and the job
//! file and Fab Notes carry the edge-connector keys.
mod drill_reader;
mod gerber_reader;
mod job_reader;

use std::fs;
use std::path::Path;

use gerber_reader::Object;
use reda_model::PadRole;
use reda_testkit::open_reference_copy;
use serde_json::json;

/// The `.AperFunction` of each flash in a Gerber file, with its position.
fn flashes(layer: &gerber_reader::Layer) -> Vec<(String, (i64, i64))> {
    layer
        .flashes()
        .map(|o| match o {
            Object::Flash { function, at, .. } => (function.clone().unwrap_or_default(), *at),
            _ => unreachable!(),
        })
        .collect()
}

#[test]
fn the_m2_card_fab_files_read_back_as_its_fingers_half_moon_and_bevel() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/m2_card");
    let (_dir, mut engine) = open_reference_copy(&source, "M2Card.reda");
    let job = engine.project().unwrap().output_jobs().next().unwrap().uuid;
    let report = engine.run_ok("output.job.run", json!({"job": job}));
    let output = Path::new(report["destination"].as_str().unwrap());
    let pcb = engine.project().unwrap().pcb().unwrap();
    let fab = output.join("fab");

    // Key M removes Pins 59 to 66: 34 odd fingers on top (0.35 × 1.50) and 33 even ones
    // below (0.35 × 2.00), so 34 × 0.525 + 33 × 0.70 = 40.95 mm² of hard gold.
    let job = job_reader::read(&fs::read(fab.join("M2Card-job.gbrjob")).unwrap());
    let specs = &job.general_specs;
    assert_eq!(specs.layer_number, 2);
    assert_eq!(specs.board_thickness, Some(0.8));
    assert_eq!(specs.edge_connector, Some(true));
    assert_eq!(specs.edge_connector_bevelled, Some(true));
    assert_eq!(specs.hard_gold_area, Some(40.95));
    assert_eq!(specs.castellated, Some(true));
    let notes = specs.notes.as_deref().unwrap();
    assert!(
        notes.contains("20°") && notes.contains("1.30 mm"),
        "{notes}"
    );
    let top_copper = (job.files_attributes.iter())
        .find(|f| f.file_function == "Copper,L1,Top,Signal")
        .map(|f| f.path.as_str());
    assert_eq!(top_copper, Some("M2Card.gtl"));

    // Every finger on its own side, as a ConnectorPad; nothing else is one.
    let fingers = pcb
        .footprints()
        .flat_map(|f| &pcb.footprint_copy(f.footprint).unwrap().footprint.pads)
        .filter(|p| p.role == Some(PadRole::EdgeConnector))
        .count();
    let top = flashes(&gerber_reader::read(
        &fs::read(fab.join("M2Card.gtl")).unwrap(),
    ));
    let bottom = flashes(&gerber_reader::read(
        &fs::read(fab.join("M2Card.gbl")).unwrap(),
    ));
    let count = |flashes: &[(String, (i64, i64))], function: &str| {
        flashes.iter().filter(|(f, _)| f == function).count()
    };
    assert_eq!(count(&top, "ConnectorPad"), 34);
    assert_eq!(count(&bottom, "ConnectorPad"), 33);
    assert_eq!(fingers, 67);

    // The half-moon: one castellated land on each side and its plated hole, centred on
    // the far edge at (11, 80).
    let moon = (11_000_000, 80_000_000);
    for flashes in [&top, &bottom] {
        let castellated: Vec<_> = flashes
            .iter()
            .filter(|(f, _)| f == "CastellatedPad")
            .collect();
        assert_eq!(castellated, [&("CastellatedPad".to_owned(), moon)]);
    }
    let drill = drill_reader::read(&fs::read(fab.join("M2Card-PTH.drl")).unwrap());
    let hits: Vec<_> = drill
        .hits
        .iter()
        .filter(|h| h.function.as_deref() == Some("CastellatedDrill"))
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].at, hits[0].diameter), (moon, 3_500_000));

    let fab_notes = fs::read_to_string(fab.join("M2Card-fab-notes.txt")).unwrap();
    assert!(fab_notes.contains("Bevel edge-connector edge: 20°, 1.30 mm"));
    assert!(fab_notes.contains("Hard gold on edge-connector Pads, 40.95 mm²"));
}
