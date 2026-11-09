//! The CSV BOM and CPL: what goes in them, grouping and order, the CPL origin, rotation
//! and side, the Fab Preset's columns, and byte-exact goldens.

use std::path::{Path, PathBuf};

use reda_model::{
    BottomPosition, BottomRotation, Column, ColumnSource, CplSettings, CplUnits, SideValues, Uuid,
};
use reda_output::{OutputFile, bom_file, cpl_file};
use reda_testkit::{TempDir, TestEngine, assert_golden, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
/// The starter Library's Resistor (Standard) and its 0603 Footprint.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESC1608: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";
/// The starter Library's 2-pin header (Standard, through-hole by default).
const HEADER_1X02: &str = "d0560bc9-5203-4a8b-b575-24058968bdd8";
/// The starter Library's MountingHole_M3 (Mechanical, no Symbol).
const MOUNTING_HOLE_M3: &str = "326ace02-ef44-4af4-8f9d-21d6c65a74ef";

fn starter_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn engine_on(dir: &TempDir, library: &Path) -> TestEngine {
    let mut engine = test_engine_with_library(library);
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine
}

fn engine(dir: &TempDir) -> TestEngine {
    engine_on(dir, &starter_library())
}

fn place(engine: &mut TestEngine, part: &str, footprint: Option<&str>, n: usize) {
    for i in 0..n {
        engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET, "part": part, "footprint": footprint,
                   "at": {"x": format!("{}mm", 20 * i), "y": "40mm"}}),
        );
    }
}

fn sync(engine: &mut TestEngine) {
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash": hash}));
}

/// The Component UUID behind Designator `designator`.
fn component(engine: &TestEngine, designator: &str) -> Uuid {
    let sheet = engine
        .project()
        .unwrap()
        .schematic_sheet(SHEET.parse().unwrap())
        .unwrap();
    sheet
        .units()
        .find(|u| u.designator == designator)
        .unwrap_or_else(|| panic!("no {designator}"))
        .component
        .uuid
}

fn set(engine: &mut TestEngine, designators: &[&str], changes: Value) {
    let components: Vec<Uuid> = designators.iter().map(|d| component(engine, d)).collect();
    let mut params = changes;
    params["components"] = json!(components);
    engine.run_ok("schematic.component.set", params);
}

/// The Footprint instance with Designator `designator`.
fn footprint(engine: &TestEngine, designator: &str) -> Uuid {
    engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .footprints()
        .find(|f| f.designator == designator)
        .unwrap_or_else(|| panic!("no Footprint {designator}"))
        .uuid
}

/// Moves Footprint `designator` to (`x`, `y`) mm, turned `rotation` degrees.
fn put(engine: &mut TestEngine, designator: &str, x: i64, y: i64, rotation: i64) {
    let uuid = footprint(engine, designator);
    let at = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .footprint(uuid)
        .unwrap()
        .at;
    engine.run_ok(
        "pcb.move",
        json!({"objects": [uuid], "by": {"x": x * 1_000_000 - at.x.0, "y": y * 1_000_000 - at.y.0}}),
    );
    if rotation != 0 {
        engine.run_ok(
            "pcb.rotate",
            json!({"objects": [uuid], "angle": format!("{rotation}deg")}),
        );
    }
}

fn preset_columns(engine: &TestEngine) -> (Vec<Column>, Vec<Column>) {
    let preset = engine
        .project()
        .unwrap()
        .fab_presets()
        .next()
        .unwrap()
        .preset();
    (preset.bom.columns, preset.cpl.columns)
}

fn bom_with(engine: &TestEngine, columns: &[Column]) -> String {
    let project = engine.project().unwrap();
    text(bom_file(
        project,
        project.pcb().unwrap(),
        columns,
        &default_mount(),
    ))
}

fn cpl_with(engine: &TestEngine, columns: &[Column]) -> String {
    cpl_settings(
        engine,
        &CplSettings {
            columns: columns.to_vec(),
            ..Default::default()
        },
    )
}

/// The CPL with `settings`, measured from design zero.
fn cpl_settings(engine: &TestEngine, settings: &CplSettings) -> String {
    let project = engine.project().unwrap();
    text(cpl_file(
        project,
        &[reda_output::BoardInstance::single(
            project.pcb().unwrap(),
            &Default::default(),
            reda_model::FileOrigin::Absolute,
        )],
        settings,
        &default_mount(),
    ))
}

