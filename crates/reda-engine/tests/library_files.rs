//! Library element files (`.redalib`, `.redasym`, `.redafp`, `.redapart`): canonical bytes
//! that round-trip byte-identically, and readers that normalise hand edits.

use std::path::{Path, PathBuf};

use reda_engine::format::library::{
    read_footprint, read_library_info, read_part, read_symbol, write_footprint, write_library_info,
    write_part, write_symbol,
};
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{
    ComponentType, Field, Fields, Footprint, FootprintGenerator, FootprintGraphic, FootprintLayer,
    FootprintText, Hole, InsideEdge, LibraryInfo, OutsideEdge, Pad, PadKind, PadShape, Part, Pin,
    PinMap, PinPads, PinType, Provenance, Shape, Symbol, SymbolGraphic, TextStyle, Unit, Uuid,
};
use reda_testkit::assert_golden;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/library/Demo");

fn golden(rel: &str) -> PathBuf {
    Path::new(GOLDEN).join(rel)
}

fn id(n: u128) -> Uuid {
    Uuid::v4_from_bits(n << 96)
}

fn info() -> LibraryInfo {
    LibraryInfo {
        uuid: id(1),
        name: "Demo".into(),
        description: "A demo Library for the file-format tests".into(),
    }
}

/// A dual op-amp: Units A and B and a power Unit.
fn opamp() -> Symbol {
    let (a, b, power) = (id(0x11), id(0x12), id(0x13));
    let pin = |n: u128, unit, name: &str, number: &str, electrical_type, x, y, rot| Pin {
        uuid: id(n),
        unit,
        name: name.into(),
        number: number.into(),
        electrical_type,
        at: Point::new(Nm::from_mil(x), Nm::from_mil(y)),
        rotation: Millideg(rot),
        length: Nm::from_mil(100),
        name_visible: true,
        number_visible: true,
        description: String::new(),
        inside_edge: None,
        outside_edge: None,
    };
    let mut out = pin(0x2a, a, "OUT", "1", PinType::Output, 300, 0, 180_000);
    out.description = "Output of amplifier A".into();
    out.outside_edge = Some(OutsideEdge::Dot);
    let mut minus = pin(0x2b, a, "-", "2", PinType::Input, -300, 100, 0);
    minus.inside_edge = Some(InsideEdge::Clock);
    minus.outside_edge = Some(OutsideEdge::ActiveLowInput);
    Symbol {
        uuid: id(0x10),
        name: "OpAmp_Dual".into(),
        description: "Dual operational amplifier".into(),
        designator_prefix: "U".into(),
        units: vec![
            Unit {
                uuid: a,
                name: "A".into(),
            },
            Unit {
                uuid: b,
                name: "B".into(),
            },
            Unit {
                uuid: power,
                name: "Power".into(),
            },
        ],
        pins: vec![
            out,
            minus,
            pin(0x2c, a, "+", "3", PinType::Input, -300, -100, 0),
            pin(0x2d, power, "V-", "4", PinType::PowerIn, 0, -300, 90_000),
            pin(0x25, b, "+", "5", PinType::Input, -300, -100, 0),
            pin(0x26, b, "-", "6", PinType::Input, -300, 100, 0),
            pin(0x27, b, "OUT", "7", PinType::Output, 300, 0, 180_000),
            pin(0x28, power, "V+", "8", PinType::PowerIn, 0, 300, 270_000),
        ],
        graphics: vec![
            SymbolGraphic {
                uuid: id(0x1a),
                unit: a,
                shape: Shape::Polygon {
                    points: vec![
                        Point::new(Nm::from_mil(-200), Nm::from_mil(200)),
                        Point::new(Nm::from_mil(200), Nm(0)),
                        Point::new(Nm::from_mil(-200), Nm::from_mil(-200)),
                    ],
                },
                width: Nm::from_mil(10),
                fill: true,
            },
            SymbolGraphic {
                uuid: id(0x1b),
                unit: power,
                shape: Shape::Text {
                    at: Point::new(Nm::from_mil(50), Nm(0)),
                    text: "PWR".into(),
                    size: Nm::from_mil(50),
                    rotation: Millideg(90_000),
                },
                width: Nm::from_mil(6),
                fill: false,
            },
        ],
        provenance: None,
    }
}

