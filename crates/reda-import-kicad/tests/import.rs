//! Import part from a KiCad symbol and footprint: what the pure mapping makes of
//! hand-written files. Every fixture under `tests/fixtures/` was written for these tests;
//! none comes from a vendor or from KiCad's libraries (ADR-0002).

use reda_import_kicad::{ImportError, ImportOptions, Imported, Service, SourceFile, import};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    ComponentType, ConnectionStyle, Corner, Corners, FootprintLayer, Hole, PadKind, PadShape,
    PinType, Provenance, Ratio, Shape, Side, Uuid,
};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// UUIDs counting up from 1.
fn ids() -> impl FnMut() -> Uuid {
    let mut n = 0;
    move || {
        n += 1;
        Uuid::v4_from_bits(n)
    }
}

fn import_pair(symbol: &str, footprint: &str) -> Imported {
    import(
        &fixture(symbol),
        &fixture(footprint),
        &ImportOptions::default(),
        &mut ids(),
    )
    .unwrap()
}

fn mm(s: &str) -> Nm {
    Nm::parse(&format!("{s}mm")).unwrap()
}

fn pt(x: &str, y: &str) -> Point {
    Point::new(mm(x), mm(y))
}

#[test]
fn a_symbol_and_footprint_become_a_symbol_footprint_and_part() {
    let out = import_pair("divider.kicad_sym", "divider.kicad_mod");

    let symbol = &out.symbol;
    assert_eq!(symbol.name, "R_Divider");
    assert_eq!(symbol.designator_prefix, "R");
    assert_eq!(symbol.units.len(), 1);
    assert_eq!(symbol.units[0].name, "A");
    let mut pins: Vec<_> = symbol.pins.iter().collect();
    pins.sort_by(|a, b| a.number.cmp(&b.number));
    assert_eq!(pins.len(), 2);
    assert_eq!(pins[0].number, "1");
    assert_eq!(pins[0].name, "", "KiCad's `~` is an empty name");
    assert_eq!(pins[0].electrical_type, PinType::Passive);
    assert_eq!(pins[0].at, pt("0", "3.81"));
    assert_eq!(pins[0].rotation, Millideg(270_000));
    assert_eq!(pins[0].length, mm("1.27"));
    assert_eq!(pins[1].at, pt("0", "-3.81"));
    assert_eq!(pins[1].rotation, Millideg(90_000));

    let footprint = &out.footprint;
    assert_eq!(footprint.name, "DIV_0805");
    assert_eq!(
        footprint.description,
        "Example two-pad land pattern written for the REDA tests"
    );
    let mut pads: Vec<_> = footprint.pads.iter().collect();
    pads.sort_by(|a, b| a.number.cmp(&b.number));
    assert_eq!(pads.len(), 2);
    assert_eq!(pads[0].kind, PadKind::Smd);
    assert_eq!(pads[0].shape, PadShape::RoundedRect);
    assert_eq!(pads[0].at, pt("-0.9125", "0"));
    assert_eq!((pads[0].width, pads[0].height), (mm("1.025"), mm("1.4")));
    assert_eq!(pads[0].corner_ratio, Some(Ratio::percent(50)));
    assert_eq!(pads[0].outline().uniform_radius(), Some(mm("0.25625")));
    assert_eq!(pads[0].hole, None);
    assert_eq!(pads[1].rotation, Millideg(180_000));

    let part = &out.part;
    assert_eq!(part.name, "R_Divider");
    assert_eq!(part.component_type, ComponentType::Standard);
    assert_eq!(part.symbol, Some(symbol.uuid));
    assert_eq!(part.footprints, vec![footprint.uuid]);
    assert!(part.pin_map.is_empty(), "Pins bind to Pads by number");
    assert_eq!(part.fields.value, "R_Divider");
    assert_eq!(part.fields.datasheet, "https://example.com/divider.pdf");
    assert_eq!(
        part.fields.description,
        "Example resistor written for the REDA tests"
    );
    assert_eq!(part.fields.keywords, "resistor example");
    assert!(part.fields.custom.is_empty(), "{:?}", part.fields.custom);
}

