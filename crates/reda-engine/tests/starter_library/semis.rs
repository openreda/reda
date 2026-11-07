//! Diodes, LEDs, bipolar transistors and MOSFETs.
//!
//! Diode Pin 1 is the cathode, as on the Footprints, whose Pad 1 is marked.

use serde_json::json;

use crate::passives::{Tht, axial, fields, tht_two};
use crate::script::{
    Kind, PartSpec, Script, Sym, arc, arc_millideg, bottom, circle, left, line, mil, mm, polygon,
    rect, top,
};

pub fn build(s: &mut Script) {
    let diodes = diode_footprints(s);
    diode(
        s,
        "Diode",
        "Diode",
        "1N4148",
        "D diode rectifier signal",
        &diodes,
        Bar::Plain,
    );
    diode(
        s,
        "Diode_Schottky",
        "Schottky diode",
        "BAT54",
        "D diode schottky",
        &diodes,
        Bar::Schottky,
    );
    diode(
        s,
        "Diode_Zener",
        "Zener diode",
        "5V1",
        "D diode zener reference",
        &diodes,
        Bar::Zener,
    );
    led(s);
    let (sot23, to92) = transistor_footprints(s);
    bjt(s, true, &sot23, &to92);
    bjt(s, false, &sot23, &to92);
    mosfet(s, true, &sot23);
    mosfet(s, false, &sot23);
}

/// A vertical diode Symbol: anode (Pin 2) at the top, cathode (Pin 1) at the bottom, the
/// triangle pointing down.
fn diode_symbol(s: &mut Script, name: &str, description: &str, prefix: &str) -> Sym {
    let mut sym = s.symbol(name, prefix, description);
    sym.names = false;
    sym.numbers = false;
    s.pin(&sym, top("A", "2", "passive", 0, 150));
    s.pin(&sym, bottom("K", "1", "passive", 0, -150));
    s.draw(
        &sym,
        polygon(&[(mil(-50), mil(50)), (mil(50), mil(50)), (0, mil(-50))]),
        false,
    );
    sym
}

enum Bar {
    Plain,
    Schottky,
    Zener,
}

/// SOD-123, SMA (DO-214AC), DO-41 and DO-35, by name.
struct Diodes {
    sod123: String,
    sma: String,
    do41: String,
    do35: String,
}

/// A two-pad SMD Footprint: Pad 1 on the left, with a cathode bar beside it.
struct Smd<'a> {
    name: &'a str,
    description: &'a str,
    pitch: f64,
    pad: (f64, f64),
    body: (f64, f64),
}

fn smd_two(s: &mut Script, t: Smd<'_>) -> String {
    let fp = s.footprint(t.name, t.description);
    for (number, x) in [("1", -t.pitch / 2.0), ("2", t.pitch / 2.0)] {
        s.pad(
            &fp,
            json!({
                "number": number,
                "kind": "smd",
                "shape": "rounded_rect",
                "at": {"x": mm(x), "y": 0},
                "width": mm(t.pad.0),
                "height": mm(t.pad.1),
            }),
        );
    }
    let (bx, by) = (t.body.0 / 2.0, t.body.1 / 2.0);
    s.fp_draw(
        &fp,
        "assembly_top",
        rect(mm(-bx), mm(-by), mm(bx), mm(by)),
        mm(0.1),
    );
    let outer = t.pitch / 2.0 + t.pad.0 / 2.0;
    let mark = outer + 0.26;
    let sy = by.max(t.pad.1 / 2.0 + 0.26) + 0.06;
    for y in [-sy, sy] {
        s.fp_draw(
            &fp,
            "silkscreen_top",
            line(mm(-mark), mm(y), mm(bx), mm(y)),
            mm(0.12),
        );
    }
    s.fp_draw(
        &fp,
        "silkscreen_top",
        line(mm(-mark), mm(-sy), mm(-mark), mm(sy)),
        mm(0.12),
    );
    let cx = mm(mark + 0.06 + 0.25);
    let cy = mm(sy + 0.06 + 0.25);
    s.fp_draw(&fp, "courtyard_top", rect(-cx, -cy, cx, cy), mm(0.05));
    s.finish_footprint(&fp, cy)
}