/// An SOIC-8 cut down to two Pads, plus a thermal Pad that shares number 4.
fn soic() -> Footprint {
    let pad = |n: u128, number: &str, x| Pad {
        uuid: id(n),
        number: number.into(),
        kind: PadKind::Smd,
        shape: PadShape::RoundedRect,
        at: Point::new(Nm::from_um(x), Nm::from_um(-1905)),
        rotation: Millideg(0),
        width: Nm::from_um(600),
        height: Nm::from_um(1550),
        corner_ratio: Some(reda_engine::model::Ratio::percent(50)),
        chamfer_ratio: None,
        corners: None,
        offset: None,
        hole: None,
        mask: None,
        paste: None,
        side: None,
        zone_connection: None,
    };
    let mut exposed = pad(0x33, "4", 0);
    exposed.shape = PadShape::Rect;
    exposed.corner_ratio = None;
    exposed.paste = Some(reda_engine::model::Paste::Expansion {
        expansion: Nm::from_um(-100),
    });
    Footprint {
        uuid: id(0x30),
        name: "SOIC-8".into(),
        description: String::new(),
        pads: vec![pad(0x31, "1", -1905), pad(0x32, "4", 1905), exposed],
        graphics: vec![
            FootprintGraphic {
                uuid: id(0x34),
                layer: FootprintLayer::CourtyardTop,
                shape: Shape::Rect {
                    start: Point::new(Nm::from_um(-3700), Nm::from_um(-2700)),
                    end: Point::new(Nm::from_um(3700), Nm::from_um(2700)),
                },
                width: Nm::from_um(50),
                fill: false,
            },
            FootprintGraphic {
                uuid: id(0x35),
                layer: FootprintLayer::SilkscreenTop,
                shape: Shape::Arc {
                    center: Point::new(Nm::from_um(-2500), Nm(0)),
                    start: Point::new(Nm::from_um(-2500), Nm::from_um(500)),
                    sweep: Millideg(-180_000),
                },
                width: Nm::from_um(120),
                fill: false,
            },
        ],
        height: Some(Nm::from_um(1750)),
        texts: vec![
            FootprintText {
                at: Point::new(Nm(0), Nm::from_um(-3500)),
                ..FootprintText::designator(id(0x36))
            },
            FootprintText {
                visible: false,
                ..FootprintText::value(id(0x37))
            },
            FootprintText {
                field: "MPN".into(),
                style: TextStyle {
                    font: "Noto Sans".into(),
                    height: Nm::from_um(600),
                    thickness: Nm::from_um(90),
                },
                layer: FootprintLayer::AssemblyBottom,
                ..FootprintText::value(id(0x38))
            },
        ],
        generator: Some(Box::new(FootprintGenerator {
            command: "library.footprint.generate.soic".into(),
            params: serde_json::json!({"library": id(1), "pins": 8}),
        })),
        provenance: None,
    }
}