#[test]
fn vendor_property_names_map_onto_built_in_fields_and_the_rest_stay_free_form() {
    let part = import_pair("dual_switch.kicad_sym", "dual_switch.kicad_mod").part;
    let f = &part.fields;
    assert_eq!(f.value, "EXAMPLE-SW2");
    assert_eq!(
        f.description,
        "Made-up dual analog switch for the REDA tests"
    );
    assert_eq!(f.manufacturer, "Example Semiconductors");
    assert_eq!(f.mpn, "EXSW2-SOP8");
    assert_eq!(f.datasheet, "", "an empty Datasheet stays empty");
    let custom: Vec<(&str, &str)> = f
        .custom
        .iter()
        .map(|c| (c.name.as_str(), c.value.as_str()))
        .collect();
    assert_eq!(
        custom,
        [
            ("Mouser", "000-EXSW2"),
            ("Height", "1.75"),
            (
                "Copyright",
                "Copyright (C) 2026 Example Parts Service. All rights reserved."
            ),
            ("SamacSys_Note", "Example note naming the service"),
        ]
    );
}

#[test]
fn other_services_names_for_the_same_fields_are_recognised() {
    let symbol = fixture("divider.kicad_sym").replace(
        r#"(property "ki_keywords""#,
        r#"(property "MF" "Example Corp") (property "MP" "EX-123")
           (property "MANUFACTURER_PART_NUMBER" "EX-123")
           (property "LCSC Part" "C000001") (property "DigiKey_Part_Number" "EX-123-ND")
           (property "ki_fp_filters" "R_*") (property "ki_keywords""#,
    );
    let part = import(
        &symbol,
        &fixture("divider.kicad_mod"),
        &ImportOptions::default(),
        &mut ids(),
    )
    .unwrap()
    .part;
    assert_eq!(part.fields.manufacturer, "Example Corp");
    assert_eq!(
        part.fields.mpn, "EX-123",
        "the duplicate MPN is not repeated"
    );
    let names: Vec<&str> = part.fields.custom.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["LCSC", "DigiKey"]);
}

#[test]
fn units_pins_and_body_drawing_carry_over_from_a_multi_unit_symbol() {
    let symbol = import_pair("dual_switch.kicad_sym", "dual_switch.kicad_mod").symbol;
    assert_eq!(symbol.designator_prefix, "U");
    let units: Vec<&str> = symbol.units.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(units, ["A", "B", "C"]);
    let unit_of = |number: &str| {
        let pin = symbol.pins.iter().find(|p| p.number == number).unwrap();
        let unit = symbol.units.iter().find(|u| u.uuid == pin.unit).unwrap();
        (unit.name.as_str(), pin)
    };
    assert_eq!(unit_of("1").0, "A");
    assert_eq!(unit_of("4").0, "B");
    let (unit, gnd) = unit_of("5");
    assert_eq!(unit, "C");
    assert!(!gnd.name_visible && !gnd.number_visible, "a hidden pin");
    assert_eq!(gnd.electrical_type, PinType::PowerIn);
    assert_eq!(unit_of("6").1.electrical_type, PinType::NotConnected);
    assert_eq!(unit_of("7").1.electrical_type, PinType::Passive);

    // The unit-0 body rectangle is drawn on every Unit; the De Morgan circle is skipped.
    let rects = symbol
        .graphics
        .iter()
        .filter(|g| matches!(g.shape, Shape::Rect { .. }))
        .count();
    assert_eq!(rects, 3);
    assert!(
        symbol
            .graphics
            .iter()
            .all(|g| !matches!(g.shape, Shape::Circle { .. }))
    );
    let rect = symbol
        .graphics
        .iter()
        .find(|g| matches!(g.shape, Shape::Rect { .. }))
        .unwrap();
    assert!(rect.fill);
    assert_eq!(rect.width, mm("0.1524"), "width 0 means KiCad's default");
    let arc = symbol
        .graphics
        .iter()
        .find_map(|g| match g.shape {
            Shape::Arc {
                center,
                start,
                sweep,
            } => Some((center, start, sweep)),
            _ => None,
        })
        .unwrap();
    assert_eq!(arc, (pt("0", "0"), pt("2.54", "0"), Millideg(180_000)));
    let text = symbol
        .graphics
        .iter()
        .find_map(|g| match &g.shape {
            Shape::Text {
                text,
                rotation,
                size,
                ..
            } => Some((text.clone(), *rotation, *size)),
            _ => None,
        })
        .unwrap();
    assert_eq!(text, ("PWR".into(), Millideg(90_000), mm("1.016")));
    assert_eq!(
        symbol
            .graphics
            .iter()
            .filter(|g| matches!(g.shape, Shape::Polygon { .. } | Shape::Line { .. }))
            .count(),
        2,
        "the open three-point polyline is two lines"
    );
}