/// The default mount spellings, `SMT` and `THT`.
fn default_mount() -> reda_model::MountValues {
    reda_model::BomSettings::default().mount_values
}

fn bom(engine: &TestEngine) -> String {
    bom_with(engine, &preset_columns(engine).0)
}

fn cpl(engine: &TestEngine) -> String {
    cpl_with(engine, &preset_columns(engine).1)
}

fn text(file: OutputFile) -> String {
    String::from_utf8(file.bytes).unwrap()
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn columns(list: &[(&str, ColumnSource)]) -> Vec<Column> {
    list.iter()
        .map(|(header, source)| Column::new(header, source.clone()))
        .collect()
}

#[test]
fn the_files_are_named_after_the_project() {
    let dir = TempDir::new("assembly-names");
    let engine = engine(&dir);
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let (b, c) = preset_columns(&engine);
    let bom = bom_file(project, pcb, &b, &default_mount());
    assert_eq!((bom.name.as_str(), bom.function), ("Demo-BOM.csv", "bom"));
    let cpl = cpl_file(
        project,
        &[reda_output::BoardInstance::single(
            pcb,
            &Default::default(),
            reda_model::FileOrigin::Absolute,
        )],
        &CplSettings {
            columns: c,
            ..Default::default()
        },
        &default_mount(),
    );
    assert_eq!((cpl.name.as_str(), cpl.function), ("Demo-CPL.csv", "cpl"));
    assert_eq!(
        String::from_utf8(cpl.bytes).unwrap(),
        "Designator,Mid X,Mid Y,Layer,Rotation\n",
        "an empty board still has the header"
    );
}

#[test]
fn bom_rows_group_part_footprint_and_value_with_designators_in_natural_order() {
    let dir = TempDir::new("assembly-groups");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 10);
    set(
        &mut engine,
        &["R3", "R4", "R5", "R6", "R7", "R8", "R9"],
        json!({"value": "1k"}),
    );
    sync(&mut engine);
    let cols = columns(&[
        ("Designator", ColumnSource::Designator),
        ("Qty", ColumnSource::Quantity),
        ("Value", ColumnSource::Value),
        ("Footprint", ColumnSource::Footprint),
    ]);
    assert_eq!(
        lines(&bom_with(&engine, &cols)),
        [
            "Designator,Qty,Value,Footprint",
            "\"R1, R2, R10\",3,10k,RESC1608X55N",
            "\"R3, R4, R5, R6, R7, R8, R9\",7,1k,RESC1608X55N",
        ]
    );
}

#[test]
fn a_different_footprint_or_field_makes_its_own_row() {
    let dir = TempDir::new("assembly-split");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 3);
    place(&mut engine, RESISTOR, None, 1);
    set(&mut engine, &["R2"], json!({"fields": {"LCSC": "C25804"}}));
    sync(&mut engine);
    let cols = columns(&[
        ("Designator", ColumnSource::Designator),
        ("Footprint", ColumnSource::Footprint),
        ("LCSC", ColumnSource::Field("LCSC".into())),
    ]);
    assert_eq!(
        lines(&bom_with(&engine, &cols)),
        [
            "Designator,Footprint,LCSC",
            "\"R1, R3\",RESC1608X55N,",
            "R2,RESC1608X55N,C25804",
            "R4,RESC1005X40N,",
        ]
    );
}

#[test]
fn the_bom_and_cpl_follow_in_bom_in_cpl_and_not_fitted() {
    let dir = TempDir::new("assembly-left-out");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 4);
    set(&mut engine, &["R2"], json!({"not_fitted": true}));
    set(&mut engine, &["R3"], json!({"in_cpl": false}));
    set(&mut engine, &["R4"], json!({"in_bom": false}));
    sync(&mut engine);
    // The starter mounting hole is Mechanical with both flags off.
    engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": "3mm", "y": "3mm"}}),
    );
    assert_eq!(
        lines(&bom(&engine)),
        [
            "Designator,Quantity,Value,Footprint,Manufacturer,MPN",
            "\"R1, R3\",2,10k,RESC1608X55N,,",
        ]
    );
    assert_eq!(designators(&cpl(&engine)), ["R1", "R4"]);
}