fn diode_footprints(s: &mut Script) -> Diodes {
    // onsemi MMSD4148T1/D, SOD-123 case 425 issue H, Recommended Mounting Footprint:
    // 0.91 × 1.22 mm pads, centres 3.27 mm apart; body E × D 2.69 × 1.60 mm nominal.
    let sod123 = smd_two(
        s,
        Smd {
            name: "DIO_SOD-123",
            description: "Diode, SOD-123, 2.69 × 1.60 mm body. \
                          Lands per onsemi SOD-123 case 425, Recommended Mounting Footprint",
            pitch: 3.27,
            pad: (0.91, 1.22),
            body: (2.69, 1.6),
        },
    );
    // onsemi MBRA340T3/D, SMA case 403D issue J, Recommended Mounting Footprint:
    // 2.0 × 2.0 mm pads, centres 4.0 mm apart; body E × D 4.32 × 2.60 mm nominal.
    let sma = smd_two(
        s,
        Smd {
            name: "DIO_SMA",
            description: "Diode, SMA (DO-214AC), 4.32 × 2.60 mm body. \
                          Lands per onsemi SMA case 403D, Recommended Mounting Footprint",
            pitch: 4.0,
            pad: (2.0, 2.0),
            body: (4.32, 2.6),
        },
    );
    // onsemi 1N4001/D, DO-41 case 59-10: body 5.20 × 2.70 mm max, leads Ø0.86 mm max, so
    // a 1.1 mm hole and a 2.0 mm pad (IPC-2222 Level B: 1.06 and 1.66 mm at least).
    let do41 = tht_two(
        s,
        Tht {
            name: "DIO_Axial_DO-41_P10.16mm",
            description: "Diode, DO-41 axial, 5.2 × 2.7 mm body, leads 10.16 mm apart. \
                          Per onsemi DO-41 case 59-10",
            pitch: 10.16,
            pad: 2.0,
            drill: 1.1,
            extent: (2.6, 1.35),
        },
        |s, fp| axial(s, fp, 10.16, 2.0, (5.2, 2.7), Some(1.9)),
    );
    // onsemi 1N914/D, DO-35 axial lead case 017AG issue O: body 4.56 × 1.91 mm max, leads
    // Ø0.533 mm max, so a 0.8 mm hole and a 1.6 mm pad (IPC-2222 Level B: 0.73 and
    // 1.33 mm at least).
    let do35 = tht_two(
        s,
        Tht {
            name: "DIO_Axial_DO-35_P7.62mm",
            description: "Diode, DO-35 axial, 4.56 × 1.91 mm body, leads 7.62 mm apart. \
                          Per onsemi axial lead case 017AG",
            pitch: 7.62,
            pad: 1.6,
            drill: 0.8,
            extent: (2.28, 0.955),
        },
        |s, fp| axial(s, fp, 7.62, 1.6, (4.56, 1.91), Some(1.7)),
    );
    Diodes {
        sod123,
        sma,
        do41,
        do35,
    }
}

fn diode(
    s: &mut Script,
    name: &str,
    description: &str,
    value: &str,
    keywords: &str,
    fp: &Diodes,
    bar: Bar,
) {
    let sym = diode_symbol(s, name, description, "D");
    let y = mil(-50);
    s.draw(&sym, line(mil(-50), y, mil(50), y), false);
    match bar {
        Bar::Plain => {}
        Bar::Schottky => {
            s.draw(&sym, line(mil(50), y, mil(50), mil(-30)), false);
            s.draw(&sym, line(mil(50), mil(-30), mil(70), mil(-30)), false);
            s.draw(&sym, line(mil(-50), y, mil(-50), mil(-70)), false);
            s.draw(&sym, line(mil(-50), mil(-70), mil(-70), mil(-70)), false);
        }
        Bar::Zener => {
            s.draw(&sym, line(mil(50), y, mil(70), mil(-30)), false);
            s.draw(&sym, line(mil(-50), y, mil(-70), mil(-70)), false);
        }
    }
    let sym = s.finish_symbol(sym);
    let footprints = match bar {
        Bar::Zener => vec![fp.sod123.clone(), fp.do35.clone()],
        _ => vec![fp.sod123.clone(), fp.sma.clone(), fp.do41.clone()],
    };
    s.part(PartSpec {
        name,
        symbol: Some(&sym),
        footprints,
        fields: fields(value, description, keywords),
        pin_map: None,
        kind: Kind::Standard,
    });
}