#[test]
fn a_legacy_module_footprint_is_read_with_y_pointing_up() {
    let footprint = import_pair("dual_switch.kicad_sym", "dual_switch.kicad_mod").footprint;
    assert_eq!(footprint.name, "SOP8_EXAMPLE");
    let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
    assert_eq!(footprint.pads.len(), 9);
    assert_eq!(pad("1").kind, PadKind::PlatedThroughHole);
    assert_eq!(pad("1").shape, PadShape::Rect);
    assert_eq!(pad("1").at, pt("-3.81", "3.81"), "KiCad's y points down");
    assert_eq!(pad("1").hole, Some(Hole::new(mm("0.8"), mm("0.8"))));
    assert_eq!(pad("2").shape, PadShape::Oval);
    assert_eq!((pad("2").width, pad("2").height), (mm("1.6"), mm("1.2")));
    assert_eq!(pad("8").hole, Some(Hole::new(mm("0.8"), mm("1.2"))));
    assert_eq!(pad("8").mask_margin, Some(mm("0.05")));
    assert_eq!(pad("").kind, PadKind::NonPlated);

    let field = |name: &str| footprint.texts.iter().find(|t| t.field == name).unwrap();
    let (designator, value) = (field("Designator"), field("Value"));
    assert_eq!(designator.at, pt("0", "6.5"));
    assert_eq!(designator.rotation, Millideg(90_000));
    assert_eq!(designator.style.height, mm("1.2"));
    assert_eq!(designator.layer, FootprintLayer::SilkscreenTop);
    assert!(!value.visible);
    assert_eq!(value.layer, FootprintLayer::AssemblyTop);

    let on = |layer| footprint.graphics.iter().filter(move |g| g.layer == layer);
    assert_eq!(on(FootprintLayer::CourtyardTop).count(), 1);
    assert_eq!(on(FootprintLayer::CourtyardBottom).count(), 1);
    assert_eq!(
        on(FootprintLayer::SilkscreenTop).count(),
        3,
        "line, arc and PIN1 text"
    );
    let arc = on(FootprintLayer::SilkscreenTop)
        .find_map(|g| match g.shape {
            Shape::Arc {
                center,
                start,
                sweep,
            } => Some((center, start, sweep)),
            _ => None,
        })
        .unwrap();
    // On screen the legacy -180 arc turns anticlockwise from (-1, -5) about (0, -5).
    assert_eq!(arc, (pt("0", "5"), pt("-1", "5"), Millideg(180_000)));
    let poly = on(FootprintLayer::AssemblyTop).next().unwrap();
    assert!(poly.fill, "legacy polygons are filled");
    assert_eq!(
        footprint.graphics.len(),
        6,
        "Edge.Cuts and Cmts.User are left out"
    );
}

#[test]
fn every_element_records_the_service_its_terms_and_the_copyright_lines() {
    let out = import_pair("dual_switch.kicad_sym", "dual_switch.kicad_mod");
    let expected = Provenance {
        service: "SamacSys".into(),
        terms_url: "https://intelligence.supplyframe.com/terms/".into(),
        copyright: vec![
            "Copyright (C) 2026 Example Parts Service. All rights reserved.".into(),
            "Copyright 2026 Example Parts Service. All rights reserved.".into(),
        ],
    };
    assert_eq!(out.symbol.provenance.as_deref(), Some(&expected));
    assert_eq!(out.footprint.provenance.as_deref(), Some(&expected));
    assert_eq!(out.part.provenance.as_deref(), Some(&expected));

    let plain = import_pair("divider.kicad_sym", "divider.kicad_mod");
    let unknown = plain.part.provenance.unwrap();
    assert_eq!(
        (unknown.service.as_str(), unknown.terms_url.as_str()),
        ("Unknown", "")
    );
}

