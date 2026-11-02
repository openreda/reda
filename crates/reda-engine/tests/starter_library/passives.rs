//! Resistors, capacitors, inductors, ferrite beads, fuses, crystals and push buttons.

use serde_json::{Value, json};

use crate::script::{
    PartSpec, Pin, Script, Sym, arc, bottom, circle, left, line, mil, mm, polygon, rect, right, top,
};

pub fn build(s: &mut Script) {
    let r = resistors(s);
    capacitors(s);
    inductors(s);
    fuse(s, &r);
    crystals(s);
    button(s);
}

/// A vertical two-terminal Symbol: Pin 1 at the top, Pin 2 at the bottom, each `length`
/// mil long from ±150 mil, with names and numbers hidden.
fn two_pin(s: &mut Script, name: &str, prefix: &str, description: &str, length: i64) -> Sym {
    let mut sym = s.symbol(name, prefix, description);
    sym.names = false;
    sym.numbers = false;
    s.pin(
        &sym,
        Pin {
            length,
            ..top("~", "1", "passive", 0, 150)
        },
    );
    s.pin(
        &sym,
        Pin {
            length,
            ..bottom("~", "2", "passive", 0, -150)
        },
    );
    sym
}

pub fn fields(value: &str, description: &str, keywords: &str) -> Value {
    json!({"value": value, "description": description, "keywords": keywords})
}

pub fn chip(s: &mut Script, class: &str, size: &str) -> String {
    s.generate("chip", json!({"class": class, "size": size}))
}

/// A two-lead through-hole Footprint with leads `pitch` apart along x: Pad 1 (square) on
/// the left. `body` draws the Silkscreen and Assembly outline; the courtyard is the
/// bounding box of the pads and `extent` (half width, half height), plus 0.25 mm.
pub struct Tht<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub pitch: f64,
    pub pad: f64,
    pub drill: f64,
    pub extent: (f64, f64),
}

pub fn tht_two(s: &mut Script, t: Tht<'_>, body: impl FnOnce(&mut Script, &str)) -> String {
    let fp = s.footprint(t.name, t.description);
    for (number, x, shape) in [
        ("1", -t.pitch / 2.0, "rect"),
        ("2", t.pitch / 2.0, "circle"),
    ] {
        s.pad(
            &fp,
            json!({
                "number": number,
                "kind": "plated_through_hole",
                "shape": shape,
                "at": {"x": mm(x), "y": 0},
                "width": mm(t.pad),
                "height": mm(t.pad),
                "hole": {"width": mm(t.drill)},
            }),
        );
    }
    body(s, &fp);
    let cx = mm((t.pitch / 2.0 + t.pad / 2.0).max(t.extent.0) + 0.25);
    let cy = mm((t.pad / 2.0).max(t.extent.1) + 0.25);
    s.fp_draw(&fp, "courtyard_top", rect(-cx, -cy, cx, cy), mm(0.05));
    s.finish_footprint(&fp, cy)
}

/// An axial body `length` × `diameter` between leads `pitch` apart, Silkscreen around
/// it with lead lines out to the pads; `band` draws a cathode band that far left of
/// centre.
pub fn axial(
    s: &mut Script,
    fp: &str,
    pitch: f64,
    pad: f64,
    (length, diameter): (f64, f64),
    band: Option<f64>,
) {
    let (x, y) = (length / 2.0, diameter / 2.0);
    s.fp_draw(
        fp,
        "assembly_top",
        rect(mm(-x), mm(-y), mm(x), mm(y)),
        mm(0.1),
    );
    let (sx, sy) = (x + 0.06, y + 0.06);
    s.fp_draw(
        fp,
        "silkscreen_top",
        rect(mm(-sx), mm(-sy), mm(sx), mm(sy)),
        mm(0.12),
    );
    let lead_end = pitch / 2.0 - pad / 2.0 - 0.26;
    if lead_end > sx {
        for sign in [-1.0, 1.0] {
            s.fp_draw(
                fp,
                "silkscreen_top",
                line(mm(sign * sx), 0, mm(sign * lead_end), 0),
                mm(0.12),
            );
        }
    }
    if let Some(band) = band {
        for dx in [0.0, 0.15, 0.3] {
            let bx = mm(-band + dx);
            s.fp_draw(
                fp,
                "silkscreen_top",
                line(bx, mm(-sy), bx, mm(sy)),
                mm(0.12),
            );
        }
    }
}

