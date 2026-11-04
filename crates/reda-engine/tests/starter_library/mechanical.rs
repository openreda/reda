//! Mechanical Parts, placed straight on the PCB with no Symbol: mounting holes and
//! fiducials.

use serde_json::json;

use crate::passives::fields;
use crate::script::{PartSpec, Script, circle, mm};

pub fn build(s: &mut Script) {
    // Screw size, clearance hole, screw head diameter: the hole is ISO 273's fine
    // clearance hole (M2 2.2, M2.5 2.7, M3 3.2 mm), the head ISO 4762's largest socket
    // head (M2 3.8, M2.5 4.5, M3 5.5 mm).
    for (screw, hole, head) in [("M2", 2.2, 3.8), ("M2.5", 2.7, 4.5), ("M3", 3.2, 5.5)] {
        mounting_hole(s, screw, hole, head, false);
    }
    mounting_hole(s, "M3", 3.2, 5.5, true);
    fiducial(s);
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
        mechanical: true,
    });
}

/// A 1 mm copper dot in a 2 mm solder-mask opening, for pick-and-place cameras.
fn fiducial(s: &mut Script) {
    let description = "Fiducial mark, 1 mm copper in a 2 mm mask opening";
    let fp = s.footprint("Fiducial_1mm_Mask2mm", description);
    s.pad(
        &fp,
        json!({
            "number": "",
            "kind": "smd",
            "shape": "circle",
            "at": {"x": 0, "y": 0},
            "width": mm(1.0),
            "height": mm(1.0),
            "mask_margin": mm(0.5),
        }),
    );
    s.fp_draw(&fp, "courtyard_top", circle(0, 0, mm(1.25)), mm(0.05));
    let fp = s.finish_footprint(&fp, mm(1.25));
    s.part(PartSpec {
        name: "Fiducial",
        symbol: None,
        footprints: vec![fp],
        fields: fields("Fiducial", description, "FID fiducial mechanical"),
        pin_map: None,
        mechanical: true,
    });
}
