//! The Library check: each finding, reported as Violations, over a scanned Library.

mod support;

use reda_library::{LibraryIndex, LibraryRoot, Origin, check, scan};
use reda_model::geom::{Nm, Point};
use reda_model::{
    ComponentType, Footprint, FootprintGraphic, FootprintLayer, Hole, LibraryCheck,
    LibrarySeverities, PadKind, PadShape, Part, PinPads, Severity, Shape, Side, Symbol, Violation,
    ViolationArg,
};
use support::*;

/// The support Symbol, with a description so it is clean.
fn symbol(n: u128, name: &str) -> Symbol {
    let mut s = support::symbol(n, name);
    s.description = format!("{name} symbol");
    s
}

/// The support Footprint, centred on its origin and described, so it is clean: Pads at
/// x = -0.5 and 0.5 mm, courtyard from (-1.1, -0.7) to (1.1, 0.7) mm.
fn footprint(n: u128, name: &str) -> Footprint {
    let mut f = support::footprint(n, name);
    f.description = format!("{name} footprint");
    for pad in &mut f.pads {
        pad.at.x = pad.at.x - Nm::from_um(1500);
    }
    f.graphics[0].shape = Shape::Rect {
        start: Point::new(Nm::from_um(-1100), Nm::from_um(-700)),
        end: Point::new(Nm::from_um(1100), Nm::from_um(700)),
    };
    f
}

/// The support Part, described.
fn part(n: u128, name: &str, symbol: u128, footprints: &[u128]) -> Part {
    let mut p = support::part(n, name, symbol, footprints);
    p.fields.description = format!("{name} part");
    p
}

const ROOT: &str = "/libs/mine";

fn scanned(files: &mut MemFiles) -> LibraryIndex {
    files.library(&format!("{ROOT}/mine.redalib"), library(1, "Mine"));
    scan(&[LibraryRoot::new(ROOT, Origin::User)], files)
}

/// The check of one element of the index.
fn check_of(index: &LibraryIndex, n: u128) -> Vec<Violation> {
    check_with(index, n, &LibrarySeverities::DEFAULT)
}

/// The check of one element of the index with the given severities.
fn check_with(index: &LibraryIndex, n: u128, severities: &LibrarySeverities) -> Vec<Violation> {
    let element = index.element(uuid(n)).expect("the element was scanned");
    check(index, &element, severities)
}

fn checks(violations: &[Violation]) -> Vec<&str> {
    violations.iter().map(|v| v.check.as_str()).collect()
}

fn text(s: &str) -> ViolationArg {
    ViolationArg::Text(s.into())
}

#[test]
fn a_clean_part_has_no_violations() {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    files.footprint(
        &format!("{ROOT}/footprints/R0603.redafp"),
        footprint(20, "R0603"),
    );
    files.part(
        &format!("{ROOT}/parts/R.redapart"),
        part(30, "R", 10, &[20]),
    );
    let index = scanned(&mut files);
    for n in [10, 20, 30] {
        assert_eq!(check_of(&index, n), vec![], "element {n}");
    }
}

#[test]
fn a_pin_off_the_50_mil_grid_is_reported() {
    let mut files = MemFiles::default();
    let mut s = symbol(10, "R");
    s.pins[1].at = Point::new(Nm::from_mil(25), Nm::from_mil(200));
    let off = s.pins[1].uuid;
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), s);
    let index = scanned(&mut files);

    let found = check_of(&index, 10);
    assert_eq!(checks(&found), ["off_grid_pin"]);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].objects, [uuid(10), off]);
    assert_eq!(found[0].args["pin"], text("2"));
    assert_eq!(found[0].args["grid"], ViolationArg::Int(1_270_000));
}

#[test]
fn pins_sharing_a_number_are_one_violation() {
    let mut files = MemFiles::default();
    let mut s = symbol(10, "R");
    s.pins[1].number = "1".into();
    let pins = [s.pins[0].uuid, s.pins[1].uuid];
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), s);
    let index = scanned(&mut files);

    let found = check_of(&index, 10);
    assert_eq!(checks(&found), ["duplicate_pin_number"]);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].objects, [uuid(10), pins[0], pins[1]]);
    assert_eq!(found[0].args["number"], text("1"));
}