fn resistors(s: &mut Script) -> Vec<String> {
    let mut sym = two_pin(s, "Resistor", "R", "Resistor", 50);
    sym.names = false;
    s.draw(&sym, rect(mil(-40), mil(-100), mil(40), mil(100)), false);
    let sym = s.finish_symbol(sym);
    let chips: Vec<String> = ["0402", "0603", "0805", "1206"]
        .iter()
        .map(|size| chip(s, "resistor", size))
        .collect();
    let axial_fp = tht_two(
        s,
        Tht {
            name: "RES_Axial_L6.3mm_D2.5mm_P10.16mm",
            description: "Axial resistor, 0207 body (6.3 × 2.5 mm), leads 10.16 mm apart",
            pitch: 10.16,
            pad: 1.6,
            drill: 0.8,
            extent: (3.25, 1.35),
        },
        |s, fp| axial(s, fp, 10.16, 1.6, (6.3, 2.5), None),
    );
    let mut footprints = chips.clone();
    footprints.push(axial_fp);
    s.part(PartSpec {
        name: "Resistor",
        symbol: Some(&sym),
        footprints,
        fields: fields("10k", "Resistor", "R resistor"),
        pin_map: None,
        mechanical: false,
    });
    chips
}

fn capacitors(s: &mut Script) {
    let sym = two_pin(s, "Capacitor", "C", "Capacitor", 130);
    for y in [20, -20] {
        s.draw(&sym, line(mil(-80), mil(y), mil(80), mil(y)), false);
    }
    let sym = s.finish_symbol(sym);
    let mut footprints: Vec<String> = ["0402", "0603", "0805", "1206"]
        .iter()
        .map(|size| chip(s, "capacitor", size))
        .collect();
    footprints.push(tht_two(
        s,
        Tht {
            name: "CAP_Disc_D5.0mm_W2.5mm_P5.08mm",
            description: "Ceramic disc capacitor, 5.0 × 2.5 mm body, leads 5.08 mm apart",
            pitch: 5.08,
            pad: 1.6,
            drill: 0.8,
            extent: (2.5, 1.25),
        },
        |s, fp| {
            s.fp_draw(
                fp,
                "assembly_top",
                rect(mm(-2.5), mm(-1.25), mm(2.5), mm(1.25)),
                mm(0.1),
            );
            for y in [-1.31, 1.31] {
                s.fp_draw(
                    fp,
                    "silkscreen_top",
                    line(mm(-2.56), mm(y), mm(2.56), mm(y)),
                    mm(0.12),
                );
            }
        },
    ));
    s.part(PartSpec {
        name: "Capacitor",
        symbol: Some(&sym),
        footprints,
        fields: fields("100n", "Unpolarised capacitor", "C capacitor ceramic"),
        pin_map: None,
        mechanical: false,
    });

    let sym = two_pin(
        s,
        "Capacitor_Polarised",
        "C",
        "Polarised capacitor; Pin 1 is positive",
        130,
    );
    s.draw(&sym, line(mil(-80), mil(20), mil(80), mil(20)), false);
    s.draw(&sym, rect(mil(-80), mil(-40), mil(80), mil(-20)), true);
    s.draw(&sym, line(mil(-70), mil(70), mil(-30), mil(70)), false);
    s.draw(&sym, line(mil(-50), mil(50), mil(-50), mil(90)), false);
    let sym = s.finish_symbol(sym);
    let mut footprints = vec![];
    for (name, length, width, height) in [
        ("CAPMP3216X180N", (3.0, 3.4), (1.4, 1.8), "1.8mm"),
        ("CAPMP3528X210N", (3.3, 3.7), (2.6, 3.0), "2.1mm"),
    ] {
        let fp = s.generate(
            "chip",
            json!({
                "class": "capacitor",
                "name": name,
                "dimensions": {
                    "length": {"min": mm(length.0), "max": mm(length.1)},
                    "width": {"min": mm(width.0), "max": mm(width.1)},
                    "terminal": {"min": "0.5mm", "max": "1.1mm"},
                    "height": height,
                },
            }),
        );
        s.mark_pad_one(&fp);
        footprints.push(fp);
    }
    for (diameter, pitch, pad, drill) in [(5.0, 2.0, 1.4, 0.8), (6.3, 2.5, 1.6, 0.8)] {
        let name = format!("CAPP_Radial_D{diameter:.1}mm_P{pitch:.2}mm");
        let description =
            format!("Radial electrolytic capacitor, {diameter} mm across, leads {pitch} mm apart");
        let r = diameter / 2.0;
        footprints.push(tht_two(
            s,
            Tht {
                name: &name,
                description: &description,
                pitch,
                pad,
                drill,
                extent: (r, r),
            },
            |s, fp| {
                s.fp_draw(fp, "assembly_top", circle(0, 0, mm(r)), mm(0.1));
                s.fp_draw(fp, "silkscreen_top", circle(0, 0, mm(r + 0.12)), mm(0.12));
                // A plus sign on the body by Pad 1.
                let (px, py) = (-pitch / 2.0, pad / 2.0 + 0.75);
                s.fp_draw(
                    fp,
                    "silkscreen_top",
                    line(mm(px - 0.4), mm(py), mm(px + 0.4), mm(py)),
                    mm(0.12),
                );
                s.fp_draw(
                    fp,
                    "silkscreen_top",
                    line(mm(px), mm(py - 0.4), mm(px), mm(py + 0.4)),
                    mm(0.12),
                );
            },
        ));
    }
    s.part(PartSpec {
        name: "Capacitor_Polarised",
        symbol: Some(&sym),
        footprints,
        fields: fields(
            "10u",
            "Polarised capacitor (tantalum or electrolytic)",
            "C capacitor polarised electrolytic tantalum",
        ),
        pin_map: None,
        mechanical: false,
    });
}

