//! `library.import_part`: one KiCad symbol and footprint written into a registered Library
//! as a Symbol, a Footprint and a Part, with provenance, the Library check and the
//! one-time vendor-terms notice. The KiCad files are the hand-written fixtures of
//! `reda-import-kicad`; no vendor file is used (ADR-0002).

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_engine::format::library::write_library_info;
use reda_engine::model::{LibraryInfo, Uuid};
use reda_testkit::{TempDir, TestEngine, test_engine_in};
use serde_json::{Value, json};

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-import-kicad/tests/fixtures")
        .join(name)
}

/// A Project in `dir/Demo` with the empty Project Library `dir/Demo/lib`, and per-user
/// folders under `dir/home`.
fn engine(dir: &TempDir) -> TestEngine {
    engine_with_layers(dir, 2)
}

fn engine_with_layers(dir: &TempDir, layers: u8) -> TestEngine {
    let mut e = test_engine_in(&dir.path().join("home"));
    e.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "layers": layers}),
    );
    let lib = dir.path().join("Demo/lib");
    std::fs::create_dir_all(&lib).unwrap();
    let info = LibraryInfo {
        uuid: LIBRARY.parse::<Uuid>().unwrap(),
        name: "Imports".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Imports.redalib"),
        write_library_info(&info).unwrap(),
    )
    .unwrap();
    e.run_ok("project.library.add", json!({"path": "lib"}));
    e
}

fn params(symbol: &Path, footprint: &Path) -> Value {
    json!({"library": LIBRARY, "symbol_file": symbol, "footprint_file": footprint})
}