#[test]
fn a_footprint_without_courtyard_is_reported() {
    let mut files = MemFiles::default();
    let mut f = footprint(20, "R0603");
    f.graphics
        .retain(|g| g.layer != FootprintLayer::CourtyardTop);
    f.graphics.push(silkscreen_line(20));
    files.footprint(&format!("{ROOT}/footprints/R0603.redafp"), f);
    let index = scanned(&mut files);

    let found = check_of(&index, 20);
    assert_eq!(checks(&found), ["missing_courtyard"]);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].objects, [uuid(20)]);
}

#[test]
fn a_plated_pad_whose_offset_leaves_its_hole_outside_the_copper_is_reported() {
    let mut files = MemFiles::default();
    let mut f = footprint(20, "R0603");
    for pad in &mut f.pads {
        pad.kind = PadKind::PlatedThroughHole;
        pad.shape = PadShape::Rect;
        pad.corner_ratio = None;
        pad.hole = Some(Hole::new(Nm::from_um(400), Nm::from_um(400)));
    }
    // Pad 1's copper sits 0.6 mm to the right of its 0.4 mm hole: the hole pokes out.
    f.pads[0].offset = Some(Point::new(Nm::from_um(600), Nm(0)));
    // Pad 2's copper moves 0.1 mm, which keeps the hole inside.
    f.pads[1].offset = Some(Point::new(Nm::from_um(100), Nm(0)));
    files.footprint(&format!("{ROOT}/footprints/R0603.redafp"), f);
    let index = scanned(&mut files);

    let found = check_of(&index, 20);
    assert_eq!(checks(&found), ["hole_outside_copper"]);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].objects, [uuid(20), uuid(20_001)]);
    assert_eq!(found[0].args["pad"], text("1"));
}

#[test]
fn a_bottom_courtyard_counts() {
    let mut files = MemFiles::default();
    let mut f = footprint(20, "R0603");
    f.graphics[0].layer = FootprintLayer::CourtyardBottom;
    files.footprint(&format!("{ROOT}/footprints/R0603.redafp"), f);
    let index = scanned(&mut files);
    assert_eq!(check_of(&index, 20), vec![]);
}

fn silkscreen_line(n: u128) -> FootprintGraphic {
    FootprintGraphic {
        uuid: uuid(n * 1000 + 500),
        layer: FootprintLayer::SilkscreenTop,
        shape: Shape::Line {
            start: Point::new(Nm(0), Nm(0)),
            end: Point::new(Nm::from_mm(1), Nm(0)),
        },
        width: Nm::from_um(120),
        fill: false,
    }
}

#[test]
fn a_part_naming_a_missing_symbol_or_footprint_is_reported() {
    let mut files = MemFiles::default();
    files.footprint(
        &format!("{ROOT}/footprints/R0603.redafp"),
        footprint(20, "R0603"),
    );
    files.part(
        &format!("{ROOT}/parts/R.redapart"),
        part(30, "R", 10, &[20, 21]),
    );
    let index = scanned(&mut files);

    let found = check_of(&index, 30);
    assert_eq!(checks(&found), ["broken_reference", "broken_reference"]);
    assert!(found.iter().all(|v| v.severity == Severity::Error));
    assert!(found.iter().all(|v| v.objects == [uuid(30)]));
    assert_eq!(found[0].args["kind"], text("symbol"));
    assert_eq!(found[0].args["reference"], text(&uuid(10).to_string()));
    assert_eq!(found[1].args["kind"], text("footprint"));
    assert_eq!(found[1].args["reference"], text(&uuid(21).to_string()));
}

#[test]
fn a_part_referencing_an_element_of_another_kind_is_broken() {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    files.part(
        &format!("{ROOT}/parts/R.redapart"),
        part(30, "R", 10, &[10]),
    );
    let index = scanned(&mut files);

    let found = check_of(&index, 30);
    assert_eq!(checks(&found), ["broken_reference"]);
    assert_eq!(found[0].args["kind"], text("footprint"));
}

#[test]
fn a_pin_map_override_naming_no_pin_of_the_symbol_is_broken() {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    files.footprint(
        &format!("{ROOT}/footprints/R0603.redafp"),
        footprint(20, "R0603"),
    );
    let mut p = part(30, "R", 10, &[20]);
    p.pin_map.pins.push(PinPads {
        pin: "9".into(),
        pads: vec!["1".into()],
    });
    files.part(&format!("{ROOT}/parts/R.redapart"), p);
    let index = scanned(&mut files);

    let found = check_of(&index, 30);
    assert_eq!(checks(&found), ["broken_reference"]);
    assert_eq!(found[0].args["kind"], text("pin"));
    assert_eq!(found[0].args["reference"], text("9"));
    assert_eq!(found[0].objects, [uuid(30), uuid(10)]);
}