#[test]
fn a_mechanical_part_on_the_board_is_listed_by_its_flags_not_its_type() {
    let dir = TempDir::new("assembly-mechanical-flags");
    let mut engine = engine(&dir);
    let placed = engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": "3mm", "y": "3mm"}}),
    );
    engine.run_ok(
        "pcb.mechanical.set",
        json!({"footprints": [placed["result"]["footprint"]], "in_bom": true, "in_cpl": true}),
    );
    assert_eq!(designators(&bom(&engine)), ["H1"]);
    assert_eq!(designators(&cpl(&engine)), ["H1"]);
}

/// The Designators a BOM or CPL lists, one per row (the first of a BOM row's).
fn designators(text: &str) -> Vec<&str> {
    lines(text)[1..]
        .iter()
        .map(|l| l.trim_start_matches('"').split([',', '"']).next().unwrap())
        .collect()
}

/// A Mechanical Part with a Symbol and no Footprint, such as a screw.
const SCREW: &str = "5c4e3a10-0000-4000-8000-000000000001";

/// A copy of the starter Library whose Resistor is a Mechanical Part (In BOM, not In
/// CPL), with a footprintless Mechanical [`SCREW`] beside it.
fn mechanical_library(dir: &TempDir) -> PathBuf {
    let library = dir.path().join("library");
    copy_dir(&starter_library(), &library);
    let part = library.join("parts/Resistor.redapart");
    let text = std::fs::read_to_string(&part).unwrap().replace(
        "\"component_type\":\"standard\",\"in_bom\":true,\"in_cpl\":true",
        "\"component_type\":\"mechanical\",\"in_bom\":true,\"in_cpl\":false",
    );
    std::fs::write(&part, &text).unwrap();
    let footprints = text.find("\"footprints\"").unwrap();
    let fields = text.find("  \"fields\"").unwrap();
    let screw = format!(
        "{}\"footprints\": [],\n{}",
        &text[..footprints],
        &text[fields..]
    )
    .replace(RESISTOR, SCREW)
    .replace("\"name\":\"Resistor\"", "\"name\":\"Screw\"")
    .replace("\"value\":\"10k\"", "\"value\":\"M3 screw\"");
    std::fs::write(library.join("parts/Screw.redapart"), screw).unwrap();
    library
}

#[test]
fn a_mechanical_component_on_a_sheet_reaches_the_bom_with_or_without_a_footprint() {
    let dir = TempDir::new("assembly-mechanical-sheet");
    let library = mechanical_library(&dir);
    let mut engine = engine_on(&dir, &library);
    place(&mut engine, RESISTOR, Some(RESC1608), 1);
    place(&mut engine, HEADER_1X02, None, 1);
    // The screw has no Footprint, so it stays on the schematic.
    place(&mut engine, SCREW, None, 2);
    set(&mut engine, &["R3"], json!({"not_fitted": true}));
    sync(&mut engine);
    assert_eq!(
        lines(&bom(&engine)),
        [
            "Designator,Quantity,Value,Footprint,Manufacturer,MPN",
            "J1,1,Header 1x2,HDRV0W64P254R1L508T254H850B,,",
            "R1,1,10k,RESC1608X55N,,",
            "R2,1,M3 screw,,,",
        ]
    );
    assert_eq!(designators(&cpl(&engine)), ["J1"]);
}

#[test]
fn cpl_positions_are_from_the_board_outline_lower_left_without_an_output_origin() {
    let dir = TempDir::new("assembly-origin");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 2);
    sync(&mut engine);
    // A board from (10, 5) whose left edge bulges out to x = 0 with a half circle.
    engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": [
            {"x": "10mm", "y": "5mm"}, {"x": "40mm", "y": "5mm"},
            {"x": "40mm", "y": "25mm"},
            {"x": "10mm", "y": "25mm", "sweep": "180deg"},
        ]}),
    );
    put(&mut engine, "R1", 20, 15, 0);
    put(&mut engine, "R2", 7, 6, 0);
    assert_eq!(
        lines(&relative_cpl(&engine))[1..],
        [
            "R1,20.000000,10.000000,Top,0.000",
            "R2,7.000000,1.000000,Top,0.000",
        ]
    );

    engine.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "20mm", "y": "10mm"}}),
    );
    assert_eq!(
        lines(&relative_cpl(&engine))[1..],
        [
            "R1,0.000000,5.000000,Top,0.000",
            "R2,-13.000000,-4.000000,Top,0.000",
        ],
        "a Output Origin wins"
    );
}