fn inductors(s: &mut Script) {
    let sym = two_pin(s, "Inductor", "L", "Inductor", 50);
    for y in [75, 25, -25, -75] {
        s.draw(&sym, arc((0, mil(y)), (0, mil(y + 25)), -180), false);
    }
    let sym = s.finish_symbol(sym);
    let footprints: Vec<String> = ["0603", "0805", "1210"]
        .iter()
        .map(|size| chip(s, "inductor", size))
        .collect();
    s.part(PartSpec {
        name: "Inductor",
        symbol: Some(&sym),
        footprints: footprints.clone(),
        fields: fields("10u", "Inductor", "L inductor choke"),
        pin_map: None,
        mechanical: false,
    });

    let sym = two_pin(s, "Ferrite_Bead", "FB", "Ferrite bead", 70);
    s.draw(
        &sym,
        polygon(&[
            (mil(-60), mil(30)),
            (mil(0), mil(80)),
            (mil(60), mil(-30)),
            (mil(0), mil(-80)),
        ]),
        false,
    );
    let sym = s.finish_symbol(sym);
    let mut beads: Vec<String> = vec![chip(s, "inductor", "0402")];
    beads.extend(footprints.into_iter().take(2));
    s.part(PartSpec {
        name: "Ferrite_Bead",
        symbol: Some(&sym),
        footprints: beads,
        fields: fields("600R@100MHz", "Ferrite bead", "FB ferrite bead EMI"),
        pin_map: None,
        mechanical: false,
    });
}