#[test]
fn a_pin_with_no_pad_in_one_alternate_is_reported() {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    files.footprint(
        &format!("{ROOT}/footprints/R0603.redafp"),
        footprint(20, "R0603"),
    );
    let mut small = footprint(21, "Pad1Only");
    small.pads.truncate(1);
    files.footprint(&format!("{ROOT}/footprints/Pad1Only.redafp"), small);
    files.part(
        &format!("{ROOT}/parts/R.redapart"),
        part(30, "R", 10, &[20, 21]),
    );
    let index = scanned(&mut files);

    let found = check_of(&index, 30);
    assert_eq!(checks(&found), ["pin_without_pad"]);
    assert_eq!(found[0].severity, Severity::Error);
    let pin2 = index.symbol(uuid(10)).unwrap().pins[1].uuid;
    assert_eq!(found[0].objects, [uuid(30), pin2, uuid(21)]);
    assert_eq!(found[0].args["pin"], text("2"));
    assert_eq!(found[0].args["footprint"], text("Pad1Only"));
}

#[test]
fn a_pin_mapped_to_another_pad_needs_that_pad() {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    let mut f = footprint(20, "R0603");
    f.pads[1].number = "7".into();
    files.footprint(&format!("{ROOT}/footprints/R0603.redafp"), f);
    let mut p = part(30, "R", 10, &[20]);
    p.pin_map.pins.push(PinPads {
        pin: "2".into(),
        pads: vec!["7".into()],
    });
    files.part(&format!("{ROOT}/parts/R.redapart"), p.clone());
    let index = scanned(&mut files);
    assert_eq!(check_of(&index, 30), vec![], "pin 2 reaches pad 7");

    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    files.footprint(
        &format!("{ROOT}/footprints/R0603.redafp"),
        footprint(20, "R0603"),
    );
    files.part(&format!("{ROOT}/parts/R.redapart"), p);
    let index = scanned(&mut files);
    let found = check_of(&index, 30);
    // Pin 2 is overridden to the absent pad 7, so pad 2 is mapped to nothing either.
    assert_eq!(checks(&found), ["pin_without_pad", "unmapped_pad"]);
    assert_eq!(found[0].args["pin"], text("2"));
}

/// A Footprint with Pads 1, 2 and two Pads numbered 3.
fn footprint_with_extra_pads(n: u128) -> Footprint {
    let mut f = footprint(n, "SOT-23-ish");
    for i in [3, 4] {
        let mut pad = f.pads[0].clone();
        pad.uuid = uuid(n * 1000 + i);
        pad.number = "3".into();
        pad.at.y = Nm::from_mm(2) * (i as i64 - 2);
        f.pads.push(pad);
    }
    f
}

#[test]
fn pads_reached_by_no_pin_are_one_violation_per_number() {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    files.footprint(
        &format!("{ROOT}/footprints/F.redafp"),
        footprint_with_extra_pads(20),
    );
    files.part(
        &format!("{ROOT}/parts/R.redapart"),
        part(30, "R", 10, &[20]),
    );
    let index = scanned(&mut files);

    let found = check_of(&index, 30);
    assert_eq!(checks(&found), ["unmapped_pad"]);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(
        found[0].objects,
        [uuid(30), uuid(20), uuid(20_003), uuid(20_004)]
    );
    assert_eq!(found[0].args["pad"], text("3"));
    assert_eq!(found[0].args["footprint"], text("SOT-23-ish"));
}

#[test]
fn pads_marked_unconnected_are_not_reported() {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    files.footprint(
        &format!("{ROOT}/footprints/F.redafp"),
        footprint_with_extra_pads(20),
    );
    let mut p = part(30, "R", 10, &[20]);
    p.pin_map.unconnected.push("3".into());
    files.part(&format!("{ROOT}/parts/R.redapart"), p);
    let index = scanned(&mut files);
    assert_eq!(check_of(&index, 30), vec![]);
}

#[test]
fn non_plated_pads_need_no_pin() {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    let mut f = footprint_with_extra_pads(20);
    for pad in f.pads.iter_mut().filter(|p| p.number == "3") {
        pad.kind = PadKind::NonPlated;
    }
    files.footprint(&format!("{ROOT}/footprints/F.redafp"), f);
    files.part(
        &format!("{ROOT}/parts/R.redapart"),
        part(30, "R", 10, &[20]),
    );
    let index = scanned(&mut files);
    assert_eq!(check_of(&index, 30), vec![]);
}

