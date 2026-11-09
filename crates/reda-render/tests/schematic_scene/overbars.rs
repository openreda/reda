//! Overbars over negated characters of Net Label, Port, Power Port and Pin names.

use super::*;

/// The overbar strokes an object draws (capsules in its text's colour), as x ranges at
/// their height, for horizontal names.
fn bars(scene: &reda_render::Scene, object: Uuid, color: SchematicColor) -> Vec<(i64, i64, i64)> {
    scene
        .items_of(object)
        .filter(|i| i.color == reda_render::ColorKey::Schematic(color))
        .filter_map(|i| match i.primitive {
            Primitive::Capsule { a, b, .. } if a.y == b.y => Some((a.x.0, b.x.0, a.y.0)),
            _ => None,
        })
        .collect()
}

/// An object's first text run and its ink bounds.
fn run_of(scene: &reda_render::Scene, object: Uuid) -> Run {
    scene
        .items_of(object)
        .find_map(|i| match &i.primitive {
            Primitive::Glyphs(run) => Some(Run {
                text: run.text.clone(),
                bounds: i.primitive.bounds().unwrap(),
            }),
            _ => None,
        })
        .unwrap()
}

struct Run {
    text: String,
    bounds: (Point, Point),
}

fn label(engine: &mut TestEngine, name: &str, x: i64) -> Uuid {
    let out = engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": name, "at": at(x, 1000)}),
    );
    out["result"]["net_label"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn a_backslash_after_a_character_draws_an_overbar_over_it() {
    let dir = TempDir::new("render-overbar");
    let mut engine = demo(&dir);
    let reset = label(&mut engine, "R\\E\\S\\E\\T\\", 1000);
    let csn = label(&mut engine, "CSN\\", 3000);
    let plain = label(&mut engine, "VIN", 5000);
    let scene = schematic_scene(engine.project().unwrap(), sheet(), &[]).unwrap();

    // The markup is not drawn.
    assert_eq!(run_of(&scene, reset).text, "RESET");
    assert_eq!(run_of(&scene, csn).text, "CSN");

    // One bar over the whole of RESET, above its capitals.
    let (lo, hi) = run_of(&scene, reset).bounds;
    let reset_bars = bars(&scene, reset, SchematicColor::NetLabel);
    assert_eq!(reset_bars.len(), 1, "{reset_bars:?}");
    let (a, b, y) = reset_bars[0];
    assert!(
        a <= lo.x.0 + 100_000 && b >= hi.x.0 - 100_000,
        "{reset_bars:?} {lo:?} {hi:?}"
    );
    assert!(y > hi.y.0, "the bar is above the capitals");

    // Only over the N of CSN: the right third of the text.
    let (lo, hi) = run_of(&scene, csn).bounds;
    let csn_bars = bars(&scene, csn, SchematicColor::NetLabel);
    assert_eq!(csn_bars.len(), 1, "{csn_bars:?}");
    let (a, b, _) = csn_bars[0];
    let width = hi.x.0 - lo.x.0;
    assert!(
        a > lo.x.0 + width / 2 && b >= hi.x.0 - 100_000,
        "{csn_bars:?} {lo:?} {hi:?}"
    );

    assert!(bars(&scene, plain, SchematicColor::NetLabel).is_empty());
}

#[test]
fn a_leading_backslash_negates_the_whole_name_only_under_single_negation() {
    let dir = TempDir::new("render-single-negation");
    let mut engine = demo(&dir);
    let net_label = label(&mut engine, "\\RESET", 1000);
    let port: Uuid = engine.run_ok(
        "schematic.port.add",
        json!({"document": SHEET, "name": "\\WR", "at": at(1000, 3000)}),
    )["result"]["port"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let power: Uuid = engine.run_ok(
        "schematic.power_port.add",
        json!({"document": SHEET, "name": "\\EN", "at": at(4000, 3000)}),
    )["result"]["power_port"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let project = engine.project().unwrap();
    let scene = |single_negation| {
        let style = SchematicStyle {
            single_negation,
            ..SchematicStyle::default()
        };
        schematic_scene_styled(project, sheet(), &[], &style).unwrap()
    };
    let off = scene(false);
    let on = scene(true);
    // Off, the backslash is dropped and nothing is negated.
    assert_eq!(run_of(&off, net_label).text, "RESET");
    assert!(bars(&off, net_label, SchematicColor::NetLabel).is_empty());
    assert_eq!(bars(&on, net_label, SchematicColor::NetLabel).len(), 1);
    // A Power Port's bar and a Port's flag have horizontal lines of their own: the name
    // adds exactly one more.
    let power_lines = |s: &reda_render::Scene| bars(s, power, SchematicColor::PowerPort).len();
    assert_eq!(power_lines(&on), power_lines(&off) + 1);
    let port_lines = |s: &reda_render::Scene| bars(s, port, SchematicColor::Port).len();
    assert_eq!(port_lines(&on), port_lines(&off) + 1);
    assert_eq!(run_of(&on, port).text, "WR");
    assert_golden(golden("overbars.scene"), on.to_text().as_bytes());
}

#[test]
fn a_pin_name_draws_its_overbars_but_ignores_a_leading_backslash() {
    let dir = TempDir::new("render-pin-overbar");
    let mut engine = demo(&dir);
    place(&mut engine, LM358, 1000, 1000, 0);
    let project = engine.project().unwrap();
    let entry = project
        .schematic_sheet(sheet())
        .unwrap()
        .units()
        .next()
        .and_then(|u| project.schematic_sheet(sheet()).unwrap().part(u.part))
        .unwrap();
    let mut symbol = entry.symbol.clone();
    // The Power Unit shows V+ and V-.
    let unit = symbol.units[2].uuid;
    let pins: Vec<usize> = (symbol.pins.iter().enumerate())
        .filter(|(_, p)| p.unit == unit && p.name_visible)
        .map(|(i, _)| i)
        .take(2)
        .collect();
    assert_eq!(pins.len(), 2, "the Power Unit shows two Pin names");
    symbol.pins[pins[0]].name = "CS\\".into();
    symbol.pins[pins[1]].name = "\\OE".into();
    let (cs, oe) = (symbol.pins[pins[0]].uuid, symbol.pins[pins[1]].uuid);
    let scene = reda_render::symbol_scene(&symbol, unit).unwrap();
    let pin_bars = |pin| {
        scene
            .items_of(pin)
            .filter(|i| i.color == reda_render::ColorKey::Schematic(SchematicColor::PinName))
            .filter(|i| matches!(i.primitive, Primitive::Capsule { .. }))
            .count()
    };
    assert_eq!(pin_bars(cs), 1);
    assert_eq!(pin_bars(oe), 0, "Pin names have no single negation");
    let names: Vec<_> = scene
        .items_of(oe)
        .filter_map(|i| match &i.primitive {
            Primitive::Glyphs(run) if run.text.contains("OE") => Some(run.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["OE"]);
}