fn hole() -> Footprint {
    Footprint {
        uuid: id(0x40),
        name: "MountingHole_M3".into(),
        description: "3.2 mm non-plated hole".into(),
        pads: vec![Pad {
            uuid: id(0x41),
            number: String::new(),
            kind: PadKind::NonPlated,
            shape: PadShape::Circle,
            at: Point::default(),
            rotation: Millideg(0),
            width: Nm::from_um(3200),
            height: Nm::from_um(3200),
            corner_ratio: None,
            chamfer_ratio: None,
            corners: None,
            offset: None,
            hole: Some(Hole::new(Nm::from_um(3200), Nm::from_um(3200))),
            mask: Some(reda_engine::model::Mask {
                top: Some(reda_engine::model::MaskSide::Expansion {
                    expansion: Nm::from_um(50),
                }),
                bottom: Some(reda_engine::model::MaskSide::Expansion {
                    expansion: Nm::from_um(50),
                }),
                from_hole_edge: None,
            }),
            paste: None,
            side: None,
            zone_connection: None,
        }],
        graphics: vec![FootprintGraphic {
            uuid: id(0x42),
            layer: FootprintLayer::AssemblyBottom,
            shape: Shape::Circle {
                center: Point::default(),
                radius: Nm::from_um(1600),
            },
            width: Nm::from_um(100),
            fill: false,
        }],
        height: None,
        texts: vec![FootprintText::designator(id(0x43))],
        generator: None,
        provenance: None,
    }
}

fn lm358() -> Part {
    Part {
        uuid: id(0x50),
        name: "LM358".into(),
        component_type: ComponentType::Standard,
        in_bom: ComponentType::Standard.default_in_bom(),
        in_cpl: ComponentType::Standard.default_in_cpl(),
        symbol: Some(id(0x10)),
        footprints: vec![id(0x30), id(0x60)],
        fields: Fields {
            value: "LM358".into(),
            description: "Dual op-amp".into(),
            datasheet: "https://example.com/lm358.pdf".into(),
            manufacturer: "Generic".into(),
            mpn: "LM358DR".into(),
            keywords: "opamp amplifier".into(),
            custom: vec![
                Field {
                    name: "LCSC".into(),
                    value: "C7950".into(),
                },
                Field {
                    name: "DigiKey".into(),
                    value: "296-1395-1-ND".into(),
                },
            ],
        },
        pin_map: PinMap {
            pins: vec![PinPads {
                pin: "4".into(),
                pads: vec!["4".into(), "9".into()],
            }],
            unconnected: vec!["EP".into()],
        },
        provenance: None,
    }
}

fn mounting_hole() -> Part {
    Part {
        uuid: id(0x70),
        name: "MountingHole_M3".into(),
        component_type: ComponentType::Mechanical,
        in_bom: ComponentType::Mechanical.default_in_bom(),
        in_cpl: ComponentType::Mechanical.default_in_cpl(),
        symbol: None,
        footprints: vec![id(0x40)],
        fields: Fields::default(),
        pin_map: PinMap::default(),
        provenance: None,
    }
}

#[test]
fn the_writers_match_the_goldens() {
    assert_golden(
        golden("Demo.redalib"),
        &write_library_info(&info()).unwrap(),
    );
    assert_golden(
        golden("symbols/OpAmp_Dual.redasym"),
        &write_symbol(&opamp()).unwrap(),
    );
    assert_golden(
        golden("footprints/SOIC-8.redafp"),
        &write_footprint(&soic()).unwrap(),
    );
    assert_golden(
        golden("footprints/MountingHole_M3.redafp"),
        &write_footprint(&hole()).unwrap(),
    );
    assert_golden(
        golden("parts/LM358.redapart"),
        &write_part(&lm358()).unwrap(),
    );
    assert_golden(
        golden("parts/MountingHole_M3.redapart"),
        &write_part(&mounting_hole()).unwrap(),
    );
}

#[test]
fn library_files_read_back_as_the_same_elements() {
    let read = |rel: &str| (golden(rel), std::fs::read(golden(rel)).unwrap());
    let (p, b) = read("Demo.redalib");
    assert_eq!(read_library_info(&p, &b).unwrap(), info());
    let (p, b) = read("symbols/OpAmp_Dual.redasym");
    let mut expected = opamp();
    expected.pins.sort_by_key(|pin| pin.uuid);
    assert_eq!(read_symbol(&p, &b).unwrap(), expected);
    let (p, b) = read("footprints/SOIC-8.redafp");
    assert_eq!(read_footprint(&p, &b).unwrap(), soic());
    let (p, b) = read("parts/LM358.redapart");
    assert_eq!(read_part(&p, &b).unwrap(), lm358());
    let (p, b) = read("parts/MountingHole_M3.redapart");
    assert_eq!(read_part(&p, &b).unwrap(), mounting_hole());
}