#[test]
fn a_part_without_a_symbol_skips_the_pin_checks() {
    let mut files = MemFiles::default();
    files.footprint(
        &format!("{ROOT}/footprints/F.redafp"),
        footprint(20, "Hole"),
    );
    let mut p = part(30, "MountingHole", 0, &[20]);
    p.symbol = None;
    p.component_type = ComponentType::Mechanical;
    files.part(&format!("{ROOT}/parts/Hole.redapart"), p);
    let index = scanned(&mut files);
    assert_eq!(check_of(&index, 30), vec![]);
}

/// A Footprint whose two Pads touch: Pad 2 moved onto Pad 1's edge.
fn touching_pads(n: u128) -> Footprint {
    let mut f = footprint(n, "Tie");
    f.pads[1].at = Point::new(f.pads[0].at.x + f.pads[0].width, Nm(0));
    f
}

/// A Library with Symbol 10, Footprints 20 (clean) and 21, and Part 30 of type `ty`
/// with both Footprints as alternates.
fn tie_library(ty: ComponentType, second: Footprint) -> LibraryIndex {
    let mut files = MemFiles::default();
    files.symbol(&format!("{ROOT}/symbols/NT.redasym"), symbol(10, "NT"));
    files.footprint(&format!("{ROOT}/footprints/A.redafp"), footprint(20, "A"));
    files.footprint(&format!("{ROOT}/footprints/B.redafp"), second);
    let mut p = part(30, "NT", 10, &[20, 21]);
    p.component_type = ty;
    files.part(&format!("{ROOT}/parts/NT.redapart"), p);
    scanned(&mut files)
}

#[test]
fn touching_pads_with_different_numbers_short_a_part_that_is_not_a_net_tie() {
    let tie = touching_pads(21);
    let pads = [tie.pads[0].uuid, tie.pads[1].uuid];
    let index = tie_library(ComponentType::Standard, tie);

    let found = check_of(&index, 30);
    assert_eq!(checks(&found), ["shorted_copper"]);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].objects, [uuid(30), uuid(21), pads[0], pads[1]]);
    assert_eq!(found[0].args["pad_a"], text("1"));
    assert_eq!(found[0].args["pad_b"], text("2"));
    assert_eq!(found[0].args["footprint"], text("Tie"));
    assert_eq!(
        check_of(&index, 21),
        vec![],
        "a Footprint alone has no type"
    );
}

#[test]
fn touching_pads_sharing_a_number_or_on_different_sides_do_not_short() {
    let mut same = touching_pads(21);
    same.pads[1].number = "1".into();
    let index = tie_library(ComponentType::Mechanical, same);
    assert!(!checks(&check_of(&index, 30)).contains(&"shorted_copper"));

    let mut sides = touching_pads(21);
    sides.pads[1].side = Some(reda_model::Side::Bottom);
    let index = tie_library(ComponentType::Standard, sides);
    assert_eq!(check_of(&index, 30), vec![]);
}

#[test]
fn a_net_tie_whose_pads_touch_is_valid_and_not_shorted() {
    let index = tie_library(ComponentType::NetTie, touching_pads(21));
    let found = check_of(&index, 30);
    // Footprint 20's Pads don't touch: that alternate is no tie.
    assert_eq!(checks(&found), ["net_tie_not_joined"]);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].objects, [uuid(30), uuid(20)]);
    assert_eq!(found[0].args["reason"], text("not_joined"));
    assert_eq!(found[0].args["footprint"], text("A"));
}

#[test]
fn a_net_tie_needs_smd_pads_on_one_side_with_two_numbers() {
    let reason = |f: Footprint| {
        let index = tie_library(ComponentType::NetTie, f);
        let found: Vec<_> = check_of(&index, 30)
            .into_iter()
            .filter(|v| v.objects.contains(&uuid(21)) && v.check != "pin_without_pad")
            .collect();
        assert_eq!(checks(&found), ["net_tie_not_joined"]);
        found[0].args["reason"].clone()
    };
    let mut sides = touching_pads(21);
    sides.pads[1].side = Some(reda_model::Side::Bottom);
    assert_eq!(reason(sides), text("not_smd_one_side"));

    let mut through = touching_pads(21);
    through.pads[1].kind = PadKind::PlatedThroughHole;
    through.pads[1].hole = Some(Hole {
        width: Nm::from_um(300),
        height: Nm::from_um(300),
        rotation: reda_model::geom::Millideg(0),
        tolerance: None,
    });
    assert_eq!(reason(through), text("not_smd_one_side"));

    let mut one_number = touching_pads(21);
    one_number.pads[1].number = "1".into();
    assert_eq!(reason(one_number), text("too_few_numbers"));
}

