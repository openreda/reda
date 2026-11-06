//! The Library check: each finding, reported as Violations, over a scanned Library.

mod support;

use reda_library::{LibraryIndex, LibraryRoot, Origin, check, scan};
use reda_model::geom::{Nm, Point};
use reda_model::{
    ComponentType, Footprint, FootprintGraphic, FootprintLayer, Hole, PadKind, PadShape, PinPads,
    Severity, Shape, Violation, ViolationArg,
};
use support::*;

const ROOT: &str = "/libs/mine";

fn scanned(files: &mut MemFiles) -> LibraryIndex {
    files.library(&format!("{ROOT}/mine.redalib"), library(1, "Mine"));
    scan(&[LibraryRoot::new(ROOT, Origin::User)], files)
}

/// The check of one element of the index.
fn check_of(index: &LibraryIndex, n: u128) -> Vec<Violation> {
    let element = index.element(uuid(n)).expect("the element was scanned");
    check(index, &element)
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
