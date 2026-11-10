//! Copper-only Parts: Mechanical mounting holes and fiducials, placed straight on the PCB
//! with no Symbol, the Net Tie and the test point.

use serde_json::json;

use crate::passives::fields;
use crate::script::{Kind, PartSpec, Script, bottom, circle, left, mil, mm, rect, right};

pub fn build(s: &mut Script) {
    // Screw size, clearance hole, screw head diameter: the hole is ISO 273's fine
    // clearance hole (M2 2.2, M2.5 2.7, M3 3.2 mm), the head ISO 4762's largest socket
    // head (M2 3.8, M2.5 4.5, M3 5.5 mm).
    for (screw, hole, head) in [("M2", 2.2, 3.8), ("M2.5", 2.7, 4.5), ("M3", 3.2, 5.5)] {
        mounting_hole(s, screw, hole, head, false);
    }
    mounting_hole(s, "M3", 3.2, 5.5, true);
    fiducial(s);
    net_tie(s);
    test_point(s);
}

/// The height of a copper-only Footprint: 1 oz copper, 0.035 mm.
const COPPER: f64 = 0.035;

/// The head height of an ISO 4762 socket head cap screw: the screw's diameter.
fn head_height(screw: &str) -> f64 {
    screw[1..].parse().unwrap()
}

/// A clearance hole for `screw`: non-plated, or `plated` with a copper ring the size of
/// the screw head (Pad 1, to tie to a Net such as GND).
fn mounting_hole(s: &mut Script, screw: &str, hole: f64, head: f64, plated: bool) {
    let suffix = if plated { "_Pad" } else { "" };
    let name = format!("MountingHole_{screw}{suffix}");
    let description = if plated {
        format!("Mounting hole for {screw}, {hole} mm plated, {head} mm copper ring")
    } else {
        format!("Mounting hole for {screw}, {hole} mm, not plated")
    };
    let fp = s.footprint(
        &name,
        &format!("{description}. Hole per ISO 273 fine, head per ISO 4762"),
        head_height(screw),
    );
    let size = if plated { head } else { hole };
    s.pad(
        &fp,
        json!({
            "number": if plated { "1" } else { "" },
            "kind": if plated { "plated_through_hole" } else { "non_plated" },
            "shape": "circle",
            "at": {"x": 0, "y": 0},
            "width": mm(size),
            "height": mm(size),
            "hole": {"width": mm(hole)},
        }),
    );
    let r = head / 2.0;
    s.fp_draw(&fp, "assembly_top", circle(0, 0, mm(r)), mm(0.1));
    s.fp_draw(&fp, "silkscreen_top", circle(0, 0, mm(r + 0.32)), mm(0.12));
    s.fp_draw(&fp, "courtyard_top", circle(0, 0, mm(r + 0.5)), mm(0.05));
    let fp = s.finish_footprint(&fp, mm(r + 0.5));
    s.part(PartSpec {
        name: &name,
        symbol: None,
        footprints: vec![fp],
        fields: fields(screw, &description, "H mounting hole screw mechanical"),
        pin_map: None,
        kind: Kind::Mechanical,
    });
}

/// A 1 mm copper dot in a 2 mm solder-mask opening, for pick-and-place cameras.
fn fiducial(s: &mut Script) {
    let description = "Fiducial mark, 1 mm copper in a 2 mm mask opening";
    let fp = s.footprint("Fiducial_1mm_Mask2mm", description, COPPER);
    s.pad(
        &fp,
        json!({
            "number": "",
            "kind": "smd",
            "shape": "circle",
            "at": {"x": 0, "y": 0},
            "width": mm(1.0),
            "height": mm(1.0),
            "mask": {"top": {"expansion": mm(0.5)}},
            "paste": "none",
        }),
    );
    // The Assembly outline is the copper dot, the only thing there is to place.
    s.fp_draw(&fp, "assembly_top", circle(0, 0, mm(0.5)), mm(0.1));
    s.fp_draw(&fp, "courtyard_top", circle(0, 0, mm(1.25)), mm(0.05));
    let fp = s.finish_footprint(&fp, mm(1.25));
    s.part(PartSpec {
        name: "Fiducial",
        symbol: None,
        footprints: vec![fp],
        fields: fields("Fiducial", description, "FID fiducial mechanical"),
        pin_map: None,
        kind: Kind::Mechanical,
    });
}