/// One Library holding `symbols`, `footprints` and `parts`.
fn library_of(symbols: Vec<Symbol>, footprints: Vec<Footprint>, parts: Vec<Part>) -> LibraryIndex {
    let mut files = MemFiles::default();
    for s in symbols {
        files.symbol(&format!("{ROOT}/symbols/{}.redasym", s.name), s);
    }
    for f in footprints {
        files.footprint(&format!("{ROOT}/footprints/{}.redafp", f.name), f);
    }
    for p in parts {
        files.part(&format!("{ROOT}/parts/{}.redapart", p.name), p);
    }
    scanned(&mut files)
}

#[test]
fn a_pad_with_no_number_is_reported_per_part() {
    let mut f = footprint(20, "F");
    f.pads[1].number = String::new();
    let pad = f.pads[1].uuid;
    let mut p = part(30, "R", 10, &[20]);
    p.pin_map.pins.push(PinPads {
        pin: "2".into(),
        pads: vec!["1".into()],
    });
    let index = library_of(vec![symbol(10, "R")], vec![f], vec![p]);

    assert_eq!(
        check_of(&index, 20),
        vec![],
        "a Footprint alone is not checked"
    );
    let found = check_of(&index, 30);
    assert_eq!(checks(&found), ["missing_pad_number"]);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].objects, [uuid(30), uuid(20), pad]);
    assert_eq!(found[0].args["footprint"], text("F"));
}

#[test]
fn non_plated_pads_and_mechanical_parts_need_no_pad_number() {
    let mut npth = footprint(20, "F");
    npth.pads[1].number = String::new();
    npth.pads[1].kind = PadKind::NonPlated;
    npth.pads[1].hole = Some(Hole::new(Nm::from_um(400), Nm::from_um(400)));
    let mut p = part(30, "R", 10, &[20]);
    p.pin_map.pins.push(PinPads {
        pin: "2".into(),
        pads: vec!["1".into()],
    });
    let index = library_of(vec![symbol(10, "R")], vec![npth], vec![p]);
    assert_eq!(check_of(&index, 30), vec![]);

    let mut fiducial = footprint(20, "Fiducial");
    for pad in &mut fiducial.pads {
        pad.number = String::new();
    }
    let mut p = part(30, "Fiducial", 0, &[20]);
    p.symbol = None;
    p.component_type = ComponentType::Mechanical;
    let index = library_of(vec![], vec![fiducial], vec![p]);
    assert_eq!(check_of(&index, 30), vec![]);
}

#[test]
fn a_footprint_with_smd_copper_only_on_the_bottom_is_mirrored() {
    let mut f = footprint(20, "F");
    for pad in &mut f.pads {
        pad.side = Some(Side::Bottom);
    }
    let index = library_of(vec![], vec![f.clone()], vec![]);
    let found = check_of(&index, 20);
    assert_eq!(checks(&found), ["mirrored_footprint"]);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].objects, [uuid(20)]);

    // One Pad on top, or one through-hole Pad, and it is not.
    let mut mixed = f.clone();
    mixed.pads[0].side = None;
    let index = library_of(vec![], vec![mixed], vec![]);
    assert_eq!(check_of(&index, 20), vec![]);
    let mut through = f;
    through.pads[0].side = None;
    through.pads[0].kind = PadKind::PlatedThroughHole;
    through.pads[0].shape = PadShape::Circle;
    through.pads[0].width = through.pads[0].height;
    through.pads[0].corner_ratio = None;
    through.pads[0].hole = Some(Hole::new(Nm::from_um(400), Nm::from_um(400)));
    let index = library_of(vec![], vec![through], vec![]);
    assert_eq!(check_of(&index, 20), vec![]);
}

/// The Footprint with everything moved by `dx` µm along x.
fn shifted(mut f: Footprint, dx: i64) -> Footprint {
    let by = Point::new(Nm::from_um(dx), Nm(0));
    for pad in &mut f.pads {
        pad.at = pad.at + by;
    }
    for g in &mut f.graphics {
        if let Shape::Rect { start, end } | Shape::Line { start, end } = &mut g.shape {
            *start = *start + by;
            *end = *end + by;
        }
    }
    f
}

