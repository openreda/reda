//! Object Defaults (docs/spec/v0.3/input-and-defaults.md#object-defaults): the
//! `defaults.*` Preferences, with the shipped values of the Kinds and fields table, the
//! old Tool keys they replace, and the Commands that never read them.

use std::path::Path;

use reda_engine::command::UserDirs;
use reda_engine::preferences;
use reda_testkit::{TempDir, test_engine_in};
use serde_json::{Value, json};

const MIL: i64 = 25_400;

fn shipped(key: &str) -> Value {
    let prefs = preferences::parse("").values;
    match preferences::value_of(&prefs, key) {
        Some((_, value)) => value,
        None => panic!("{key} is not a Preference"),
    }
}

fn write_prefs(home: &Path, text: &str) {
    let path = UserDirs::under(home).preferences_file();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn remember_last_used_starts_on() {
    assert_eq!(shipped("defaults.remember_last_used"), json!(true));
}

#[test]
fn the_line_family_on_boards_and_footprints_has_a_width_per_layer_kind() {
    // The Per Layer kind table: Copper 0.15, Silkscreen 0.15, Courtyard 0.05,
    // Assembly 0.1, Mechanical 0.15 mm.
    let widths = [
        ("copper", "0.15mm"),
        ("silkscreen", "0.15mm"),
        ("courtyard", "0.05mm"),
        ("assembly", "0.1mm"),
        ("mechanical", "0.15mm"),
    ];
    for kind in ["line", "rectangle", "arc", "circle", "polygon"] {
        for (layer_kind, width) in widths {
            let key = format!("defaults.pcb.{kind}.{layer_kind}.width");
            assert_eq!(shipped(&key), json!(width), "{key}");
        }
    }
    // Not filled: the closed shapes carry the fill, the open ones have none to carry.
    for kind in ["rectangle", "circle", "polygon"] {
        for (layer_kind, _) in widths {
            let key = format!("defaults.pcb.{kind}.{layer_kind}.fill");
            assert_eq!(shipped(&key), json!(false), "{key}");
        }
    }
}

#[test]
fn text_on_boards_and_footprints_is_one_millimetre_high_in_the_stroke_font() {
    for layer_kind in [
        "copper",
        "silkscreen",
        "courtyard",
        "assembly",
        "mechanical",
    ] {
        let at = |field: &str| shipped(&format!("defaults.pcb.text.{layer_kind}.{field}"));
        assert_eq!(at("height"), json!("1mm"), "{layer_kind}");
        assert_eq!(at("thickness"), json!("0.15mm"), "{layer_kind}");
        assert_eq!(at("font"), json!("Hershey Roman Simplex"), "{layer_kind}");
    }
    // Left and bottom on a board, centred in a Footprint.
    assert_eq!(shipped("defaults.pcb.text.justify.h"), json!("left"));
    assert_eq!(shipped("defaults.pcb.text.justify.v"), json!("bottom"));
    assert_eq!(
        shipped("defaults.footprint.text.justify.h"),
        json!("center")
    );
    assert_eq!(
        shipped("defaults.footprint.text.justify.v"),
        json!("center")
    );
}

#[test]
fn the_board_kinds_ship_the_tables_values() {
    for (key, value) in [
        // Dimension
        ("defaults.pcb.dimension.width", json!("0.15mm")),
        ("defaults.pcb.dimension.extension_width", json!("0.15mm")),
        ("defaults.pcb.dimension.offset", json!("2mm")),
        ("defaults.pcb.dimension.gap", json!("0.5mm")),
        ("defaults.pcb.dimension.text_gap", json!("0.25mm")),
        ("defaults.pcb.dimension.arrow_size", json!("1mm")),
        ("defaults.pcb.dimension.arrow_length", json!("2mm")),
        ("defaults.pcb.dimension.arrow_position", json!("inside")),
        ("defaults.pcb.dimension.precision", json!(2)),
        ("defaults.pcb.dimension.units", json!("automatic")),
        ("defaults.pcb.dimension.format", json!("value_unit")),
        ("defaults.pcb.dimension.text_height", json!("1mm")),
        // Free Pad
        ("defaults.pcb.free_pad.kind", json!("plated_through_hole")),
        ("defaults.pcb.free_pad.shape", json!("circle")),
        ("defaults.pcb.free_pad.width", json!("2mm")),
        ("defaults.pcb.free_pad.height", json!("2mm")),
        ("defaults.pcb.free_pad.hole", json!("1mm")),
        // Zone
        ("defaults.pcb.zone.clearance", Value::Null),
        ("defaults.pcb.zone.min_width", Value::Null),
        ("defaults.pcb.zone.pour_over", json!("zones_only")),
        ("defaults.pcb.zone.remove_unconnected", json!(true)),
        ("defaults.pcb.zone.min_island_area", json!("0mm2")),
        // Keepout: everything but Footprints.
        ("defaults.pcb.keepout.shape", json!("polygon")),
        ("defaults.pcb.keepout.all_layers", json!(true)),
        (
            "defaults.pcb.keepout.restrict",
            json!(["tracks", "vias", "zones", "smd_pads", "tht_pads"]),
        ),
        // Via
        ("defaults.pcb.via.teardrop", json!(true)),
    ] {
        assert_eq!(shipped(key), value, "{key}");
    }
}

#[test]
fn the_library_and_sheet_kinds_ship_the_tables_values() {
    for (key, value) in [
        // Footprint Pad: SMD, rectangle, 1 × 1.5 mm, 0.8 mm hole when through-hole, 50 %
        // chamfer, no margins.
        ("defaults.footprint.pad.kind", json!("smd")),
        ("defaults.footprint.pad.shape", json!("rect")),
        ("defaults.footprint.pad.width", json!("1mm")),
        ("defaults.footprint.pad.height", json!("1.5mm")),
        ("defaults.footprint.pad.hole_width", json!("0.8mm")),
        ("defaults.footprint.pad.hole_height", json!("0.8mm")),
        ("defaults.footprint.pad.chamfer_ratio", json!(50_000)),
        ("defaults.footprint.pad.mask_margin", Value::Null),
        ("defaults.footprint.pad.paste_margin", Value::Null),
        // Symbol Pin
        ("defaults.symbol.pin.length", json!("100mil")),
        ("defaults.symbol.pin.electrical_type", json!("passive")),
        ("defaults.symbol.pin.name_visible", json!(true)),
        ("defaults.symbol.pin.number_visible", json!(true)),
        // Symbol graphics: 10 mil, not filled.
        ("defaults.symbol.line.width", json!("10mil")),
        ("defaults.symbol.arc.width", json!("10mil")),
        ("defaults.symbol.rectangle.width", json!("10mil")),
        ("defaults.symbol.rectangle.fill", json!(false)),
        ("defaults.symbol.circle.width", json!("10mil")),
        ("defaults.symbol.circle.fill", json!(false)),
        ("defaults.symbol.polygon.width", json!("10mil")),
        ("defaults.symbol.polygon.fill", json!(false)),
        // Text in Symbols and on sheets, shared: 50 mil.
        ("defaults.schematic.text.height", json!("50mil")),
        ("defaults.schematic.rectangle.fill", json!(false)),
        ("defaults.schematic.port.direction", json!("bidirectional")),
    ] {
        assert_eq!(shipped(key), value, "{key}");
    }
}

#[test]
fn records_hold_no_position_identity_net_text_or_rule_values() {
    let banned = [
        "x",
        "y",
        "at",
        "position",
        "uuid",
        "net",
        "name",
        "number",
        "text",
        "layer",
        "rotation",
        "diameter",
        "track_width",
        "via_size",
    ];
    for key in preferences::keys() {
        let Some(rest) = key.strip_prefix("defaults.") else {
            continue;
        };
        // The field is the last segment (the kinds and Layer kinds come before it).
        let field = rest.rsplit('.').next().unwrap();
        assert!(!banned.contains(&field), "{key} holds {field}");
    }
}

#[test]
fn a_record_value_is_set_and_read_back_through_the_preferences() {
    let home = TempDir::new("defaults-set");
    let mut engine = test_engine_in(home.path());
    let out = engine.run_ok(
        "app.preferences.set",
        json!({"key": "defaults.pcb.line.silkscreen.width", "value": "0.2mm"}),
    );
    assert_eq!(out["value"], json!("0.2mm"));
    let text = std::fs::read_to_string(UserDirs::under(home.path()).preferences_file()).unwrap();
    assert!(text.contains("[defaults.pcb.line.silkscreen]"), "{text}");
    let prefs = preferences::load(&UserDirs::under(home.path())).values;
    assert_eq!(prefs.defaults.pcb.line.silkscreen.width.nm().0, 200_000);
    // A width can't be negative, a text can't be flat, and a precision has at most six
    // digits.
    for (key, value) in [
        ("defaults.pcb.line.copper.width", json!("-1mm")),
        ("defaults.pcb.text.copper.height", json!(0)),
        ("defaults.pcb.dimension.precision", json!(7)),
        ("defaults.pcb.free_pad.width", json!(0)),
        ("defaults.symbol.pin.length", json!(-1)),
        ("defaults.pcb.text.copper.font", json!(" ")),
    ] {
        let err = engine.run_err("app.preferences.set", json!({"key": key, "value": value}));
        assert_eq!(
            err.code,
            reda_engine::command::ErrorCode::InvalidParams,
            "{key} = {value}"
        );
    }
}

#[test]
fn the_v0_1_zone_and_keepout_tool_keys_are_dropped_on_load_with_a_line_each() {
    let loaded = preferences::parse(
        "[pcb.zone]\nauto_refill = \"off\"\n\n[pcb.zone.tool]\nclearance = \"1mm\"\n\
         pour_over = \"all\"\n\n[pcb.keepout]\nshape = \"circle\"\n",
    );
    // The values are not carried over: the records keep their shipped values.
    assert_eq!(loaded.values.defaults.pcb.zone.clearance, None);
    assert_eq!(
        loaded.values.defaults.pcb.keepout.shape,
        preferences::KeepoutToolShape::Polygon
    );
    // Other keys of the same section still read.
    assert_eq!(
        loaded.values.pcb.zone.auto_refill,
        preferences::AutoRefill::Off
    );
    assert_eq!(
        loaded.warnings,
        vec![
            "preferences.toml: pcb.zone.tool.clearance is dropped: Object Defaults are \
             under defaults.pcb.zone now"
                .to_owned(),
            "preferences.toml: pcb.zone.tool.pour_over is dropped: Object Defaults are \
             under defaults.pcb.zone now"
                .to_owned(),
            "preferences.toml: pcb.keepout.shape is dropped: Object Defaults are under \
             defaults.pcb.keepout now"
                .to_owned(),
        ]
    );
}

#[test]
fn the_next_write_takes_the_dropped_keys_out_of_the_file() {
    let home = TempDir::new("defaults-retired");
    write_prefs(
        home.path(),
        "[pcb.zone]\nauto_refill = \"off\"\n\n[pcb.zone.tool]\nclearance = \"1mm\"\n\n\
         [pcb.keepout]\nall_layers = false\n",
    );
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "defaults.pcb.via.teardrop", "value": false}),
    );
    let text = std::fs::read_to_string(UserDirs::under(home.path()).preferences_file()).unwrap();
    assert!(!text.contains("clearance"), "{text}");
    assert!(!text.contains("all_layers"), "{text}");
    assert!(text.contains("auto_refill = \"off\""), "{text}");
    assert!(text.contains("teardrop = false"), "{text}");
    assert_eq!(preferences::parse(&text).warnings, Vec::<String>::new());
}