#[test]
fn importing_writes_a_symbol_footprint_and_part_into_the_library() {
    let dir = TempDir::new("import-part");
    let mut e = engine(&dir);
    let out = e.run_ok(
        "library.import_part",
        params(&fixture("divider.kicad_sym"), &fixture("divider.kicad_mod")),
    );
    let lib = dir.path().join("Demo/lib");
    for file in [
        "symbols/R_Divider.redasym",
        "footprints/DIV_0805.redafp",
        "parts/R_Divider.redapart",
    ] {
        assert!(lib.join(file).exists(), "{file}");
    }
    assert_eq!(out["violations"], json!([]));
    assert_eq!(out["provenance"], json!({"service": "Unknown"}));

    // The Library now holds the three, linked: search finds the Part with its Footprint.
    let found = e.run_ok("library.search", json!({"text": "R_Divider"}));
    let part = &found["parts"][0];
    assert_eq!(part["uuid"], out["part"], "{found}");
    assert_eq!(part["symbol"], out["symbol"]);
    assert_eq!(part["footprints"], json!([out["footprint"]]));
    assert_eq!(
        part["fields"]["datasheet"],
        "https://example.com/divider.pdf"
    );

    let part_file = std::fs::read_to_string(lib.join("parts/R_Divider.redapart")).unwrap();
    assert!(
        part_file.contains(&format!("\"symbol\":{}", out["symbol"])),
        "{part_file}"
    );
    assert!(
        part_file.contains(r#""provenance": {"service":"Unknown"}"#),
        "{part_file}"
    );
}

#[test]
fn provenance_is_recorded_in_every_element_file() {
    let dir = TempDir::new("import-provenance");
    let mut e = engine(&dir);
    let out = e.run_ok(
        "library.import_part",
        params(
            &fixture("dual_switch.kicad_sym"),
            &fixture("dual_switch.kicad_mod"),
        ),
    );
    assert_eq!(out["provenance"]["service"], "SamacSys");
    let lib = dir.path().join("Demo/lib");
    for file in [
        "symbols/EXAMPLE-SW2.redasym",
        "footprints/SOP8_EXAMPLE.redafp",
        "parts/EXAMPLE-SW2.redapart",
    ] {
        let text = std::fs::read_to_string(lib.join(file)).unwrap();
        assert!(
            text.contains(r#""terms_url":"https://intelligence.supplyframe.com/terms/""#),
            "{file}: {text}"
        );
        assert!(
            text.contains("Copyright 2026 Example Parts Service"),
            "{file}"
        );
    }
}

#[test]
fn the_library_check_runs_on_what_was_imported() {
    let dir = TempDir::new("import-check");
    let mut e = engine(&dir);
    let footprint = std::fs::read_to_string(fixture("divider.kicad_mod")).unwrap();
    let footprint = footprint
        .lines()
        .filter(|l| !l.contains("F.CrtYd"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("(pad \"2\"", "(pad \"3\"");
    let path = dir.path().join("no_courtyard.kicad_mod");
    std::fs::write(&path, footprint).unwrap();
    let out = e.run_ok(
        "library.import_part",
        params(&fixture("divider.kicad_sym"), &path),
    );
    let checks: Vec<&str> = out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["check"].as_str().unwrap())
        .collect();
    assert_eq!(
        checks,
        ["missing_courtyard", "pin_without_pad", "unmapped_pad"]
    );
}

#[test]
fn the_imports_check_uses_the_users_severities() {
    let dir = TempDir::new("import-severities");
    let mut e = engine(&dir);
    e.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.severities.unmapped_pad", "value": "error"}),
    );
    e.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.severities.pin_without_pad", "value": "off"}),
    );
    let footprint = std::fs::read_to_string(fixture("divider.kicad_mod")).unwrap();
    let path = dir.path().join("renumbered.kicad_mod");
    std::fs::write(&path, footprint.replace("(pad \"2\"", "(pad \"3\"")).unwrap();
    let out = e.run_ok(
        "library.import_part",
        params(&fixture("divider.kicad_sym"), &path),
    );
    let found: Vec<(&str, &str)> = out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["check"].as_str().unwrap(),
                v["severity"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(found, [("unmapped_pad", "error")]);
}

#[test]
fn the_vendor_terms_notice_comes_with_the_first_import_only() {
    let dir = TempDir::new("import-notice");
    let mut e = engine(&dir);
    let first = e.run_ok(
        "library.import_part",
        params(&fixture("divider.kicad_sym"), &fixture("divider.kicad_mod")),
    );
    let notice = first["notice"].as_str().unwrap();
    assert!(notice.contains("10 files per location"), "{notice}");
    let second = e.run_ok(
        "library.import_part",
        json!({"library": LIBRARY, "symbol_file": fixture("dual_switch.kicad_sym"),
               "footprint_file": fixture("dual_switch.kicad_mod")}),
    );
    assert_eq!(second.get("notice"), None, "{second}");
}

#[test]
fn nothing_is_overwritten() {
    let dir = TempDir::new("import-twice");
    let mut e = engine(&dir);
    let p = params(&fixture("divider.kicad_sym"), &fixture("divider.kicad_mod"));
    e.run_ok("library.import_part", p.clone());
    let before = std::fs::read(dir.path().join("Demo/lib/parts/R_Divider.redapart")).unwrap();
    let err = e.run_err("library.import_part", p.clone());
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("already exists"), "{}", err.message);
    let after = std::fs::read(dir.path().join("Demo/lib/parts/R_Divider.redapart")).unwrap();
    assert_eq!(before, after);

    // Under another Part name the Symbol and Footprint files still collide.
    let mut renamed = p;
    renamed["name"] = json!("Other");
    let err = e.run_err("library.import_part", renamed);
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(!dir.path().join("Demo/lib/parts/Other.redapart").exists());
}

#[test]
fn bad_inputs_are_refused_before_anything_is_written() {
    let dir = TempDir::new("import-bad");
    let mut e = engine(&dir);
    let sym = fixture("divider.kicad_sym");
    let fp = fixture("divider.kicad_mod");

    let err = e.run_err(
        "library.import_part",
        params(&dir.path().join("none.kicad_sym"), &fp),
    );
    assert_eq!(err.code, ErrorCode::NotFound);

    let err = e.run_err("library.import_part", params(&fp, &fp));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("the symbol file"), "{}", err.message);

    let mut unknown = params(&sym, &fp);
    unknown["library"] = json!("000000bb-0000-4000-8000-000000000000");
    assert_eq!(
        e.run_err("library.import_part", unknown).code,
        ErrorCode::NotFound
    );

    assert!(!dir.path().join("Demo/lib/symbols").exists());
}

/// A placed imported Part keeps the provenance of its Part, Symbol and Footprint in the
/// sheet's copies, through a save and an open.
#[test]
fn placed_copies_keep_their_provenance() {
    let dir = TempDir::new("import-place");
    let mut e = engine(&dir);
    let out = e.run_ok(
        "library.import_part",
        params(
            &fixture("dual_switch.kicad_sym"),
            &fixture("dual_switch.kicad_mod"),
        ),
    );
    let sheet = e.run_ok("project.overview", json!({}))["documents"][0]["uuid"].clone();
    e.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": out["part"], "at": {"x": 0, "y": 0}}),
    );
    e.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let text = std::fs::read_to_string(folder.join("Demo.redasch")).unwrap();
    let terms = r#""terms_url":"https://intelligence.supplyframe.com/terms/""#;
    assert_eq!(
        text.matches(terms).count(),
        3,
        "Part, Symbol and Footprint: {text}"
    );

    let mut again = test_engine_in(&dir.path().join("home"));
    again.run_ok("project.open", json!({"path": folder}));
    again.run_ok("project.save", json!({}));
    assert_eq!(
        std::fs::read_to_string(folder.join("Demo.redasch")).unwrap(),
        text
    );
}