#[test]
fn an_origin_outside_the_courtyard_box_is_outside_the_body() {
    let index = library_of(vec![], vec![shifted(footprint(20, "F"), 2000)], vec![]);
    let found = check_of(&index, 20);
    // Outside the body says it all: it is not also reported as off the centre.
    assert_eq!(checks(&found), ["origin_outside_body"]);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].objects, [uuid(20)]);
}

#[test]
fn an_origin_more_than_a_tenth_of_a_millimetre_off_the_body_centre_is_reported() {
    let index = library_of(vec![], vec![shifted(footprint(20, "F"), 100)], vec![]);
    assert_eq!(check_of(&index, 20), vec![], "0.1 mm is close enough");

    let index = library_of(vec![], vec![shifted(footprint(20, "F"), 300)], vec![]);
    let found = check_of(&index, 20);
    assert_eq!(checks(&found), ["origin_off_centroid"]);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].args["x"], ViolationArg::Int(300_000));
    assert_eq!(found[0].args["y"], ViolationArg::Int(0));
}

#[test]
fn with_no_courtyard_the_body_box_is_around_copper_and_drawing() {
    let mut f = footprint(20, "F");
    f.graphics.clear();
    // Silkscreen reaching to x = 3 mm puts the fallback box's centre at x = 1.05 mm.
    f.graphics.push(FootprintGraphic {
        uuid: uuid(20_500),
        layer: FootprintLayer::SilkscreenTop,
        shape: Shape::Line {
            start: Point::new(Nm(0), Nm(0)),
            end: Point::new(Nm::from_mm(3), Nm(0)),
        },
        width: Nm::from_um(120),
        fill: false,
    });
    let index = library_of(vec![], vec![f], vec![]);
    let found = check_of(&index, 20);
    assert_eq!(checks(&found), ["missing_courtyard", "origin_off_centroid"]);
    assert_eq!(found[1].args["x"], ViolationArg::Int(1_050_000));
}

#[test]
fn a_symbol_without_designator_prefix_or_pin_numbers_is_reported() {
    let mut s = symbol(10, "R");
    s.designator_prefix = " ".into();
    s.pins[0].number = String::new();
    s.pins[1].number = String::new();
    let pins = [s.pins[0].uuid, s.pins[1].uuid];
    let index = library_of(vec![s], vec![], vec![]);
    let found = check_of(&index, 10);
    // Two Pins without a number don't share one.
    assert_eq!(
        checks(&found),
        [
            "missing_designator_prefix",
            "missing_pin_number",
            "missing_pin_number"
        ]
    );
    assert!(found.iter().all(|v| v.severity == Severity::Error));
    assert_eq!(found[0].objects, [uuid(10)]);
    assert_eq!(found[1].objects, [uuid(10), pins[0]]);
    assert_eq!(found[2].objects, [uuid(10), pins[1]]);
}

#[test]
fn the_same_graphic_twice_on_one_layer_is_a_duplicate_primitive() {
    let mut f = footprint(20, "F");
    let mut twin = f.graphics[0].clone();
    twin.uuid = uuid(20_600);
    f.graphics.push(twin.clone());
    let mut other_layer = twin.clone();
    other_layer.uuid = uuid(20_601);
    other_layer.layer = FootprintLayer::CourtyardBottom;
    f.graphics.push(other_layer);
    let index = library_of(vec![], vec![f], vec![]);
    let found = check_of(&index, 20);
    assert_eq!(checks(&found), ["duplicate_primitives"]);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].objects, [uuid(20), uuid(20_600), uuid(20_999)]);

    let mut s = symbol(10, "R");
    let unit = s.units[0].uuid;
    let line = |n: u128| reda_model::SymbolGraphic {
        uuid: uuid(n),
        unit,
        shape: Shape::Line {
            start: Point::new(Nm(0), Nm(0)),
            end: Point::new(Nm::from_mil(100), Nm(0)),
        },
        width: Nm::from_um(150),
        fill: false,
    };
    s.graphics = vec![line(10_700), line(10_701)];
    let index = library_of(vec![s], vec![], vec![]);
    let found = check_of(&index, 10);
    assert_eq!(checks(&found), ["duplicate_primitives"]);
    assert_eq!(found[0].objects, [uuid(10), uuid(10_700), uuid(10_701)]);
}