/// A Net Tie between two Nets: two square SMD Pads `size` mm wide that touch edge to edge,
/// so they form one copper group; the copper is the neck, as wide as a Pad. Tented, with
/// no paste: nothing is soldered to it. Sizes follow the common 0.5 mm (signal) and
/// 2.0 mm (power) ties.
fn net_tie(s: &mut Script) {
    let mut sym = s.symbol("NetTie_2", "NT", "Net Tie between two Nets");
    sym.names = false;
    sym.numbers = false;
    s.pin(&sym, left("~", "1", "passive", -150, 0));
    s.pin(&sym, right("~", "2", "passive", 150, 0));
    s.draw(&sym, rect(mil(-50), mil(-25), mil(50), mil(25)), true);
    let sym = s.finish_symbol(sym);
    let footprints = [0.5, 2.0]
        .into_iter()
        .map(|size| tie_footprint(s, size))
        .collect();
    s.part(PartSpec {
        name: "NetTie",
        symbol: Some(&sym),
        footprints,
        fields: fields(
            "NetTie",
            "Net Tie: a deliberate short between two Nets",
            "NT net tie short star ground",
        ),
        pin_map: None,
        kind: Kind::NetTie,
    });
}

fn tie_footprint(s: &mut Script, size: f64) -> String {
    let name = format!("NetTie_2_SMD_Pad{size:.1}mm");
    let description = format!("Net Tie, two touching {size} mm SMD Pads, tented, no paste");
    let fp = s.footprint(&name, &description, COPPER);
    for (number, x) in [("1", -size / 2.0), ("2", size / 2.0)] {
        s.pad(
            &fp,
            json!({
                "number": number,
                "kind": "smd",
                "shape": "rect",
                "at": {"x": mm(x), "y": 0},
                "width": mm(size),
                "height": mm(size),
                "mask": {"top": "tent"},
                "paste": "none",
            }),
        );
    }
    let (w, h) = (mm(size), mm(size / 2.0));
    s.fp_draw(&fp, "assembly_top", rect(-w, -h, w, h), mm(0.1));
    let (cw, ch) = (w + mm(0.25), h + mm(0.25));
    s.fp_draw(&fp, "courtyard_top", rect(-cw, -ch, cw, ch), mm(0.05));
    s.finish_footprint(&fp, ch)
}

/// A test point: one round SMD Pad to probe, with no paste so it stays bare copper. A
/// Standard Part, but out of the BOM and CPL because nothing is placed on it.
fn test_point(s: &mut Script) {
    let mut sym = s.symbol("TestPoint", "TP", "Test point");
    sym.names = false;
    sym.numbers = false;
    s.pin(&sym, bottom("~", "1", "passive", 0, -100));
    s.draw(&sym, circle(0, mil(50), mil(50)), false);
    let sym = s.finish_symbol(sym);
    let description = "Test point, 1.5 mm round SMD Pad, no paste";
    let fp = s.footprint("TestPoint_Pad_D1.5mm", description, COPPER);
    s.pad(
        &fp,
        json!({
            "number": "1",
            "kind": "smd",
            "shape": "circle",
            "at": {"x": 0, "y": 0},
            "width": mm(1.5),
            "height": mm(1.5),
            "paste": "none",
        }),
    );
    s.fp_draw(&fp, "assembly_top", circle(0, 0, mm(0.75)), mm(0.1));
    s.fp_draw(&fp, "courtyard_top", circle(0, 0, mm(1.0)), mm(0.05));
    let fp = s.finish_footprint(&fp, mm(1.0));
    s.part(PartSpec {
        name: "TestPoint",
        symbol: Some(&sym),
        footprints: vec![fp],
        fields: fields("TestPoint", description, "TP test point probe"),
        pin_map: None,
        kind: Kind::Unassembled,
    });
}