#[test]
fn imported_and_authored_pad_stacks_survive_placement_flipping_and_save_open() {
    use reda_engine::format::library::{read_footprint, write_footprint};
    use reda_engine::model::{
        PadShape, Side, UnusedLayers,
        geom::{CornerCut, Nm},
    };
    use reda_engine::pcb::placed::placed_pads;

    let dir = TempDir::new("import-stack-place");
    let mut e = engine_with_layers(&dir, 4);
    let folder = dir.path().join("Demo");
    let fp_path = folder.join("lib/footprints/Stacked_Header.redafp");
    let imported = e.run_ok(
        "library.import_part",
        params(
            &fixture("divider.kicad_sym"),
            &fixture("stacked_pads.kicad_mod"),
        ),
    );
    let fp_bytes = std::fs::read(&fp_path).unwrap();
    let fp = read_footprint(&fp_path, &fp_bytes).unwrap();
    assert_eq!(write_footprint(&fp).unwrap(), fp_bytes);
    // Give the second imported Pad an authored inner diameter before placing it.
    e.run_ok("library.element.open", json!({"element":fp.uuid}));
    let pad2 = fp.pads.iter().find(|p| p.number == "2").unwrap();
    e.run_ok(
        "library.footprint.pad.edit",
        json!({"footprint":fp.uuid,"pad":pad2.uuid,
        "middle":{"shape":"circle","width":"1.25mm","height":"1.25mm"}}),
    );
    e.run_ok("library.element.save", json!({"element":fp.uuid}));
    let sheet = e.run_ok("project.overview", json!({}))["documents"][0]["uuid"].clone();
    e.run_ok(
        "schematic.component.place",
        json!({"document":sheet,"part":imported["part"],"at":{"x":0,"y":0}}),
    );
    let plan = e.run_ok("sync.preview", json!({}));
    e.run_ok("sync.apply", json!({"hash":plan["hash"]}));
    let pads = |e: &TestEngine| {
        let board = e.project().unwrap().pcb().unwrap();
        assert_eq!(board.copper_layers().count(), 4);
        let instance = board.footprints().next().unwrap();
        (instance.uuid, placed_pads(board, instance))
    };
    let (instance, top) = pads(&e);
    let p = top.iter().find(|p| p.number == "1").unwrap();
    assert_eq!(
        (p.shape, p.outline.width, p.unused_layers),
        (PadShape::Rect, Nm(2_000_000), UnusedLayers::Remove)
    );
    assert_eq!(p.copper_on(None).1.width, Nm(1_200_000));
    assert_eq!(
        p.copper_on(Some(Side::Bottom)).1.corners[0],
        CornerCut::Chamfer(Nm(200_000))
    );
    let p2 = top.iter().find(|p| p.number == "2").unwrap();
    assert_eq!(p2.copper_on(None).0, PadShape::Circle);
    assert_eq!(p2.copper_on(None).1.width, Nm(1_250_000));

    e.run_ok("pcb.flip", json!({"objects":[instance]}));
    let (_, bottom) = pads(&e);
    let p = bottom.iter().find(|p| p.number == "1").unwrap();
    assert_eq!(p.shape, PadShape::ChamferedRect);
    assert_eq!(p.outline.corners[1], CornerCut::Chamfer(Nm(200_000)));
    assert_eq!(p.copper_on(Some(Side::Bottom)).1.width, Nm(2_000_000));
    assert_eq!(p.copper_on(None).1.width, Nm(1_200_000));
    e.run_ok("project.save", json!({}));
    let board_file = folder.join("Demo.redapcb");
    let before = std::fs::read(&board_file).unwrap();
    let mut reopened = test_engine_in(&dir.path().join("home"));
    reopened.run_ok("project.open", json!({"path":folder}));
    assert_eq!(pads(&reopened), (instance, bottom));
    reopened.run_ok("project.save", json!({}));
    assert_eq!(std::fs::read(&board_file).unwrap(), before);
}
