//! The starter Library that ships with REDA (`library/starter/` at the repo root) is
//! authored by a script of `reda` Commands, [`build`]. These tests rebuild it and fail
//! when the checked-in files drift from what the script makes; run
//! `REDA_BLESS=1 cargo test -p reda-engine --test starter_library` after changing the
//! script, and review the diff.
//!
//! Keep UUIDs stable for users who placed starter Parts: they are frozen in `uuids.tsv`
//! (see [`script::Uuids`]), so renaming an element means renaming its key there too.
//!
//! The hand-drawn Footprints are checked against a named datasheet drawing or IPC
//! nominal, cited in a comment beside each and in its description. Through-hole lands
//! follow IPC-2222 Level B unless the datasheet recommends a hole: hole = largest lead
//! (or lead diagonal) + 0.20 mm, pad = hole + 0.60 mm at least.

mod connectors;
mod ics;
mod mechanical;
mod passives;
mod script;
mod semis;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, read_tree, test_engine_with_library};
use serde_json::{Value, json};

use script::{LIBRARY, Script, Uuids};

/// The checked-in starter Library.
fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

/// Runs the whole script into a new Library under `dir` and returns its folder and the
/// UUIDs its elements drew.
fn build(dir: &Path) -> (PathBuf, Uuids) {
    let mut s = Script::new(dir);
    passives::build(&mut s);
    semis::build(&mut s);
    ics::build(&mut s);
    connectors::build(&mut s);
    mechanical::build(&mut s);
    (s.library().to_owned(), s.uuids())
}