#[test]
fn cpl_rotation_is_the_footprint_turn_and_layer_its_side() {
    let dir = TempDir::new("assembly-rotation");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 3);
    sync(&mut engine);
    put(&mut engine, "R1", 5, 5, 90);
    put(&mut engine, "R2", 10, 5, -45);
    put(&mut engine, "R3", 15, 5, 30);
    engine.run_ok("pcb.flip", json!({"objects": [footprint(&engine, "R3")]}));
    assert_eq!(
        lines(&cpl(&engine))[1..],
        [
            "R1,5.000000,5.000000,Top,90.000",
            "R2,10.000000,5.000000,Top,315.000",
            "R3,15.000000,5.000000,Bottom,150.000",
        ],
        "Generic writes a bottom part's rotation plus 180"
    );
}

#[test]
fn common_column_sources_follow_the_preset_mapping() {
    let dir = TempDir::new("assembly-columns");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 2);
    place(&mut engine, HEADER_1X02, None, 1);
    set(
        &mut engine,
        &["R1", "R2"],
        json!({"fields": {"manufacturer": "Yageo", "mpn": "RC0603FR-0710KL",
                          "LCSC": "C25804"}}),
    );
    set(&mut engine, &["J1"], json!({"value": "PWR, 5V \"main\""}));
    sync(&mut engine);
    put(&mut engine, "R1", 5, 5, 0);
    put(&mut engine, "R2", 10, 5, 0);
    put(&mut engine, "J1", 20, 10, 0);
    let all = columns(&[
        ("#", ColumnSource::Line),
        ("Ref", ColumnSource::Designator),
        ("N", ColumnSource::Quantity),
        ("Val", ColumnSource::Value),
        ("Pkg", ColumnSource::Footprint),
        ("Type", ColumnSource::Mount),
        ("X", ColumnSource::CentreX),
        ("Y", ColumnSource::CentreY),
        ("Side", ColumnSource::Side),
        ("Rot", ColumnSource::Rotation),
        ("Maker", ColumnSource::Field("Manufacturer".into())),
        ("Part Number", ColumnSource::Field("MPN".into())),
        ("LCSC Part #", ColumnSource::Field("LCSC".into())),
    ]);
    assert_eq!(
        lines(&bom_with(&engine, &all)),
        [
            "#,Ref,N,Val,Pkg,Type,X,Y,Side,Rot,Maker,Part Number,LCSC Part #",
            "1,J1,1,\"PWR, 5V \"\"main\"\"\",HDRV0W64P254R1L508T254H850B,THT,,,,,,,",
            "2,\"R1, R2\",2,10k,RESC1608X55N,SMT,,,,,Yageo,RC0603FR-0710KL,C25804",
        ],
        "position columns are blank in a BOM"
    );
    assert_eq!(
        lines(&cpl_with(&engine, &all))[1..],
        [
            "1,J1,1,\"PWR, 5V \"\"main\"\"\",HDRV0W64P254R1L508T254H850B,THT,20.000000,10.000000,Top,0.000,,,",
            "2,R1,1,10k,RESC1608X55N,SMT,5.000000,5.000000,Top,0.000,Yageo,RC0603FR-0710KL,C25804",
            "3,R2,1,10k,RESC1608X55N,SMT,10.000000,5.000000,Top,0.000,Yageo,RC0603FR-0710KL,C25804",
        ]
    );
}

/// A small assembled board for the goldens: two values of Resistor, a header, one
/// Not Fitted part, a bottom-side part and a Mechanical Part.
fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = engine(dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 4);
    place(&mut engine, HEADER_1X02, None, 1);
    set(&mut engine, &["R3"], json!({"value": "4k7"}));
    set(&mut engine, &["R4"], json!({"not_fitted": true}));
    set(
        &mut engine,
        &["R1", "R2"],
        json!({"fields": {"manufacturer": "Yageo", "mpn": "RC0603FR-0710KL",
                          "LCSC": "C25804"}}),
    );
    sync(&mut engine);
    put(&mut engine, "R1", 5, 5, 0);
    put(&mut engine, "R2", 10, 5, 90);
    put(&mut engine, "R3", 15, 5, 0);
    put(&mut engine, "R4", 20, 5, 0);
    put(&mut engine, "J1", 25, 15, 270);
    engine.run_ok("pcb.flip", json!({"objects": [footprint(&engine, "R3")]}));
    engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": "3mm", "y": "3mm"}}),
    );
    engine
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/assembly")
        .join(name)
}