fn led(s: &mut Script) {
    let sym = diode_symbol(s, "LED", "Light-emitting diode", "D");
    s.draw(&sym, line(mil(-50), mil(-50), mil(50), mil(-50)), false);
    for y in [0, -40] {
        s.draw(&sym, line(mil(60), mil(y), mil(110), mil(y - 50)), false);
        s.draw(
            &sym,
            polygon(&[
                (mil(110), mil(y - 50)),
                (mil(80), mil(y - 40)),
                (mil(100), mil(y - 20)),
            ]),
            true,
        );
    }
    let sym = s.finish_symbol(sym);
    let mut footprints = vec![];
    for (name, length, width, terminal, height) in [
        ("LEDC1608X80N", (1.5, 1.7), (0.7, 0.9), (0.2, 0.4), "0.8mm"),
        (
            "LEDC2012X100N",
            (1.9, 2.1),
            (1.15, 1.35),
            (0.3, 0.5),
            "1.0mm",
        ),
        ("LEDC3216X110N", (3.0, 3.4), (1.4, 1.8), (0.4, 0.6), "1.1mm"),
    ] {
        // The chip generator knows resistors, capacitors and inductors: an LED takes the
        // capacitor's fillets with its own description and a cathode mark.
        let description = format!("Chip LED, {} metric, IPC-7351 Nominal", &name[4..8]);
        let fp = s.generate(
            "chip",
            json!({
                "class": "capacitor",
                "name": name,
                "description": description,
                "polarity_mark": true,
                "dimensions": {
                    "length": {"min": mm(length.0), "max": mm(length.1)},
                    "width": {"min": mm(width.0), "max": mm(width.1)},
                    "terminal": {"min": mm(terminal.0), "max": mm(terminal.1)},
                    "height": height,
                },
            }),
        );
        footprints.push(fp);
    }
    // The 3 mm LED's pads are 1.6 mm (IPC-2222 Level B for its 0.9 mm hole is 1.5 mm) so
    // its Silkscreen ring stays 0.2 mm clear of the square Pad 1.
    for (size, pad, body, ring) in [(3.0, 1.6, 1.5, 2.5), (5.0, 1.8, 2.5, 2.96)] {
        let name = format!("LED_THT_D{size:.1}mm_P2.54mm");
        let description = format!("LED, {size} mm round, leads 2.54 mm apart");
        footprints.push(tht_two(
            s,
            Tht {
                name: &name,
                description: &description,
                pitch: 2.54,
                pad,
                drill: 0.9,
                extent: (ring + 0.06, ring + 0.06),
            },
            |s, fp| {
                s.fp_draw(fp, "assembly_top", circle(0, 0, mm(body)), mm(0.1));
                s.fp_draw(fp, "silkscreen_top", circle(0, 0, mm(ring)), mm(0.12));
            },
        ));
    }
    s.part(PartSpec {
        name: "LED",
        symbol: Some(&sym),
        footprints,
        fields: fields("LED", "Light-emitting diode", "D LED light emitting diode"),
        pin_map: None,
        kind: Kind::Standard,
    });
}