#[test]
fn defaults_are_on_their_own_preferences_page() {
    assert_eq!(
        preferences::pages::page_of("defaults.remember_last_used"),
        "defaults"
    );
    assert_eq!(
        preferences::pages::page_of("defaults.pcb.zone.pour_over"),
        "defaults"
    );
}

#[test]
fn a_length_keeps_the_unit_it_was_typed_in() {
    let home = TempDir::new("defaults-unit");
    let mut engine = test_engine_in(home.path());
    for (key, typed) in [
        ("defaults.symbol.pin.length", "200mil"),
        ("defaults.pcb.line.silkscreen.width", "0.2mm"),
        ("defaults.pcb.free_pad.hole", "40mil"),
        ("defaults.footprint.pad.width", "1500um"),
        ("defaults.pcb.zone.clearance", "0.5 mm"),
    ] {
        let out = engine.run_ok("app.preferences.set", json!({"key": key, "value": typed}));
        let kept = typed.replace(' ', "");
        assert_eq!(out["value"], json!(kept), "{key}");
        let got = engine.run_ok("app.preferences.get", json!({"key": key}));
        assert_eq!(got["value"], json!(kept), "{key}");
    }
    let prefs = preferences::load(&UserDirs::under(home.path())).values;
    assert_eq!(prefs.defaults.symbol.pin.length.nm().0, 200 * MIL);
    assert_eq!(prefs.defaults.pcb.line.silkscreen.width.nm().0, 200_000);
    assert_eq!(prefs.defaults.pcb.free_pad.hole.nm().0, 40 * MIL);
    assert_eq!(prefs.defaults.footprint.pad.width.nm().0, 1_500_000);
    assert_eq!(
        prefs.defaults.pcb.zone.clearance.map(|c| c.nm().0),
        Some(500_000)
    );
    let text = std::fs::read_to_string(UserDirs::under(home.path()).preferences_file()).unwrap();
    assert!(text.contains("length = \"200mil\""), "{text}");
    assert!(text.contains("width = \"0.2mm\""), "{text}");
    // The shipped 10 mil written in millimetres is another value: it is kept in the file,
    // not dropped as the default.
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "defaults.symbol.line.width", "value": "0.254mm"}),
    );
    let text = std::fs::read_to_string(UserDirs::under(home.path()).preferences_file()).unwrap();
    assert!(text.contains("width = \"0.254mm\""), "{text}");
    // A bare number is nanometres, with no unit of its own.
    let out = engine.run_ok(
        "app.preferences.set",
        json!({"key": "defaults.symbol.arc.width", "value": 300_000}),
    );
    assert_eq!(out["value"], json!(300_000));
}

#[test]
fn an_area_keeps_the_unit_it_was_typed_in() {
    let home = TempDir::new("defaults-area");
    let mut engine = test_engine_in(home.path());
    let key = "defaults.pcb.zone.min_island_area";
    let out = engine.run_ok(
        "app.preferences.set",
        json!({"key": key, "value": "0.5mm2"}),
    );
    assert_eq!(out["value"], json!("0.5mm2"));
    let prefs = preferences::load(&UserDirs::under(home.path())).values;
    assert_eq!(
        prefs.defaults.pcb.zone.min_island_area.nm2(),
        500_000_000_000
    );
    let out = engine.run_ok(
        "app.preferences.set",
        json!({"key": key, "value": "100sqmil"}),
    );
    assert_eq!(out["value"], json!("100mil2"));
    let prefs = preferences::load(&UserDirs::under(home.path())).values;
    assert_eq!(
        prefs.defaults.pcb.zone.min_island_area.nm2(),
        64_516_000_000
    );
}