/// Chip fuses share the resistor land patterns.
fn fuse(s: &mut Script, resistor_chips: &[String]) {
    let sym = two_pin(s, "Fuse", "F", "Fuse", 50);
    s.draw(&sym, rect(mil(-30), mil(-100), mil(30), mil(100)), false);
    s.draw(&sym, line(0, mil(-100), 0, mil(100)), false);
    let sym = s.finish_symbol(sym);
    s.part(PartSpec {
        name: "Fuse",
        symbol: Some(&sym),
        footprints: vec![
            resistor_chips[3].clone(),
            resistor_chips[2].clone(),
            resistor_chips[1].clone(),
        ],
        fields: fields("500mA", "Fuse", "F fuse polyfuse PTC"),
        pin_map: None,
        mechanical: false,
    });
}

/// A horizontal two-terminal Symbol: Pin 1 on the left, Pin 2 on the right.
fn horizontal_two_pin(s: &mut Script, name: &str, prefix: &str, description: &str) -> Sym {
    let mut sym = s.symbol(name, prefix, description);
    sym.names = false;
    sym.numbers = false;
    s.pin(&sym, left("~", "1", "passive", -200, 0));
    s.pin(&sym, right("~", "2", "passive", 200, 0));
    sym
}

fn crystals(s: &mut Script) {
    let sym = horizontal_two_pin(s, "Crystal", "Y", "Quartz crystal");
    s.draw(&sym, rect(mil(-30), mil(-70), mil(30), mil(70)), false);
    for x in [-60, 60] {
        s.draw(&sym, line(mil(x), mil(-50), mil(x), mil(50)), false);
    }
    s.draw(&sym, line(mil(-100), 0, mil(-60), 0), false);
    s.draw(&sym, line(mil(60), 0, mil(100), 0), false);
    let sym = s.finish_symbol(sym);

    // A 3.2 × 2.5 mm ceramic package: the crystal between Pads 1 and 3, Pads 2 and 4 to
    // the lid.
    let fp = s.footprint(
        "XTAL_SMD_3225_4Pad",
        "SMD crystal, 3.2 × 2.5 mm ceramic package, 4 pads",
    );
    for (number, x, y) in [
        ("1", -1.1, 0.8),
        ("2", 1.1, 0.8),
        ("3", 1.1, -0.8),
        ("4", -1.1, -0.8),
    ] {
        s.pad(
            &fp,
            json!({
                "number": number,
                "kind": "smd",
                "shape": "rounded_rect",
                "at": {"x": mm(x), "y": mm(y)},
                "width": mm(1.4),
                "height": mm(1.15),
            }),
        );
    }
    s.fp_draw(
        &fp,
        "assembly_top",
        rect(mm(-1.6), mm(-1.25), mm(1.6), mm(1.25)),
        mm(0.1),
    );
    s.fp_draw(
        &fp,
        "silkscreen_top",
        line(mm(-2.2), mm(-0.2), mm(-2.2), mm(1.6)),
        mm(0.12),
    );
    s.fp_draw(
        &fp,
        "silkscreen_top",
        line(mm(-2.2), mm(1.6), mm(0.2), mm(1.6)),
        mm(0.12),
    );
    s.fp_draw(
        &fp,
        "courtyard_top",
        rect(mm(-2.1), mm(-1.65), mm(2.1), mm(1.65)),
        mm(0.05),
    );
    let smd = s.finish_footprint(&fp, mm(1.65));
    s.part(PartSpec {
        name: "Crystal_SMD",
        symbol: Some(&sym),
        footprints: vec![smd],
        fields: fields(
            "16MHz",
            "Quartz crystal, 3.2 × 2.5 mm SMD",
            "Y crystal quartz",
        ),
        pin_map: Some(json!({"pins": [{"pin": "2", "pads": ["3"]}], "unconnected": ["2", "4"]})),
        mechanical: false,
    });

    let hc49 = tht_two(
        s,
        Tht {
            name: "XTAL_HC-49US_P4.88mm",
            description: "Crystal, HC-49/US low-profile can, leads 4.88 mm apart",
            pitch: 4.88,
            pad: 1.5,
            drill: 0.8,
            extent: (5.5, 2.33),
        },
        |s, fp| {
            s.fp_draw(
                fp,
                "assembly_top",
                rect(mm(-5.5), mm(-2.33), mm(5.5), mm(2.33)),
                mm(0.1),
            );
            s.fp_draw(
                fp,
                "silkscreen_top",
                rect(mm(-5.56), mm(-2.39), mm(5.56), mm(2.39)),
                mm(0.12),
            );
        },
    );
    s.part(PartSpec {
        name: "Crystal_HC-49US",
        symbol: Some(&sym),
        footprints: vec![hc49],
        fields: fields("16MHz", "Quartz crystal, HC-49/US", "Y crystal quartz"),
        pin_map: None,
        mechanical: false,
    });
}