#[test]
fn the_caller_can_name_the_service() {
    let options = ImportOptions {
        service: Some(Service::Snapmagic),
        ..ImportOptions::default()
    };
    let out = import(
        &fixture("divider.kicad_sym"),
        &fixture("divider.kicad_mod"),
        &options,
        &mut ids(),
    )
    .unwrap();
    let p = out.part.provenance.unwrap();
    assert_eq!(p.service, "SnapMagic");
    assert_eq!(p.terms_url, "https://www.snapeda.com/about/terms/");
}

#[test]
fn the_generator_never_changes_the_result() {
    let symbol = fixture("divider.kicad_sym");
    let footprint = fixture("divider.kicad_mod");
    let base = import(&symbol, &footprint, &ImportOptions::default(), &mut ids()).unwrap();
    for generator in [
        "kicad_symbol_editor",
        "pcbnew",
        "snapeda",
        "https://easyeda.com/x",
    ] {
        let s = symbol.replace("handwritten_for_reda_tests", &format!("\"{generator}\""));
        let f = footprint.replace("handwritten_for_reda_tests", generator);
        let out = import(&s, &f, &ImportOptions::default(), &mut ids()).unwrap();
        assert_eq!(out, base, "generator {generator}");
    }
}

#[test]
fn three_d_models_are_ignored() {
    let with = import_pair("divider.kicad_sym", "divider.kicad_mod");
    let footprint = fixture("divider.kicad_mod");
    let start = footprint.find("  (model").unwrap();
    let end = footprint.rfind(")\n)").unwrap() + 2;
    let without = format!("{}{}", &footprint[..start], &footprint[end..]);
    let out = import(
        &fixture("divider.kicad_sym"),
        &without,
        &ImportOptions::default(),
        &mut ids(),
    )
    .unwrap();
    assert_eq!(out, with);
}

fn import_err(symbol: &str, footprint: &str, options: &ImportOptions) -> ImportError {
    import(symbol, footprint, options, &mut ids()).unwrap_err()
}

#[test]
fn symbol_libraries_older_than_kicad_6_are_refused() {
    let fp = fixture("divider.kicad_mod");
    let sym = fixture("divider.kicad_sym");
    let old = sym.replace("(version 20211014)", "(version 20200126)");
    let e = import_err(&old, &fp, &ImportOptions::default());
    assert_eq!(e.file, SourceFile::Symbol);
    assert!(e.message.contains("KiCad 6 or later"), "{e}");
    let none = sym.replace("(version 20211014) ", "");
    let e = import_err(&none, &fp, &ImportOptions::default());
    assert!(e.message.contains("no version"), "{e}");
    let oldest = sym.replace("(version 20211014)", "(version 20210619)");
    assert!(import(&oldest, &fp, &ImportOptions::default(), &mut ids()).is_ok());
    let newer = sym.replace("(version 20211014)", "(version 20251024)");
    assert!(import(&newer, &fp, &ImportOptions::default(), &mut ids()).is_ok());
}

#[test]
fn a_library_with_several_symbols_needs_a_pick() {
    let sym = fixture("two_symbols.kicad_sym");
    let fp = fixture("divider.kicad_mod");
    let e = import_err(&sym, &fp, &ImportOptions::default());
    assert!(e.message.contains("BASE_PART, DERIVED_PART"), "{e}");

    let pick = |name: &str| ImportOptions {
        symbol: Some(name.into()),
        ..ImportOptions::default()
    };
    let e = import_err(&sym, &fp, &pick("MISSING"));
    assert!(e.message.contains("no symbol \"MISSING\""), "{e}");

    // A derived symbol takes its parent's Pins and its own properties.
    let out = import(&sym, &fp, &pick("DERIVED_PART"), &mut ids()).unwrap();
    assert_eq!(out.symbol.name, "DERIVED_PART");
    assert_eq!(out.symbol.pins.len(), 2);
    assert_eq!(out.part.fields.value, "DERIVED_PART");
    assert_eq!(out.part.fields.mpn, "DP-1");
    assert_eq!(out.part.fields.manufacturer, "Base Maker", "inherited");
}