#[test]
fn bom_and_cpl_match_their_goldens_for_every_shipped_preset() {
    let dir = TempDir::new("assembly-golden");
    let engine = demo(&dir);
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    for preset in reda_output::shipped() {
        let bom = bom_file(project, pcb, &preset.bom.columns, &preset.bom.mount_values);
        assert_golden(golden(&format!("{}-BOM.csv", preset.name)), &bom.bytes);
        let cpl = cpl_file(
            project,
            &[reda_output::BoardInstance::single(
                pcb,
                &Default::default(),
                preset.gerber.file_origin,
            )],
            &preset.cpl,
            &preset.bom.mount_values,
        );
        assert_golden(golden(&format!("{}-CPL.csv", preset.name)), &cpl.bytes);
    }
}

fn relative_cpl(engine: &TestEngine) -> String {
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let preset = project.fab_presets().next().unwrap().preset();
    text(cpl_file(
        project,
        &[reda_output::BoardInstance::single(
            pcb,
            &Default::default(),
            reda_model::FileOrigin::Relative,
        )],
        &preset.cpl,
        &preset.bom.mount_values,
    ))
}

/// The CPL columns the key tests below read: Designator, centre, side and rotation.
fn key_columns() -> Vec<Column> {
    columns(&[
        ("Ref", ColumnSource::Designator),
        ("X", ColumnSource::CentreX),
        ("Y", ColumnSource::CentreY),
        ("Side", ColumnSource::Side),
        ("Rot", ColumnSource::Rotation),
    ])
}

