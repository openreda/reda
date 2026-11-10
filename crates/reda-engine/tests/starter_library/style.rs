//! `docs/library-style.md`, checked over the checked-in Starter Library: every Symbol
//! and Footprint file under `library/starter/`.

use std::collections::BTreeMap;

use reda_engine::format::library::read_footprint;
use reda_engine::render::{Child, Primitive, schematic_scene};
use reda_testkit::{TempDir, read_tree, test_engine_with_library};
use serde_json::{Value, json};

use crate::{parts, starter};

/// Mils in nanometres.
const MIL: i64 = 25_400;
/// The Pin grid.
const GRID: i64 = 50 * MIL;

/// Every checked-in Symbol, by file name.
fn symbols() -> BTreeMap<String, Value> {
    read_tree(&starter().join("symbols"))
        .into_iter()
        .map(|(rel, bytes)| (rel, serde_json::from_slice(&bytes).unwrap()))
        .collect()
}

/// A Pin's connection end, its body end and the Unit it is on, in nanometres.
struct PinEnds {
    number: String,
    unit: String,
    rotation: i64,
    at: (i64, i64),
    body: (i64, i64),
    length: i64,
}

fn pins(symbol: &Value) -> Vec<PinEnds> {
    symbol["pins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let at = (
                p["at"]["x"].as_i64().unwrap(),
                p["at"]["y"].as_i64().unwrap(),
            );
            let length = p["length"].as_i64().unwrap();
            let rotation = p["rotation"].as_i64().unwrap_or(0);
            let (dx, dy) = match rotation {
                0 => (1, 0),
                90_000 => (0, 1),
                180_000 => (-1, 0),
                270_000 => (0, -1),
                other => panic!("Pin {} turned {other} millidegrees", p["number"]),
            };
            PinEnds {
                number: p["number"].as_str().unwrap().to_owned(),
                unit: p["unit"].as_str().unwrap().to_owned(),
                rotation,
                at,
                body: (at.0 + dx * length, at.1 + dy * length),
                length,
            }
        })
        .collect()
}

/// The Units that have Pins.
fn units_with_pins(symbol: &Value) -> Vec<String> {
    let pins = pins(symbol);
    symbol["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["uuid"].as_str().unwrap().to_owned())
        .filter(|u| pins.iter().any(|p| &p.unit == u))
        .collect()
}

/// The rectangle graphic of `unit`, as (min, max) corners: its body when it has one.
fn rect_body(symbol: &Value, unit: &str) -> Option<((i64, i64), (i64, i64))> {
    symbol["graphics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["unit"] == unit && g["shape"]["kind"] == "rect")
        .map(|g| {
            let p = |k: &str| {
                (
                    g["shape"][k]["x"].as_i64().unwrap(),
                    g["shape"][k]["y"].as_i64().unwrap(),
                )
            };
            let (a, b) = (p("start"), p("end"));
            ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)))
        })
        .next()
}

/// A Symbol drawn as a box: every Unit with Pins has a rectangular body (ICs, `U`, and
/// connectors, `J`).
fn boxed(symbol: &Value) -> bool {
    let prefix = symbol["symbol"]["designator_prefix"].as_str().unwrap();
    matches!(prefix, "U" | "J")
        && units_with_pins(symbol)
            .iter()
            .all(|u| rect_body(symbol, u).is_some())
}

/// An IC with a rectangular body.
fn rect_ic(symbol: &Value) -> bool {
    symbol["symbol"]["designator_prefix"] == "U" && boxed(symbol)
}

fn on_grid((x, y): (i64, i64)) -> bool {
    x % GRID == 0 && y % GRID == 0
}

fn mils((x, y): (i64, i64)) -> String {
    format!("({}, {}) mil", x as f64 / MIL as f64, y as f64 / MIL as f64)
}

