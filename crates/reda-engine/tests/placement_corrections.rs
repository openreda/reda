//! Read the corrected CPL back through an Output Job on a board with Components on both sides.
use reda_engine::{journal, output_preview::job_preview};
use reda_testkit::{TempDir, read_csv, starter_library, test_engine_with_library};
use serde_json::json;
use std::path::Path;

#[test]
fn corrected_cpl_matches_the_board_on_both_sides() {
    let dir = TempDir::new("placement-corrections");
    let mut engine = test_engine_with_library(&starter_library());
    let script = include_str!("../../../tests/scenarios/placement_corrections.jsonl");
    let mut steps = journal::parse(script).unwrap();
    journal::relocate_new_projects(&mut steps, dir.path());
    journal::replay(&mut engine, &steps).unwrap();
    let job = engine
        .project()
        .unwrap()
        .output_jobs()
        .next()
        .unwrap()
        .clone();
    let preview = job_preview(&engine, &job).unwrap();
    let info = &preview.placements;
    assert_eq!(
        info.iter().map(|p| p.matched).collect::<Vec<_>>(),
        [Some(2), Some(5), Some(2), Some(5)]
    );
    assert!(info.iter().all(|p| p.stored_rotation.0 == 90_000));
    assert_eq!(
        info.iter()
            .map(|p| p.written_rotation.0)
            .collect::<Vec<_>>(),
        [30_000, 325_000, 210_000, 145_000]
    );
    assert_eq!(
        json!(info[0].suggested_match),
        json!({"footprint":"RESC1608X55N"})
    );
    assert_eq!(
        json!(info[1].suggested_match),
        json!({"field":"LCSC","value":"c25804"})
    );
    let report = engine.run_ok("output.job.run", json!({"job":job.uuid}));
    let csv = std::fs::read_to_string(
        Path::new(report["destination"].as_str().unwrap()).join("Placement-Generic-CPL.csv"),
    )
    .unwrap();
    reda_testkit::assert_golden(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/placement_corrections/CPL.csv"),
        csv.as_bytes(),
    );
    let rows = read_csv(&csv);
    assert_eq!(rows[0], preview.cpl.header);
    assert_eq!(rows[1..], preview.cpl.rows);
    // Each resistor is turned 90°. R3 and R4 also have their local X coordinate mirrored.
    // File origin is (1, 2) mm and the preset negates bottom X and bottom angles.
    let expected = [
        ("R1", "Top", 30., [2., 7., 4., 6., 4., 5.225]),
        ("R2", "Top", 325., [8., 5.5, 9., 6., 9., 5.225]),
        ("R3", "Bottom", 210., [-12., 5., -14., 6., -14., 6.775]),
        ("R4", "Bottom", 145., [-18., 6.5, -19., 6., -19., 6.775]),
    ];
    assert_eq!(rows.len(), expected.len() + 1);
    for (row, (reference, side, rotation, positions)) in rows[1..].iter().zip(expected) {
        assert_eq!((&row[0][..], &row[3][..]), (reference, side));
        assert_eq!(
            row[4].parse::<f64>().unwrap(),
            rotation,
            "{reference} rotation"
        );
        let coords = [1, 2, 5, 6, 7, 8].map(|col| row[col].parse::<f64>().unwrap());
        assert_eq!(coords, positions, "{reference} centre, origin, Pad 1");
    }
    assert_eq!(rows[2][9], "Part, \"A\"");
}
