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
    let mut e = test_engine_in(&dir.path().join("home"));
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
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