#[test]
fn the_part_name_can_be_chosen_and_names_are_made_file_safe() {
    let sym = fixture("divider.kicad_sym").replace("R_Divider", "R/Div:ider");
    let fp = fixture("divider.kicad_mod");
    let out = import(&sym, &fp, &ImportOptions::default(), &mut ids()).unwrap();
    assert_eq!(out.symbol.name, "R_Div_ider");
    assert_eq!(out.part.name, "R_Div_ider");
    let named = ImportOptions {
        name: Some("My Divider".into()),
        ..ImportOptions::default()
    };
    let out = import(&sym, &fp, &named, &mut ids()).unwrap();
    assert_eq!(out.part.name, "My Divider");
    let bad = ImportOptions {
        name: Some("a/b".into()),
        ..ImportOptions::default()
    };
    assert!(
        import_err(&sym, &fp, &bad)
            .message
            .contains("cannot be used")
    );
}

#[test]
fn broken_files_are_refused_with_the_place_of_the_problem() {
    let sym = fixture("divider.kicad_sym");
    let fp = fixture("divider.kicad_mod");
    let e = import_err(
        &sym,
        "(footprint \"X\"\n  (pad \"1\" smd rect",
        &ImportOptions::default(),
    );
    assert_eq!(e.file, SourceFile::Footprint);
    assert_eq!(e.line, Some((2, 3)));
    assert!(
        e.to_string()
            .starts_with("the footprint file, line 2, column 3:"),
        "{e}"
    );

    let e = import_err(&fp, &fp, &ImportOptions::default());
    assert!(e.message.contains("not a KiCad symbol library"), "{e}");
    let e = import_err(&sym, &sym, &ImportOptions::default());
    assert!(e.message.contains("not a KiCad footprint"), "{e}");

    let bad_number = fp.replace("(size 1.025 1.4)", "(size 1.025 1e3)");
    let e = import_err(&sym, &bad_number, &ImportOptions::default());
    assert!(e.message.contains("\"1e3\" is not a number"), "{e}");
}

#[test]
fn chamfers_corner_flags_the_drill_offset_side_and_zone_connections_carry_over() {
    let out = import_pair("divider.kicad_sym", "shaped_pads.kicad_mod");
    let pad = |n: &str| out.footprint.pads.iter().find(|p| p.number == n).unwrap();

    // KiCad's shares of the shorter side double into shares of half of it.
    let one = pad("1");
    assert_eq!(one.shape, PadShape::ChamferedRect);
    assert_eq!(one.chamfer_ratio, Some(Ratio::percent(50)));
    assert_eq!(one.corner_ratio, Some(Ratio::percent(20)));
    let corners: Corners = [Corner::TopLeft, Corner::BottomRight].into_iter().collect();
    assert_eq!(one.corners, Some(corners));
    // The y flip keeps the chamfer where it is seen: KiCad's top-left corner (negative y)
    // is REDA's top-left (positive y), cut off; the top-right corner is not.
    let outline = one.outline();
    assert!(!outline.contains(pt("-0.98", "0.48")));
    assert!(outline.contains(pt("0.98", "0.4")));
    assert!(!outline.contains(pt("0.98", "-0.48")));
    assert_eq!(
        one.zone_connection.and_then(|z| z.style),
        Some(ConnectionStyle::Solid)
    );

    // The drill offset moves the copper away from the hole, y flipped too.
    let two = pad("2");
    assert_eq!(two.at, pt("2", "0"));
    assert_eq!(two.offset, Some(pt("0.3", "0.2")));
    let z = two.zone_connection.unwrap();
    assert_eq!(z.style, Some(ConnectionStyle::Relief));
    assert_eq!((z.spoke_width, z.gap), (Some(mm("0.3")), Some(mm("0.4"))));

    let three = pad("3");
    assert_eq!(three.side, Some(Side::Bottom));
    assert_eq!(
        three.zone_connection.and_then(|z| z.style),
        Some(ConnectionStyle::None)
    );

    // A chamfered rectangle with every corner cut keeps `corners` left out.
    let four = pad("4");
    assert_eq!(four.shape, PadShape::ChamferedRect);
    assert_eq!(
        (four.chamfer_ratio, four.corners),
        (Some(Ratio::FULL), None)
    );
    assert!(
        out.warnings.iter().all(|w| !w.contains("offset")),
        "{:?}",
        out.warnings
    );
}