/// A copy of the starter Library whose 2-pin header has Pad 1 at (1, 0) and Pad 2 at
/// (1, −2.54) mm, away from its origin, plus a non-plated hole at (6, 5) mm.
fn offset_header_library(dir: &TempDir) -> PathBuf {
    let library = dir.path().join("library");
    copy_dir(&starter_library(), &library);
    let file = library.join("footprints/HDRV0W64P254R1L508T254H850B.redafp");
    let text = std::fs::read_to_string(&file).unwrap();
    let pad2 = r#""at":{"x":0,"y":-1270000},"rotation":0,"width":1650000,"height":1650000,"hole":{"width":1150000,"height":1150000}}"#;
    assert!(text.contains(pad2));
    let text = text
        .replace(r#""at":{"x":0,"y":1270000}"#, r#""at":{"x":1000000,"y":0}"#)
        .replace(
            pad2,
            &format!(
                "{},\n    {}",
                pad2.replace("\"y\":-1270000", "\"y\":-2540000")
                    .replace("\"x\":0", "\"x\":1000000"),
                r#"{"uuid":"5c4e3a10-0000-4000-8000-0000000000f1","number":"","kind":"non_plated","shape":"circle","at":{"x":6000000,"y":5000000},"rotation":0,"width":1000000,"height":1000000,"hole":{"width":1000000,"height":1000000}}"#
            ),
        );
    std::fs::write(&file, text).unwrap();
    library
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn cpl_positions_are_the_copper_pad_centre_the_origin_or_pad_1() {
    let dir = TempDir::new("assembly-positions");
    let library = offset_header_library(&dir);
    let mut engine = engine_on(&dir, &library);
    place(&mut engine, HEADER_1X02, None, 1);
    sync(&mut engine);
    put(&mut engine, "J1", 20, 10, 90);
    // Only a non-plated hole, so no copper Pads: every position is the origin.
    let hole = engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": "3mm", "y": "4mm"}}),
    );
    engine.run_ok(
        "pcb.mechanical.set",
        json!({"footprints": [hole["result"]["footprint"]], "in_cpl": true}),
    );
    let positions = columns(&[
        ("Ref", ColumnSource::Designator),
        ("Mid X", ColumnSource::CentreX),
        ("Mid Y", ColumnSource::CentreY),
        ("Ref X", ColumnSource::OriginX),
        ("Ref Y", ColumnSource::OriginY),
        ("Pad X", ColumnSource::Pad1X),
        ("Pad Y", ColumnSource::Pad1Y),
    ]);
    // Turned 90°: the Pads' centre (1, −1.27) lands at (21.27, 11), Pad 1 (1, 0) at
    // (20, 11); the non-plated hole at (6, 5) doesn't widen the box.
    assert_eq!(
        lines(&cpl_with(&engine, &positions))[1..],
        [
            "H1,3.000000,4.000000,3.000000,4.000000,3.000000,4.000000",
            "J1,21.270000,11.000000,20.000000,10.000000,20.000000,11.000000",
        ]
    );
    engine.run_ok("pcb.flip", json!({"objects": [footprint(&engine, "J1")]}));
    let flipped = engine.project().unwrap().pcb().unwrap();
    let j1 = flipped.footprint(footprint(&engine, "J1")).unwrap();
    assert_eq!(j1.side, reda_model::Side::Bottom);
    // Flipping mirrors the part left-right on the board (its 90° becomes 270°): the
    // centre lands 1.27 left and 1 up of J1's origin, Pad 1 still 1 up.
    let (x, y) = (j1.at.x.0, j1.at.y.0);
    let mm = |nm: i64| format!("{}.{:06}", nm / 1_000_000, nm % 1_000_000);
    assert_eq!(
        lines(&cpl_with(&engine, &positions))[2],
        format!(
            "J1,{},{},{},{},{},{}",
            mm(x - 1_270_000),
            mm(y + 1_000_000),
            mm(x),
            mm(y),
            mm(x),
            mm(y + 1_000_000)
        )
    );
}

#[test]
fn bottom_rotation_writes_a_flipped_30_degree_part_as_330_150_or_30() {
    let dir = TempDir::new("assembly-bottom-rotation");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 2);
    sync(&mut engine);
    put(&mut engine, "R1", 5, 5, 30);
    put(&mut engine, "R2", 10, 5, 30);
    engine.run_ok("pcb.flip", json!({"objects": [footprint(&engine, "R2")]}));
    for (rule, written) in [
        (BottomRotation::Stored, "330.000"),
        (BottomRotation::Plus180, "150.000"),
        (BottomRotation::Negate, "30.000"),
    ] {
        let settings = CplSettings {
            columns: key_columns(),
            bottom_rotation: rule,
            ..Default::default()
        };
        assert_eq!(
            lines(&cpl_settings(&engine, &settings))[1..],
            [
                "R1,5.000000,5.000000,Top,30.000".to_owned(),
                format!("R2,10.000000,5.000000,Bottom,{written}"),
            ],
            "{rule:?} leaves a top part alone"
        );
    }
}

#[test]
fn bottom_position_negates_x_or_y_of_bottom_parts_only() {
    let dir = TempDir::new("assembly-bottom-position");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 2);
    sync(&mut engine);
    put(&mut engine, "R1", 5, 5, 0);
    put(&mut engine, "R2", 10, 6, 0);
    engine.run_ok("pcb.flip", json!({"objects": [footprint(&engine, "R2")]}));
    let position = columns(&[
        ("Ref", ColumnSource::Designator),
        ("X", ColumnSource::CentreX),
        ("Y", ColumnSource::CentreY),
        ("RX", ColumnSource::OriginX),
        ("RY", ColumnSource::OriginY),
        ("PX", ColumnSource::Pad1X),
        ("PY", ColumnSource::Pad1Y),
    ]);
    for (rule, r2) in [
        (
            BottomPosition::TopView,
            "R2,10.000000,6.000000,10.000000,6.000000,10.775000,6.000000",
        ),
        (
            BottomPosition::NegateX,
            "R2,-10.000000,6.000000,-10.000000,6.000000,-10.775000,6.000000",
        ),
        (
            BottomPosition::NegateY,
            "R2,10.000000,-6.000000,10.000000,-6.000000,10.775000,-6.000000",
        ),
    ] {
        let settings = CplSettings {
            columns: position.clone(),
            bottom_position: rule,
            ..Default::default()
        };
        assert_eq!(
            lines(&cpl_settings(&engine, &settings))[1..],
            [
                "R1,5.000000,5.000000,5.000000,5.000000,4.225000,5.000000",
                r2
            ],
            "{rule:?}"
        );
    }
}