/// Fails with every problem `check` finds over every Symbol.
fn each_symbol(check: impl Fn(&Value, &mut Vec<String>)) {
    let mut problems = vec![];
    for (rel, symbol) in symbols() {
        let mut found = vec![];
        check(&symbol, &mut found);
        problems.extend(found.into_iter().map(|p| format!("{rel}: {p}")));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn symbol_pins_sit_on_the_50_mil_grid() {
    each_symbol(|symbol, problems| {
        for pin in pins(symbol) {
            if !on_grid(pin.at) {
                problems.push(format!("Pin {} at {}", pin.number, mils(pin.at)));
            }
        }
    });
}

#[test]
fn pins_are_100_mil_long_or_200_mil_on_large_ics() {
    each_symbol(|symbol, problems| {
        let ic = rect_ic(symbol);
        for pin in pins(symbol) {
            if pin.length != 100 * MIL && !(ic && pin.length == 200 * MIL) {
                problems.push(format!(
                    "Pin {} is {} mil long",
                    pin.number,
                    pin.length as f64 / MIL as f64
                ));
            }
        }
    });
}

#[test]
fn neighbouring_pins_on_a_box_body_are_100_mil_apart() {
    each_symbol(|symbol, problems| {
        if !boxed(symbol) {
            return;
        }
        // Pins on one side of one Unit: same direction, same edge.
        let mut sides: BTreeMap<(String, i64, i64), Vec<i64>> = BTreeMap::new();
        for pin in pins(symbol) {
            let (edge, along) = match pin.rotation {
                0 | 180_000 => (pin.at.0, pin.at.1),
                _ => (pin.at.1, pin.at.0),
            };
            sides
                .entry((pin.unit.clone(), pin.rotation, edge))
                .or_default()
                .push(along);
        }
        for ((_, rotation, _), mut along) in sides {
            along.sort();
            for pair in along.windows(2) {
                let gap = pair[1] - pair[0];
                // A gap between groups is a whole number of empty 100 mil places.
                if gap % (100 * MIL) != 0 || gap == 0 {
                    problems.push(format!(
                        "Pins turned {}° are {} mil apart",
                        rotation / 1000,
                        gap as f64 / MIL as f64
                    ));
                }
            }
        }
    });
}

#[test]
fn bodies_sit_on_the_50_mil_grid() {
    each_symbol(|symbol, problems| {
        // Every Pin meets the body on the grid.
        for pin in pins(symbol) {
            if !on_grid(pin.body) {
                problems.push(format!(
                    "Pin {} meets the body at {}",
                    pin.number,
                    mils(pin.body)
                ));
            }
        }
        if boxed(symbol) {
            for unit in units_with_pins(symbol) {
                let (lo, hi) = rect_body(symbol, &unit).unwrap();
                if !on_grid(lo) || !on_grid(hi) {
                    problems.push(format!("body from {} to {}", mils(lo), mils(hi)));
                }
            }
        }
    });
}

#[test]
fn a_rectangular_ic_body_has_pin_1_at_the_origin() {
    let mut ics = 0;
    each_symbol(|symbol, problems| {
        if !rect_ic(symbol) {
            return;
        }
        match pins(symbol).iter().find(|p| p.number == "1") {
            Some(pin) if pin.at == (0, 0) => {}
            Some(pin) => problems.push(format!("Pin 1 at {}", mils(pin.at))),
            None => problems.push("no Pin 1".into()),
        }
    });
    for symbol in symbols().values() {
        ics += usize::from(rect_ic(symbol));
    }
    // The 555, the LDO and the MCU.
    assert_eq!(ics, 3);
}

#[test]
fn op_amps_and_discretes_are_centred_on_the_body() {
    each_symbol(|symbol, problems| {
        if boxed(symbol) {
            return;
        }
        // Each Unit's body is where its Pins meet it.
        let pins = pins(symbol);
        for unit in units_with_pins(symbol) {
            let ends: Vec<(i64, i64)> = pins
                .iter()
                .filter(|p| p.unit == unit)
                .map(|p| p.body)
                .collect();
            let centre = |v: Vec<i64>| (v.iter().min().unwrap() + v.iter().max().unwrap()) / 2;
            let c = (
                centre(ends.iter().map(|e| e.0).collect()),
                centre(ends.iter().map(|e| e.1).collect()),
            );
            if c != (0, 0) {
                problems.push(format!("a Unit's body is centred on {}", mils(c)));
            }
        }
    });
}

/// The box around the ink of `items`.
fn ink<'a>(items: impl Iterator<Item = &'a Primitive>) -> Option<(i64, i64, i64, i64)> {
    items
        .filter_map(Primitive::bounds)
        .fold(None, |acc, (lo, hi)| {
            let (a, b, c, d) = (lo.x.0, lo.y.0, hi.x.0, hi.y.0);
            Some(match acc {
                None => (a, b, c, d),
                Some((x0, y0, x1, y1)) => (x0.min(a), y0.min(b), x1.max(c), y1.max(d)),
            })
        })
}

#[test]
fn symbols_store_no_field_positions_and_autoposition_puts_the_designator_over_the_value() {
    each_symbol(|symbol, problems| {
        for key in ["fields", "field_text", "texts"] {
            if !symbol[key].is_null() {
                problems.push(format!("stores `{key}`"));
            }
        }
    });

    // Every Part with a Symbol, placed unturned on one sheet.
    let parts = parts();
    let dir = TempDir::new("starter-style-fields");
    let mut e = test_engine_with_library(&starter());
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Style"}));
    let sheet = e.project().unwrap().props().documents[0];
    let mut placed = vec![];
    for (i, (name, part)) in parts
        .iter()
        .filter(|(_, p)| p["symbol"].is_string())
        .enumerate()
    {
        let (x, y) = (i % 8, i / 8);
        let out = e.run_ok(
            "schematic.component.place",
            json!({"document": sheet.to_string(), "part": part["uuid"],
                   "at": {"x": format!("{}mm", x as i64 * 40), "y": format!("{}mm", y as i64 * 40)}}),
        );
        let unit = out["result"]["unit"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: {out}"))
            .to_owned();
        placed.push((name.clone(), unit));
    }
    let scene = schematic_scene(e.project().unwrap(), sheet, &[]).unwrap();
    let mut problems = vec![];
    for (name, unit) in placed {
        let field = |want: &str| {
            ink(scene
                .items
                .iter()
                .filter(|item| {
                    matches!(&item.child, Some(Child::Field { unit: u, field })
                        if u.to_string() == unit && field == want)
                })
                .map(|item| &item.primitive))
        };
        let body = ink(scene
            .items
            .iter()
            .filter(|item| item.object.to_string() == unit && item.child.is_none())
            .map(|item| &item.primitive));
        let (Some(designator), Some(value), Some(body)) =
            (field("designator"), field("value"), body)
        else {
            problems.push(format!("{name}: no Designator, Value or body drawn"));
            continue;
        };
        if designator.1 <= value.3 {
            problems.push(format!("{name}: the Designator is not above the Value"));
        }
        if designator.0 < body.2 || value.0 < body.2 {
            problems.push(format!("{name}: a field overlaps the body"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_footprint_has_its_origin_at_the_body_centroid() {
    let mut problems = vec![];
    for (rel, bytes) in read_tree(&starter().join("footprints")) {
        let footprint = read_footprint(std::path::Path::new(&rel), &bytes).unwrap();
        let body = footprint.body_box().unwrap();
        let (cx, cy) = (
            (body.min.x.0 + body.max.x.0) / 2,
            (body.min.y.0 + body.max.y.0) / 2,
        );
        if cx.abs() > 100_000 || cy.abs() > 100_000 {
            problems.push(format!("{rel}: the body is centred on ({cx}, {cy}) nm"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_footprint_has_a_closed_courtyard_an_assembly_outline_and_designator_and_a_height() {
    let mut problems = vec![];
    let footprints = read_tree(&starter().join("footprints"));
    assert!(footprints.len() > 50, "{}", footprints.len());
    for (rel, bytes) in footprints {
        let fp: Value = serde_json::from_slice(&bytes).unwrap();
        let graphics = fp["graphics"].as_array().unwrap();
        let on = |prefix: &'static str| {
            graphics
                .iter()
                .filter(move |g| g["layer"].as_str().unwrap().starts_with(prefix))
        };
        if !on("courtyard").any(|g| {
            matches!(
                g["shape"]["kind"].as_str(),
                Some("rect" | "circle" | "polygon")
            )
        }) {
            problems.push(format!("{rel}: no closed Courtyard"));
        }
        if on("assembly").next().is_none() {
            problems.push(format!("{rel}: no Assembly outline"));
        }
        let assembly_designator = fp["texts"].as_array().unwrap().iter().any(|t| {
            t["field"] == "Designator" && t["layer"].as_str().unwrap().starts_with("assembly")
        });
        if !assembly_designator {
            problems.push(format!("{rel}: no Assembly Designator"));
        }
        if fp["footprint"]["height"].as_i64().is_none_or(|h| h <= 0) {
            problems.push(format!("{rel}: no height"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