/// A 6 × 6 mm tactile switch: the two Pads numbered 1 are joined inside, as are the two
/// numbered 2; pressing joins 1 and 2.
fn button(s: &mut Script) {
    let sym = horizontal_two_pin(s, "Button", "SW", "Push button, normally open");
    for x in [-80, 80] {
        s.draw(&sym, circle(mil(x), 0, mil(20)), false);
    }
    s.draw(&sym, line(mil(-100), mil(50), mil(100), mil(50)), false);
    s.draw(&sym, line(0, mil(50), 0, mil(100)), false);
    s.draw(&sym, line(mil(-40), mil(100), mil(40), mil(100)), false);
    let sym = s.finish_symbol(sym);

    let mut footprints = vec![];
    for tht in [true, false] {
        let (name, description, x) = if tht {
            (
                "SW_Push_6x6mm_THT",
                "Tactile switch, 6 × 6 mm, through-hole",
                3.25,
            )
        } else {
            ("SW_Push_6x6mm_SMD", "Tactile switch, 6 × 6 mm, SMD", 3.975)
        };
        let fp = s.footprint(name, description);
        for (number, y) in [("1", 2.25), ("2", -2.25)] {
            for sx in [-x, x] {
                let pad = if tht {
                    json!({
                        "kind": "plated_through_hole", "shape": "circle",
                        "width": mm(1.8), "height": mm(1.8), "hole": {"width": mm(1.0)},
                    })
                } else {
                    json!({
                        "kind": "smd", "shape": "rounded_rect",
                        "width": mm(1.55), "height": mm(1.3),
                    })
                };
                let mut pad = pad;
                pad["number"] = json!(number);
                pad["at"] = json!({"x": mm(sx), "y": mm(y)});
                s.pad(&fp, pad);
            }
        }
        s.fp_draw(
            &fp,
            "assembly_top",
            rect(mm(-3.0), mm(-3.0), mm(3.0), mm(3.0)),
            mm(0.1),
        );
        s.fp_draw(&fp, "assembly_top", circle(0, 0, mm(1.75)), mm(0.1));
        // Silkscreen on the sides between the pads.
        for sx in [-3.06, 3.06] {
            s.fp_draw(
                &fp,
                "silkscreen_top",
                line(mm(sx), mm(-1.0), mm(sx), mm(1.0)),
                mm(0.12),
            );
        }
        for sy in [-3.06, 3.06] {
            s.fp_draw(
                &fp,
                "silkscreen_top",
                line(mm(-2.0), mm(sy), mm(2.0), mm(sy)),
                mm(0.12),
            );
        }
        let (cx, cy) = if tht { (4.4, 3.4) } else { (5.0, 3.25) };
        s.fp_draw(
            &fp,
            "courtyard_top",
            rect(mm(-cx), mm(-cy), mm(cx), mm(cy)),
            mm(0.05),
        );
        footprints.push(s.finish_footprint(&fp, mm(cy)));
    }
    s.part(PartSpec {
        name: "Button",
        symbol: Some(&sym),
        footprints,
        fields: fields(
            "Push",
            "Push button, normally open",
            "SW switch button tactile",
        ),
        pin_map: None,
        mechanical: false,
    });
}