/// Reads every golden file and writes it again: the bytes must not change.
#[test]
fn library_files_round_trip_byte_identically() {
    for rel in [
        "Demo.redalib",
        "symbols/OpAmp_Dual.redasym",
        "footprints/SOIC-8.redafp",
        "footprints/MountingHole_M3.redafp",
        "parts/LM358.redapart",
        "parts/MountingHole_M3.redapart",
    ] {
        let path = golden(rel);
        let bytes = std::fs::read(&path).unwrap();
        let again = rewrite(&path, &bytes);
        assert_eq!(
            String::from_utf8(again).unwrap(),
            String::from_utf8(bytes).unwrap(),
            "{rel}"
        );
    }
}

fn rewrite(path: &Path, bytes: &[u8]) -> Vec<u8> {
    match path.extension().unwrap().to_str().unwrap() {
        "redalib" => write_library_info(&read_library_info(path, bytes).unwrap()).unwrap(),
        "redasym" => write_symbol(&read_symbol(path, bytes).unwrap()).unwrap(),
        "redafp" => write_footprint(&read_footprint(path, bytes).unwrap()).unwrap(),
        "redapart" => write_part(&read_part(path, bytes).unwrap()).unwrap(),
        other => panic!("not a library file: {other}"),
    }
}

#[test]
fn hand_reformatted_files_are_normalised_on_write() {
    for rel in ["symbols/OpAmp_Dual.redasym", "parts/LM358.redapart"] {
        let path = golden(rel);
        let canonical = std::fs::read(&path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
        let pretty = serde_json::to_vec_pretty(&value).unwrap();
        assert_ne!(pretty, canonical);
        assert_eq!(rewrite(&path, &pretty), canonical, "{rel}");
    }
}

#[test]
fn element_order_inside_a_file_does_not_matter() {
    // Pins are stored sorted by UUID and pin-map overrides sorted, whatever order they
    // arrive in.
    let mut shuffled = opamp();
    shuffled.pins.reverse();
    let mut sorted = opamp();
    sorted.pins.sort_by_key(|p| p.uuid);
    assert_eq!(
        write_symbol(&shuffled).unwrap(),
        write_symbol(&sorted).unwrap()
    );

    let mut part = lm358();
    part.pin_map.unconnected = vec!["EP".into(), "A".into()];
    part.pin_map.pins.insert(
        0,
        PinPads {
            pin: "9".into(),
            pads: vec!["10".into()],
        },
    );
    let mut tidy = part.clone();
    tidy.pin_map.unconnected.sort();
    tidy.pin_map.pins.sort_by(|a, b| a.pin.cmp(&b.pin));
    assert_eq!(write_part(&part).unwrap(), write_part(&tidy).unwrap());
}

#[test]
fn unknown_fields_and_wrong_kinds_are_refused() {
    let path = golden("Demo.redalib");
    let err = read_library_info(
        &path,
        br#"{"format":1,"library":{"uuid":"00000000-0000-4000-8000-000000000001","name":"x","colour":"red"}}"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("colour"), "{err}");

    let symbol = std::fs::read(golden("symbols/OpAmp_Dual.redasym")).unwrap();
    let err = read_part(&golden("parts/x.redapart"), &symbol).unwrap_err();
    assert!(err.to_string().contains("part"), "{err}");
}

#[test]
fn a_pin_on_an_unknown_unit_is_refused() {
    let mut symbol = opamp();
    symbol.pins[0].unit = id(0x99);
    let bytes = write_symbol(&symbol).unwrap();
    let err = read_symbol(&golden("symbols/x.redasym"), &bytes).unwrap_err();
    assert!(err.to_string().contains("unit"), "{err}");
}

/// An imported element keeps its provenance through a write and a read, on one line after
/// the element's own row; elements made in REDA have no such line.
#[test]
fn provenance_is_written_on_its_own_line_and_read_back() {
    let provenance = Provenance {
        service: "SnapMagic".into(),
        terms_url: "https://www.snapeda.com/about/terms/".into(),
        copyright: vec!["Copyright (C) 2026 Example Corp.".into()],
    };
    let line = r#""provenance": {"service":"SnapMagic","terms_url":"https://www.snapeda.com/about/terms/","copyright":["Copyright (C) 2026 Example Corp."]},"#;

    let mut symbol = opamp();
    symbol.provenance = Some(Box::new(provenance.clone()));
    let bytes = write_symbol(&symbol).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert_eq!(
        text.lines().nth(3),
        Some(format!("  {line}").as_str()),
        "{text}"
    );
    let read = read_symbol(Path::new("s.redasym"), &bytes).unwrap();
    assert_eq!(read.provenance, symbol.provenance);
    assert_eq!(write_symbol(&read).unwrap(), bytes);

    let mut footprint = soic();
    footprint.provenance = Some(Box::new(provenance.clone()));
    let bytes = write_footprint(&footprint).unwrap();
    assert!(String::from_utf8(bytes.clone()).unwrap().contains(line));
    assert_eq!(
        read_footprint(Path::new("f.redafp"), &bytes).unwrap(),
        footprint
    );

    let mut part = lm358();
    part.provenance = Some(Box::new(provenance));
    let bytes = write_part(&part).unwrap();
    assert!(String::from_utf8(bytes.clone()).unwrap().contains(line));
    assert_eq!(read_part(Path::new("p.redapart"), &bytes).unwrap(), part);

    let plain = String::from_utf8(write_part(&lm358()).unwrap()).unwrap();
    assert!(!plain.contains("provenance"));
}

/// A Footprint's `generator` stays in its `.redafp`: the copy a Document takes leaves it
/// out, and so does the hash, so regenerating to the same geometry changes nothing placed.
#[test]
fn the_generator_stays_out_of_the_copy_and_its_hash() {
    use reda_engine::library::{DiskLibraryFiles, copy_part};
    use reda_library::{LibraryRoot, Origin, scan};

    let dir = reda_testkit::TempDir::new("generator-hash");
    let write = |rel: &str, bytes: Vec<u8>| {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    };
    write("Demo.redalib", write_library_info(&info()).unwrap());
    write(
        "symbols/OpAmp_Dual.redasym",
        write_symbol(&opamp()).unwrap(),
    );
    let part = Part {
        footprints: vec![id(0x30)],
        ..lm358()
    };
    write("parts/LM358.redapart", write_part(&part).unwrap());
    let copied = |footprint: &Footprint| {
        write(
            "footprints/SOIC-8.redafp",
            write_footprint(footprint).unwrap(),
        );
        let read = std::fs::read(dir.path().join("footprints/SOIC-8.redafp")).unwrap();
        let read = read_footprint(Path::new("SOIC-8.redafp"), &read).unwrap();
        assert_eq!(read.generator, footprint.generator, "the file keeps it");
        let roots = [LibraryRoot::new(dir.path(), Origin::User)];
        copy_part(&scan(&roots, &mut DiskLibraryFiles), part.uuid).unwrap()
    };

    let generated = soic();
    assert!(generated.generator.is_some());
    let copy = copied(&generated);
    assert_eq!(copy.footprints[0].footprint.generator, None);
    assert_eq!(
        copy.footprints[0].footprint,
        Footprint {
            generator: None,
            ..soic()
        }
    );

    let mut other_params = soic();
    other_params.generator.as_mut().unwrap().params = serde_json::json!({"pins": 14});
    let hand_made = Footprint {
        generator: None,
        ..soic()
    };
    for footprint in [other_params, hand_made] {
        let again = copied(&footprint);
        assert_eq!(again.footprints[0].hash, copy.footprints[0].hash);
        assert_eq!(again.hash, copy.hash);
    }
}