#[test]
fn the_checked_in_library_is_what_the_script_builds() {
    let dir = TempDir::new("starter-library");
    let (library, uuids) = build(dir.path());
    let built = read_tree(&library);
    let target = starter();
    let unused = Uuids::checked_in().unused_keys(&uuids);
    assert!(
        unused.is_empty(),
        "uuids.tsv has keys no element drew from: {unused:?}. If you renamed an element, \
         rename its key in uuids.tsv so Projects that placed it keep finding it; if you \
         removed it, delete its lines."
    );
    if std::env::var_os("REDA_BLESS").is_some() {
        let _ = std::fs::remove_dir_all(&target);
        for (rel, bytes) in &built {
            let path = target.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        std::fs::write(script::uuids_path(), uuids.to_text()).unwrap();
        return;
    }
    assert!(
        uuids == Uuids::checked_in(),
        "uuids.tsv is not the UUIDs the script draws; run REDA_BLESS=1 cargo test -p \
         reda-engine --test starter_library and review the diff"
    );
    let checked_in = read_tree(&target);
    let mut problems = vec![];
    for (rel, bytes) in &built {
        match checked_in.get(rel) {
            None => problems.push(format!("missing: {rel}")),
            Some(old) if old != bytes => problems.push(format!("changed: {rel}")),
            Some(_) => {}
        }
    }
    for rel in checked_in.keys().filter(|rel| !built.contains_key(*rel)) {
        problems.push(format!("not made by the script: {rel}"));
    }
    assert!(
        problems.is_empty(),
        "library/starter drifted from the script; run REDA_BLESS=1 cargo test -p \
         reda-engine --test starter_library and review the diff:\n{}",
        problems.join("\n")
    );
}

/// Makes a one-Unit Symbol and a Part on it, both called `name`, and returns the UUIDs a
/// user's Project would hold on to: the Symbol, its Unit and the Part.
fn symbol_and_part(s: &mut Script, name: &str) -> [String; 3] {
    let sym = s.symbol(name, "U", "A test Symbol");
    s.pin(&sym, script::left("A", "1", "passive", -200, 0));
    let unit = sym.unit.clone();
    let symbol = s.finish_symbol(sym);
    let part = s.part(script::PartSpec {
        name,
        symbol: Some(&symbol),
        footprints: vec![],
        fields: json!({"value": name}),
        pin_map: None,
        mechanical: false,
    });
    [symbol, unit, part]
}

#[test]
fn renaming_an_element_keeps_its_uuids_once_its_key_is_renamed_in_the_uuid_table() {
    let dir = TempDir::new("starter-uuids-before");
    let mut s = Script::with_uuids(dir.path(), Uuids::default());
    let before = symbol_and_part(&mut s, "Widget");
    // The rename, as a maintainer makes it in uuids.tsv.
    let table = s
        .uuids()
        .to_text()
        .replace("symbol/Widget\t", "symbol/Gadget\t")
        .replace("part/Widget\t", "part/Gadget\t");

    let dir = TempDir::new("starter-uuids-after");
    let mut s = Script::with_uuids(dir.path(), Uuids::parse(&table));
    let after = symbol_and_part(&mut s, "Gadget");
    assert_eq!(after, before);
    assert_eq!(s.uuids().to_text(), table);
}

#[test]
fn a_new_element_gets_fresh_uuids_and_an_entry_in_the_uuid_table() {
    let dir = TempDir::new("starter-uuids-new");
    let mut s = Script::with_uuids(dir.path(), Uuids::default());
    let first = symbol_and_part(&mut s, "Widget");
    let second = symbol_and_part(&mut s, "Gadget");
    for (a, b) in first.iter().zip(&second) {
        assert_ne!(a, b);
    }
    let table = s.uuids().to_text();
    assert!(
        table.contains(&format!("part/Gadget\t{}\n", second[2])),
        "{table}"
    );
}

/// The gap kept between Silkscreen and copper, as the Footprint generators keep it.
const SILKSCREEN_CLEARANCE_MM: f64 = 0.2;

/// Points every 0.02 mm along a Footprint graphic's outline, in millimetres.
fn outline_points(shape: &Value) -> Vec<(f64, f64)> {
    let mm = |v: &Value| v.as_f64().unwrap() / 1e6;
    let p = |v: &Value| (mm(&v["x"]), mm(&v["y"]));
    let segment = |a: (f64, f64), b: (f64, f64)| {
        let n = ((b.0 - a.0).hypot(b.1 - a.1) / 0.02).ceil().max(1.0) as usize;
        (0..=n)
            .map(|i| {
                let t = i as f64 / n as f64;
                (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
            })
            .collect::<Vec<_>>()
    };
    let around = |c: (f64, f64), r: f64, from: f64, sweep: f64| {
        let n = ((r * sweep.abs()) / 0.02).ceil().max(1.0) as usize;
        (0..=n)
            .map(|i| {
                let a = from + sweep * i as f64 / n as f64;
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            })
            .collect::<Vec<_>>()
    };
    match shape["kind"].as_str().unwrap() {
        "line" => segment(p(&shape["start"]), p(&shape["end"])),
        "rect" => {
            let (a, b) = (p(&shape["start"]), p(&shape["end"]));
            let corners = [a, (b.0, a.1), b, (a.0, b.1), a];
            corners
                .windows(2)
                .flat_map(|w| segment(w[0], w[1]))
                .collect()
        }
        "polygon" => {
            let mut points: Vec<(f64, f64)> =
                shape["points"].as_array().unwrap().iter().map(p).collect();
            points.push(points[0]);
            points
                .windows(2)
                .flat_map(|w| segment(w[0], w[1]))
                .collect()
        }
        "circle" => around(
            p(&shape["center"]),
            mm(&shape["radius"]),
            0.0,
            std::f64::consts::TAU,
        ),
        "arc" => {
            let (c, s) = (p(&shape["center"]), p(&shape["start"]));
            let sweep = (shape["sweep"].as_f64().unwrap() / 1000.0).to_radians();
            around(
                c,
                (s.0 - c.0).hypot(s.1 - c.1),
                (s.1 - c.1).atan2(s.0 - c.0),
                sweep,
            )
        }
        _ => vec![],
    }
}

/// How far `point` is from a Pad's copper, in millimetres (0 inside). Rounded
/// rectangles count as their full rectangle.
fn distance_to_pad(pad: &Value, (x, y): (f64, f64)) -> f64 {
    let mm = |v: &Value| v.as_f64().unwrap() / 1e6;
    let (cx, cy) = (mm(&pad["at"]["x"]), mm(&pad["at"]["y"]));
    let (mut w, mut h) = (mm(&pad["width"]), mm(&pad["height"]));
    match pad["rotation"].as_i64().unwrap_or(0) {
        0 | 180_000 => {}
        90_000 | 270_000 => std::mem::swap(&mut w, &mut h),
        other => panic!("Pad rotated {other} millidegrees"),
    }
    let (dx, dy) = ((x - cx).abs(), (y - cy).abs());
    match pad["shape"].as_str().unwrap() {
        "circle" => ((dx).hypot(dy) - w / 2.0).max(0.0),
        "oval" => {
            // A stadium: a segment along the long side, widened by the short half-width.
            let r = w.min(h) / 2.0;
            let (sx, sy) = ((w / 2.0 - r).max(0.0), (h / 2.0 - r).max(0.0));
            ((dx - sx).max(0.0).hypot((dy - sy).max(0.0)) - r).max(0.0)
        }
        _ => (dx - w / 2.0).max(0.0).hypot((dy - h / 2.0).max(0.0)),
    }
}

#[test]
fn silkscreen_stays_clear_of_every_pad() {
    let mut problems = vec![];
    for (rel, bytes) in read_tree(&starter().join("footprints")) {
        let footprint: Value = serde_json::from_slice(&bytes).unwrap();
        let pads = footprint["pads"].as_array().unwrap();
        for graphic in footprint["graphics"].as_array().unwrap() {
            if !graphic["layer"].as_str().unwrap().starts_with("silkscreen") {
                continue;
            }
            let half_width = graphic["width"].as_f64().unwrap() / 2e6;
            let gap = outline_points(&graphic["shape"])
                .into_iter()
                .flat_map(|point| pads.iter().map(move |pad| distance_to_pad(pad, point)))
                .fold(f64::INFINITY, f64::min)
                - half_width;
            if gap < SILKSCREEN_CLEARANCE_MM - 1e-3 {
                problems.push(format!("{rel}: Silkscreen {gap:.3} mm from a Pad"));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every Part of the checked-in Library, by name.
fn parts() -> BTreeMap<String, Value> {
    let mut e = test_engine_with_library(&starter());
    let out = e.run_ok("library.search", json!({ "library": LIBRARY }));
    out["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| (hit["name"].as_str().unwrap().to_owned(), hit.clone()))
        .collect()
}

#[test]
fn the_library_check_finds_nothing_in_the_starter_library() {
    let mut e = test_engine_with_library(&starter());
    let out = e.run_ok("library.check", json!({ "library": LIBRARY }));
    assert_eq!(out["problems"], json!([]));
    assert_eq!(out["violations"], json!([]));
}

/// The names of a Part's alternate Footprints, in order.
fn footprint_names(e: &mut reda_testkit::TestEngine, part: &Value) -> Vec<String> {
    part["footprints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|fp| {
            let uuid = fp.as_str().unwrap_or_else(|| fp["uuid"].as_str().unwrap());
            let out = e.run_ok("library.element.open", json!({ "element": uuid }));
            out["name"].as_str().unwrap().to_owned()
        })
        .collect()
}

#[test]
fn generic_passives_and_discretes_have_footprint_alternates() {
    let parts = parts();
    let mut e = test_engine_with_library(&starter());
    let dir = TempDir::new("starter-open");
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Open"}));
    // Several chip sizes and a through-hole alternate for the common passives.
    for (name, chips, through_hole) in [
        ("Resistor", "RESC", "RES_Axial"),
        ("Capacitor", "CAPC", "CAP_Disc"),
        ("Capacitor_Polarised", "CAPMP", "CAPP_Radial"),
        ("LED", "LEDC", "LED_THT"),
    ] {
        let footprints = footprint_names(&mut e, &parts[name]);
        let chip_count = footprints.iter().filter(|f| f.starts_with(chips)).count();
        assert!(chip_count >= 2, "{name}: {footprints:?}");
        assert!(
            footprints.iter().any(|f| f.starts_with(through_hole)),
            "{name}: {footprints:?}"
        );
    }
    for name in [
        "Inductor",
        "Ferrite_Bead",
        "Fuse",
        "Diode",
        "Diode_Schottky",
        "Diode_Zener",
        "Button",
    ] {
        let footprints = footprint_names(&mut e, &parts[name]);
        assert!(footprints.len() >= 2, "{name}: {footprints:?}");
    }
    for name in [
        "NPN",
        "PNP",
        "NMOS",
        "PMOS",
        "NPN_TO-92",
        "PNP_TO-92",
        "Crystal_SMD",
        "Crystal_HC-49US",
        "Timer_555",
        "LDO_SOT-223",
        "STM32F042F6P6",
        "USB_C_Receptacle_USB2.0",
    ] {
        let part = &parts[name];
        assert_eq!(part["component_type"], "standard", "{name}");
        assert!(part["symbol"].is_string(), "{name}");
    }
}

#[test]
fn the_op_amp_has_two_amplifier_units_and_a_power_unit() {
    let parts = parts();
    let part = &parts["OpAmp_Dual"];
    assert!(part["footprints"].as_array().unwrap().len() >= 2);
    let mut e = test_engine_with_library(&starter());
    let dir = TempDir::new("starter-opamp");
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Open"}));
    let symbol = part["symbol"].as_str().unwrap();
    e.run_ok("library.element.open", json!({ "element": symbol }));
    let symbol = e.run_ok("object.get", json!({ "uuid": symbol }))["object"].clone();
    let units: Vec<&str> = symbol["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["name"].as_str().unwrap())
        .collect();
    assert_eq!(units, ["A", "B", "Power"]);
    let power = symbol["units"][2]["uuid"].clone();
    let mut power_pins: Vec<(&str, &str)> = symbol["pins"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["unit"] == power)
        .map(|p| {
            assert_eq!(p["electrical_type"], "power_in");
            (p["name"].as_str().unwrap(), p["number"].as_str().unwrap())
        })
        .collect();
    power_pins.sort();
    assert_eq!(power_pins, [("V+", "8"), ("V-", "4")]);
}

#[test]
fn headers_sockets_and_screw_terminals_come_in_both_pitches() {
    let parts = parts();
    for name in [
        "Header_1x02",
        "Header_1x10",
        "Header_2x05",
        "Socket_1x04",
        "Socket_2x10",
        "Screw_Terminal_1x02",
        "Screw_Terminal_1x03",
    ] {
        let part = &parts[name];
        assert_eq!(part["footprints"].as_array().unwrap().len(), 2, "{name}");
        assert!(part["symbol"].is_string(), "{name}");
    }
}

#[test]
fn mounting_holes_and_fiducials_are_mechanical_with_no_symbol_out_of_the_bom_and_cpl() {
    let parts = parts();
    for name in [
        "MountingHole_M2",
        "MountingHole_M2.5",
        "MountingHole_M3",
        "MountingHole_M3_Pad",
        "Fiducial",
    ] {
        let part = &parts[name];
        assert_eq!(part["component_type"], "mechanical", "{name}");
        assert_eq!(part["in_bom"], false, "{name}");
        assert_eq!(part["in_cpl"], false, "{name}");
        assert!(part["symbol"].is_null(), "{name}");
        assert_eq!(part["footprints"].as_array().unwrap().len(), 1, "{name}");
    }
}

#[test]
fn starter_parts_place_on_a_sheet_with_every_unit() {
    let parts = parts();
    let dir = TempDir::new("starter-place");
    let mut e = test_engine_with_library(&starter());
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let sheet = e.project().unwrap().props().documents[0].to_string();
    let place = |e: &mut reda_testkit::TestEngine, part: &str, x: &str| {
        e.run_ok(
            "schematic.component.place",
            json!({"document": sheet, "part": parts[part]["uuid"], "at": {"x": x, "y": "0mm"}}),
        )["result"]
            .clone()
    };
    let r = place(&mut e, "Resistor", "0mm");
    assert_eq!(r["designator"], "R1");
    let amp = place(&mut e, "OpAmp_Dual", "20mm");
    assert_eq!(amp["designator"], "U1");
    for (unit, x) in [("B", "40mm"), ("Power", "60mm")] {
        e.run_ok(
            "schematic.unit.place",
            json!({"document": sheet, "component": amp["component"]["uuid"], "unit": unit,
                   "at": {"x": x, "y": "0mm"}}),
        );
    }
    let ldo = place(&mut e, "LDO_SOT-223", "80mm");
    assert_eq!(ldo["designator"], "U2");
}