#[test]
fn side_values_units_and_unit_suffix_spell_the_rows() {
    let dir = TempDir::new("assembly-units");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 2);
    sync(&mut engine);
    put(&mut engine, "R1", 5, 5, 0);
    put(&mut engine, "R2", -10, 25, 0);
    engine.run_ok("pcb.flip", json!({"objects": [footprint(&engine, "R2")]}));
    let row = |units, unit_suffix| {
        let settings = CplSettings {
            columns: key_columns(),
            side_values: SideValues {
                top: "T".into(),
                bottom: "B".into(),
            },
            units,
            unit_suffix,
            ..Default::default()
        };
        lines(&cpl_settings(&engine, &settings))[1..]
            .iter()
            .map(|l| (*l).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        row(CplUnits::Mm, false),
        [
            "R1,5.000000,5.000000,T,0.000",
            "R2,-10.000000,25.000000,B,180.000"
        ]
    );
    assert_eq!(
        row(CplUnits::Mm, true),
        [
            "R1,5.000000mm,5.000000mm,T,0.000",
            "R2,-10.000000mm,25.000000mm,B,180.000"
        ]
    );
    // 5 mm is 196.8503937 mil and 0.19685039 in, rounded to 6 decimals.
    assert_eq!(
        row(CplUnits::Mil, true),
        [
            "R1,196.850394mil,196.850394mil,T,0.000",
            "R2,-393.700787mil,984.251969mil,B,180.000"
        ]
    );
    assert_eq!(
        row(CplUnits::In, false),
        [
            "R1,0.196850,0.196850,T,0.000",
            "R2,-0.393701,0.984252,B,180.000"
        ]
    );
}

#[test]
fn exclude_through_hole_leaves_out_footprints_with_a_plated_hole() {
    let dir = TempDir::new("assembly-exclude-tht");
    let mut engine = engine(&dir);
    place(&mut engine, RESISTOR, Some(RESC1608), 1);
    place(&mut engine, HEADER_1X02, None, 1);
    sync(&mut engine);
    // A non-plated hole only: not through-hole, so it stays.
    let hole = engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": "3mm", "y": "3mm"}}),
    );
    engine.run_ok(
        "pcb.mechanical.set",
        json!({"footprints": [hole["result"]["footprint"]], "in_cpl": true}),
    );
    let mut settings = CplSettings {
        columns: key_columns(),
        ..Default::default()
    };
    assert_eq!(
        designators(&cpl_settings(&engine, &settings)),
        ["H1", "J1", "R1"]
    );
    settings.exclude_through_hole = true;
    assert_eq!(designators(&cpl_settings(&engine, &settings)), ["H1", "R1"]);
}

#[test]
fn pcbway_cpl_fills_its_reference_and_pad_columns_with_suffixed_positions() {
    let dir = TempDir::new("assembly-pcbway");
    let engine = demo(&dir);
    let project = engine.project().unwrap();
    let pcbway = reda_output::shipped_named("PCBWay").unwrap();
    let cpl = text(cpl_file(
        project,
        &[reda_output::BoardInstance::single(
            project.pcb().unwrap(),
            &Default::default(),
            reda_model::FileOrigin::Absolute,
        )],
        &pcbway.cpl,
        &pcbway.bom.mount_values,
    ));
    assert_eq!(
        lines(&cpl),
        [
            "Designator,Footprint,Mid X,Mid Y,Ref X,Ref Y,Pad X,Pad Y,TB,Rotation,Comment",
            "R1,RESC1608X55N,5.000000mm,5.000000mm,5.000000mm,5.000000mm,4.225000mm,5.000000mm,Top,0.000,10k",
            "R2,RESC1608X55N,10.000000mm,5.000000mm,10.000000mm,5.000000mm,10.000000mm,4.225000mm,Top,90.000,10k",
            "R3,RESC1608X55N,15.000000mm,5.000000mm,15.000000mm,5.000000mm,15.775000mm,5.000000mm,Bottom,180.000,4k7",
        ],
        "J1 is through-hole, so PCBWay leaves it out"
    );
}