/// SOT-23 and a TO-92 with its leads in a line, Pads 1 to 3 from the left with the flat
/// face down.
///
/// The TO-92 is onsemi 2N3903/D, TO-92 (TO-226) case 29-11 issue AM, bent lead (tape and
/// reel, ammo pack): leads G 2.40 to 2.80 mm apart (2.54 here), D × J 0.54 × 0.50 mm max,
/// a 0.74 mm diagonal, so IPC-2222 Level B gives a 0.95 mm hole and a 1.55 mm pad; body A
/// 4.45 to 5.20 mm across and C 3.18 to 4.19 mm deep.
fn transistor_footprints(s: &mut Script) -> (String, String) {
    let sot23 = s.generate("sot", json!({"package": "sot23_3"}));
    let fp = s.footprint(
        "TO-92_Inline_P2.54mm",
        "TO-92, leads in a line 2.54 mm apart, flat face down. \
         Per onsemi TO-92 case 29-11, bent lead",
    );
    for (i, number) in ["1", "2", "3"].iter().enumerate() {
        let x = (i as f64 - 1.0) * 2.54;
        s.pad(
            &fp,
            json!({
                "number": number,
                "kind": "plated_through_hole",
                "shape": if i == 0 { "rect" } else { "oval" },
                "at": {"x": mm(x), "y": 0},
                "width": mm(1.55),
                "height": mm(1.8),
                "hole": {"width": mm(0.95)},
            }),
        );
    }
    // The body: a 4.8 mm circle (A nominal) cut by the flat face 1.6 mm below the leads,
    // 4.0 mm deep (C).
    let (r, flat) = (2.4_f64, -1.6_f64);
    let chord = (r * r - flat * flat).sqrt();
    let start = flat.atan2(chord).to_degrees();
    let sweep = ((180.0 - 2.0 * start) * 1000.0).round() as i64;
    s.fp_draw(
        &fp,
        "assembly_top",
        arc_millideg((0, 0), (mm(chord), mm(flat)), sweep),
        mm(0.1),
    );
    s.fp_draw(
        &fp,
        "assembly_top",
        line(mm(-chord), mm(flat), mm(chord), mm(flat)),
        mm(0.1),
    );
    let ring = r + 0.06;
    let corner = mm(ring * std::f64::consts::FRAC_1_SQRT_2);
    s.fp_draw(
        &fp,
        "silkscreen_top",
        arc((0, 0), (corner, corner), 90),
        mm(0.12),
    );
    let sy = mm(flat - 0.06);
    s.fp_draw(
        &fp,
        "silkscreen_top",
        line(mm(-chord), sy, mm(chord), sy),
        mm(0.12),
    );
    // Pads reach ±3.315 mm, the body 2.4 mm up and 1.6 mm down.
    s.fp_draw(
        &fp,
        "courtyard_top",
        rect(mm(-3.6), mm(-1.9), mm(3.6), mm(2.75)),
        mm(0.05),
    );
    let to92 = s.finish_footprint(&fp, mm(2.75));
    (sot23, to92)
}

/// The transistor outline shared by BJTs and MOSFETs.
fn transistor_circle(s: &mut Script, sym: &Sym) {
    s.draw(sym, circle(mil(30), 0, mil(130)), false);
}