#[test]
fn every_kind_of_element_needs_a_description() {
    let mut s = symbol(10, "R");
    s.description.clear();
    let mut f = footprint(20, "F");
    f.description.clear();
    let mut p = part(30, "R", 10, &[20]);
    p.fields.description.clear();
    let index = library_of(vec![s], vec![f], vec![p]);
    for n in [10, 20, 30] {
        let found = check_of(&index, n);
        assert_eq!(checks(&found), ["missing_description"], "element {n}");
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].objects, [uuid(n)]);
    }
}

/// The Symbol with Pins numbered `numbers`.
fn numbered(numbers: &[&str]) -> Symbol {
    let mut s = symbol(10, "U");
    let pin = s.pins[0].clone();
    s.pins = numbers
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let mut p = pin.clone();
            p.uuid = uuid(10_100 + i as u128);
            p.number = (*n).into();
            p.at.y = Nm::from_mil(100) * i as i64;
            p
        })
        .collect();
    s
}

#[test]
fn a_gap_in_numeric_pin_numbers_is_reported() {
    let index = library_of(vec![numbered(&["1", "2", "5", "EP", "7"])], vec![], vec![]);
    let found = check_of(&index, 10);
    assert_eq!(checks(&found), ["missing_pins_in_sequence"]);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].args["missing"], text("3-4, 6"));
    assert_eq!(found[0].objects, [uuid(10)]);
}

#[test]
fn absent_pins_of_a_parts_generated_footprint_are_no_gap() {
    let mut f = footprint(20, "SOIC");
    f.generator = Some(Box::new(reda_model::FootprintGenerator {
        command: "library.footprint.generate.soic".into(),
        params: serde_json::json!({"absent_pins": [3, 4]}),
    }));
    let p = part(30, "U", 10, &[20]);
    let index = library_of(vec![numbered(&["1", "2", "5", "6"])], vec![f], vec![p]);
    let found = check_of(&index, 10);
    assert!(
        !checks(&found).contains(&"missing_pins_in_sequence"),
        "{found:?}"
    );
}

#[test]
fn parts_sharing_a_name_or_an_mpn_across_libraries_are_reported() {
    let mut files = MemFiles::default();
    files.library(&format!("{ROOT}/mine.redalib"), library(1, "Mine"));
    files.library("/libs/other/other.redalib", library(2, "Other"));
    files.symbol(&format!("{ROOT}/symbols/R.redasym"), symbol(10, "R"));
    files.footprint(&format!("{ROOT}/footprints/F.redafp"), footprint(20, "F"));
    let mut a = part(30, "R", 10, &[20]);
    a.fields.mpn = "RC0603".into();
    let mut b = part(31, "R", 10, &[20]);
    b.fields.mpn = "RC0603".into();
    let mut c = part(32, "C", 10, &[20]);
    c.fields.mpn = "RC0603".into();
    files.part(&format!("{ROOT}/parts/R.redapart"), a);
    files.part("/libs/other/parts/R.redapart", b);
    files.part("/libs/other/parts/C.redapart", c);
    let roots = [
        LibraryRoot::new(ROOT, Origin::User),
        LibraryRoot::new("/libs/other", Origin::User),
    ];
    let index = scan(&roots, &mut files);

    let found = check_of(&index, 30);
    assert_eq!(checks(&found), ["duplicate_part_name", "duplicate_mpn"]);
    assert!(found.iter().all(|v| v.severity == Severity::Warning));
    assert_eq!(found[0].args["name"], text("R"));
    assert_eq!(found[0].objects, [uuid(30), uuid(31)]);
    assert_eq!(found[1].args["mpn"], text("RC0603"));
    assert_eq!(found[1].objects, [uuid(30), uuid(31), uuid(32)]);
    assert_eq!(checks(&check_of(&index, 32)), ["duplicate_mpn"]);
}

#[test]
fn parts_without_an_mpn_do_not_share_one() {
    let a = part(30, "R", 10, &[20]);
    let b = part(31, "C", 10, &[20]);
    let index = library_of(vec![symbol(10, "R")], vec![footprint(20, "F")], vec![a, b]);
    assert_eq!(check_of(&index, 30), vec![]);
}

/// A chip Footprint made by its generator, as the Library stores it.
fn generated(n: u128) -> Footprint {
    let params = serde_json::json!({"size": "0603", "library": uuid(1)});
    let mut next = n * 1000;
    let (mut f, _) =
        reda_library::generate::rebuild("library.footprint.generate.chip", &params, &mut || {
            next += 1;
            uuid(next)
        })
        .expect("chip is a generator")
        .expect("the params work");
    f.uuid = uuid(n);
    f
}