/// NPN or PNP: base Pin 1 on the left, emitter Pin 2 at the bottom, collector Pin 3 at
/// the top (the SOT-23 order); the TO-92 Parts map the E-B-C leads onto them.
fn bjt(s: &mut Script, npn: bool, sot23: &str, to92: &str) {
    let (name, description) = if npn {
        ("NPN", "NPN bipolar transistor")
    } else {
        ("PNP", "PNP bipolar transistor")
    };
    let mut sym = s.symbol(name, "Q", description);
    sym.names = false;
    sym.numbers = false;
    s.pin(&sym, left("B", "1", "input", -200, 0));
    s.pin(&sym, bottom("E", "2", "passive", 100, -200));
    s.pin(&sym, top("C", "3", "passive", 100, 200));
    s.draw(&sym, line(mil(-30), mil(-75), mil(-30), mil(75)), false);
    s.draw(&sym, line(mil(-100), 0, mil(-30), 0), false);
    s.draw(&sym, line(mil(-30), mil(35), mil(100), mil(100)), false);
    s.draw(&sym, line(mil(-30), mil(-35), mil(100), mil(-100)), false);
    let arrow = if npn {
        [(70, -85), (47, -57), (33, -83)]
    } else {
        [(10, -55), (47, -57), (33, -83)]
    };
    let arrow: Vec<(i64, i64)> = arrow.iter().map(|&(x, y)| (mil(x), mil(y))).collect();
    s.draw(&sym, polygon(&arrow), true);
    transistor_circle(s, &sym);
    let sym = s.finish_symbol(sym);
    let generic = if npn { "MMBT3904" } else { "MMBT3906" };
    s.part(PartSpec {
        name,
        symbol: Some(&sym),
        footprints: vec![sot23.to_owned()],
        fields: fields(generic, description, "Q transistor BJT bipolar"),
        pin_map: None,
        kind: Kind::Standard,
    });
    let through_hole = if npn { "2N3904" } else { "2N3906" };
    let to92_name = format!("{name}_TO-92");
    s.part(PartSpec {
        name: &to92_name,
        symbol: Some(&sym),
        footprints: vec![to92.to_owned()],
        fields: fields(
            through_hole,
            &format!("{description}, TO-92 with E-B-C leads"),
            "Q transistor BJT bipolar",
        ),
        pin_map: Some(json!({"pins": [
            {"pin": "1", "pads": ["2"]},
            {"pin": "2", "pads": ["1"]},
        ]})),
        kind: Kind::Standard,
    });
}

/// N- or P-channel enhancement MOSFET: gate Pin 1 on the left, source Pin 2 at the
/// bottom, drain Pin 3 at the top (the SOT-23 order).
fn mosfet(s: &mut Script, n_channel: bool, sot23: &str) {
    let (name, description, value) = if n_channel {
        ("NMOS", "N-channel enhancement MOSFET", "2N7002")
    } else {
        ("PMOS", "P-channel enhancement MOSFET", "BSS84")
    };
    let mut sym = s.symbol(name, "Q", description);
    sym.names = false;
    sym.numbers = false;
    s.pin(&sym, left("G", "1", "input", -200, -50));
    s.pin(&sym, bottom("S", "2", "passive", 100, -200));
    s.pin(&sym, top("D", "3", "passive", 100, 200));
    s.draw(&sym, line(mil(-60), mil(-75), mil(-60), mil(75)), false);
    for (y1, y2) in [(45, 85), (-20, 20), (-85, -45)] {
        s.draw(&sym, line(mil(-30), mil(y1), mil(-30), mil(y2)), false);
    }
    s.draw(&sym, line(mil(-100), mil(-50), mil(-60), mil(-50)), false);
    s.draw(&sym, line(mil(-30), mil(65), mil(100), mil(65)), false);
    s.draw(&sym, line(mil(100), mil(65), mil(100), mil(100)), false);
    s.draw(&sym, line(mil(-30), mil(-65), mil(100), mil(-65)), false);
    s.draw(&sym, line(mil(100), mil(-65), mil(100), mil(-100)), false);
    s.draw(&sym, line(mil(-30), 0, mil(100), 0), false);
    s.draw(&sym, line(mil(100), 0, mil(100), mil(-65)), false);
    let arrow = if n_channel {
        [(-30, 0), (10, 20), (10, -20)]
    } else {
        [(50, 0), (10, 20), (10, -20)]
    };
    let arrow: Vec<(i64, i64)> = arrow.iter().map(|&(x, y)| (mil(x), mil(y))).collect();
    s.draw(&sym, polygon(&arrow), true);
    transistor_circle(s, &sym);
    let sym = s.finish_symbol(sym);
    s.part(PartSpec {
        name,
        symbol: Some(&sym),
        footprints: vec![sot23.to_owned()],
        fields: fields(value, description, "Q transistor MOSFET FET"),
        pin_map: None,
        kind: Kind::Standard,
    });
}