#[test]
fn a_generated_footprint_its_params_still_make_is_up_to_date() {
    let f = generated(20);
    let index = library_of(vec![], vec![f], vec![]);
    assert_eq!(check_of(&index, 20), vec![]);
}

#[test]
fn a_hand_edited_generated_footprint_is_out_of_date() {
    let mut f = generated(20);
    f.pads[0].width = f.pads[0].width + Nm::from_um(100);
    let index = library_of(vec![], vec![f.clone()], vec![]);
    let found = check_of(&index, 20);
    assert_eq!(checks(&found), ["generator_out_of_date"]);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].objects, [uuid(20)]);
    assert_eq!(
        found[0].args["generator"],
        text("library.footprint.generate.chip")
    );

    // Stored params that now make something else (as from a changed generator) read the
    // same: nothing tells the two causes apart.
    let mut changed = generated(20);
    let generator = changed.generator.as_mut().unwrap();
    generator.params["pads"]["width"] = serde_json::json!(1_000_000);
    let index = library_of(vec![], vec![changed], vec![]);
    assert_eq!(checks(&check_of(&index, 20)), ["generator_out_of_date"]);

    // A renamed one is not: the name is the Footprint's own.
    let mut renamed = generated(20);
    renamed.name = "MyChip".into();
    let index = library_of(vec![], vec![renamed], vec![]);
    assert_eq!(check_of(&index, 20), vec![]);
}

#[test]
fn stored_params_no_generator_takes_are_out_of_date() {
    let mut f = generated(20);
    f.generator.as_mut().unwrap().command = "library.footprint.generate.gone".into();
    let index = library_of(vec![], vec![f], vec![]);
    assert_eq!(checks(&check_of(&index, 20)), ["generator_out_of_date"]);
}

#[test]
fn a_pin_with_no_name_is_off_by_default() {
    let mut s = symbol(10, "R");
    s.pins[1].name = String::new();
    let pin = s.pins[1].uuid;
    let index = library_of(vec![s], vec![], vec![]);
    assert_eq!(check_of(&index, 10), vec![]);

    let mut severities = LibrarySeverities::DEFAULT;
    severities.set(LibraryCheck::MissingPinName, Severity::Warning);
    let found = check_with(&index, 10, &severities);
    assert_eq!(checks(&found), ["missing_pin_name"]);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].objects, [uuid(10), pin]);
}

#[test]
fn severities_decide_how_each_check_reports_and_off_hides_it() {
    let mut s = symbol(10, "R");
    s.designator_prefix.clear();
    s.description.clear();
    let index = library_of(vec![s], vec![], vec![]);

    let mut severities = LibrarySeverities::DEFAULT;
    severities.set(LibraryCheck::MissingDesignatorPrefix, Severity::Warning);
    severities.set(LibraryCheck::MissingDescription, Severity::Off);
    let found = check_with(&index, 10, &severities);
    assert_eq!(checks(&found), ["missing_designator_prefix"]);
    assert_eq!(found[0].severity, Severity::Warning);
}

#[test]
fn the_spec_defaults() {
    use LibraryCheck as C;
    use Severity::{Error as E, Off, Warning as W};
    let expected = [
        (C::DuplicatePinNumber, E),
        (C::PinWithoutPad, E),
        (C::BrokenReference, E),
        (C::ShortedCopper, E),
        (C::NetTieNotJoined, E),
        (C::MissingPadNumber, E),
        (C::MirroredFootprint, E),
        (C::OriginOutsideBody, E),
        (C::MissingDesignatorPrefix, E),
        (C::MissingPinNumber, E),
        (C::HoleOutsideCopper, E),
        (C::OffGridPin, W),
        (C::UnmappedPad, W),
        (C::MissingCourtyard, W),
        (C::PasteWindowOutsideCopper, W),
        (C::DuplicatePrimitives, W),
        (C::OriginOffCentroid, W),
        (C::MissingDescription, W),
        (C::MissingPinsInSequence, W),
        (C::DuplicatePartName, W),
        (C::DuplicateMpn, W),
        (C::GeneratorOutOfDate, W),
        (C::MissingPinName, Off),
    ];
    assert_eq!(LibraryCheck::ALL.len(), expected.len());
    for (check, severity) in expected {
        assert_eq!(LibrarySeverities::DEFAULT.get(check), severity, "{check:?}");
    }
}
