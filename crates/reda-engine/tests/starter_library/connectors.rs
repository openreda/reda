//! Pin headers, sockets, screw terminals and a USB-C receptacle.

use serde_json::json;

use crate::passives::fields;
use crate::script::{Kind, PartSpec, Script, bottom, left, line, mil, mm, rect, right};

/// The single-row sizes, then the double-row ones.
const SINGLE: [usize; 7] = [2, 3, 4, 5, 6, 8, 10];
const DOUBLE: [usize; 4] = [3, 5, 8, 10];

pub fn build(s: &mut Script) {
    for rows in [1, 2] {
        let sizes: &[usize] = if rows == 1 { &SINGLE } else { &DOUBLE };
        for &count in sizes {
            let sym = connector_symbol(s, rows, count);
            for socket in [false, true] {
                header(s, &sym, rows, count, socket);
            }
        }
    }
    screw_terminals(s);
    usb_c(s);
}

/// A USB 2.0 Type-C receptacle: the common 16-pin horizontal mid-mount layout, with the
/// mating face at the bottom (−y) edge of the Footprint so it sits at a board edge. Pads
/// that carry the same signal for both plug orientations are one Pad (`A1` is A1 and
/// B12), and the Pin map ties the remaining pairs and the four shell legs together.
///
/// The land pattern is GCT USB4105 drawing (revision B4, 18/12/23), Recommended PCB
/// Layout: 12 pads 1.15 mm long, 0.60 mm wide at ±3.20 and ±2.40 mm, 0.30 mm wide on a
/// 0.50 mm pitch between; shell slots at ±4.32 mm, 1.00 × 2.10 mm pads with 0.60 × 1.70 mm
/// slots at the rear and 1.00 × 1.80 mm with 0.60 × 1.40 mm in front, 4.18 mm apart, the
/// pads' inner edge level with the rear slots; Ø0.65 mm pegs at ±2.89 mm, 3.68 mm behind
/// the front slots; the board edge 2.60 mm in front of them; body 8.94 × 7.35 mm.
fn usb_c(s: &mut Script) {
    let sym = s.symbol("USB_C_USB2.0", "J", "USB 2.0 Type-C receptacle");
    for (name, number, y) in [
        ("VBUS", "A4", 300),
        ("CC1", "A5", 150),
        ("CC2", "B5", 50),
        ("D+", "A6", -50),
        ("D-", "A7", -150),
        ("SBU1", "A8", -250),
        ("SBU2", "B8", -350),
    ] {
        s.pin(&sym, right(name, number, "passive", 400, y));
    }
    s.pin(&sym, bottom("GND", "A1", "passive", 0, -550));
    s.pin(&sym, bottom("SHIELD", "S1", "passive", -200, -550));
    s.draw(&sym, rect(mil(-300), mil(-450), mil(300), mil(400)), false);
    let sym = s.finish_symbol(sym);

    let fp = s.footprint(
        "USB_C_Receptacle_16P_Horizontal",
        "USB 2.0 Type-C receptacle, 16 pins, horizontal mid-mount, mating face at the board \
         edge. Lands per GCT USB4105 drawing, Recommended PCB Layout",
    );
    for (number, x, wide) in [
        ("A1", -3.2, true),
        ("A4", -2.4, true),
        ("B8", -1.75, false),
        ("A5", -1.25, false),
        ("B7", -0.75, false),
        ("A6", -0.25, false),
        ("A7", 0.25, false),
        ("B6", 0.75, false),
        ("A8", 1.25, false),
        ("B5", 1.75, false),
        ("B4", 2.4, true),
        ("B1", 3.2, true),
    ] {
        s.pad(
            &fp,
            json!({
                "number": number,
                "kind": "smd",
                "shape": "rect",
                "at": {"x": mm(x), "y": mm(3.705)},
                "width": mm(if wide { 0.6 } else { 0.3 }),
                "height": mm(1.15),
            }),
        );
    }
    // The shell's slotted legs: long ones beside the Pad row, short ones nearer the face.
    for (number, x, y, height, slot) in [
        ("S1", -4.32, 3.13, 2.1, 1.7),
        ("S2", 4.32, 3.13, 2.1, 1.7),
        ("S3", -4.32, -1.05, 1.8, 1.4),
        ("S4", 4.32, -1.05, 1.8, 1.4),
    ] {
        s.pad(
            &fp,
            json!({
                "number": number,
                "kind": "plated_through_hole",
                "shape": "oval",
                "at": {"x": mm(x), "y": mm(y)},
                "width": mm(1.0),
                "height": mm(height),
                "hole": {"width": mm(0.6), "height": mm(slot)},
            }),
        );
    }
    // The locating pegs.
    for x in [-2.89, 2.89] {
        s.pad(
            &fp,
            json!({
                "number": "",
                "kind": "non_plated",
                "shape": "circle",
                "at": {"x": mm(x), "y": mm(2.63)},
                "width": mm(0.65),
                "height": mm(0.65),
                "hole": {"width": mm(0.65)},
            }),
        );
    }
    let (bx, face, rear) = (mm(4.47), mm(-3.65), mm(3.7));
    s.fp_draw(&fp, "assembly_top", rect(-bx, face, bx, rear), mm(0.1));
    // Silkscreen down the sides, 0.2 mm clear of the shell legs' pads.
    for x in [-bx, bx] {
        s.fp_draw(&fp, "silkscreen_top", line(x, face, x, mm(-2.21)), mm(0.12));
        s.fp_draw(
            &fp,
            "silkscreen_top",
            line(x, mm(0.11), x, mm(1.82)),
            mm(0.12),
        );
    }
    let (cx, top) = (mm(5.1), mm(4.55));
    s.fp_draw(&fp, "courtyard_top", rect(-cx, mm(-3.9), cx, top), mm(0.05));
    let fp = s.finish_footprint(&fp, top);

    s.part(PartSpec {
        name: "USB_C_Receptacle_USB2.0",
        symbol: Some(&sym),
        footprints: vec![fp],
        fields: fields(
            "USB-C",
            "USB 2.0 Type-C receptacle, 16 pins, horizontal mid-mount",
            "J connector USB Type-C receptacle",
        ),
        pin_map: Some(json!({"pins": [
            {"pin": "A1", "pads": ["A1", "B1"]},
            {"pin": "A4", "pads": ["A4", "B4"]},
            {"pin": "A6", "pads": ["A6", "B6"]},
            {"pin": "A7", "pads": ["A7", "B7"]},
            {"pin": "S1", "pads": ["S1", "S2", "S3", "S4"]},
        ]})),
        kind: Kind::Standard,
    });
}

/// `Conn_<rows>x<count>`: a 1×N connector has its Pins down the left; a 2×N one has the
/// odd Pins on the left and the even ones on the right, as the Footprints number them.
fn connector_symbol(s: &mut Script, rows: usize, count: usize) -> String {
    let name = format!("Conn_{rows}x{count:02}");
    let description = format!("Generic connector, {rows} × {count} pins");
    let mut sym = s.symbol(&name, "J", &description);
    sym.names = false;
    let count_i = count as i64;
    let mut number = 1;
    for i in 0..count_i {
        let y = ((count_i - 1) - 2 * i) * 50;
        for r in 0..rows {
            let n = number.to_string();
            let pin_name = format!("Pin_{n}");
            if r == 0 {
                s.pin(&sym, left(&pin_name, &n, "passive", -200, y));
            } else {
                s.pin(&sym, right(&pin_name, &n, "passive", 200, y));
            }
            number += 1;
        }
    }
    let half = count_i * 50;
    let x2 = if rows == 1 { 0 } else { 100 };
    s.draw(&sym, rect(mil(-100), mil(-half), mil(x2), mil(half)), false);
    s.finish_symbol(sym)
}

fn header(s: &mut Script, sym: &str, rows: usize, count: usize, socket: bool) {
    let footprints: Vec<String> = ["2.54", "1.27"]
        .iter()
        .map(|pitch| {
            s.generate(
                "header",
                json!({"rows": rows, "pins_per_row": count, "pitch": pitch, "socket": socket}),
            )
        })
        .collect();
    let kind = if socket { "Socket" } else { "Header" };
    let name = format!("{kind}_{rows}x{count:02}");
    let noun = if socket { "socket" } else { "pin header" };
    let description = format!("{rows} × {count} {noun}, 2.54 or 1.27 mm pitch");
    s.part(PartSpec {
        name: &name,
        symbol: Some(sym),
        footprints,
        fields: fields(
            &format!("{kind} {rows}x{count}"),
            &description,
            &format!("J connector {noun}"),
        ),
        pin_map: None,
        kind: Kind::Standard,
    });
}

/// Rising-clamp screw terminal blocks at 5.08 and 3.50 mm pitch.
///
/// 5.08 mm: Same Sky TB007-508 datasheet (rev 1.03), Mechanical Drawing and PCB Layout:
/// Ø1.60 mm holes; body L = 5.08 × N, 10.25 mm deep with the pins 4.90 mm from the back.
///
/// 3.50 mm: Würth Elektronik 691214110002 (WR-TBL Series 2141, rising cage clamp)
/// datasheet, Dimensions and Recommended Hole Pattern: Ø1.3 mm holes; body 7.05 mm long for
/// two positions (1.775 mm past the end pins), 7.4 mm deep with the pins 4.0 mm from the
/// wire entry.
///
/// Pads are the hole + 1.0 mm, more than IPC-2221 Level B's hole + 0.6 mm.
fn screw_terminals(s: &mut Script) {
    for count in [2usize, 3] {
        let sym = format!("Conn_1x{count:02}");
        let sym = s
            .symbol_named(&sym)
            .unwrap_or_else(|| panic!("{sym} is made with the headers"));
        let mut footprints = vec![];
        // Pitch, pad, hole, body past the end pins, and body depth in front of (wire
        // entry) and behind the pins.
        for (pitch, pad, drill, end, entry, back, source) in [
            (
                5.08,
                2.6,
                1.6,
                2.54,
                5.35,
                4.9,
                "Same Sky TB007-508 datasheet",
            ),
            (
                3.5,
                2.3,
                1.3,
                1.775,
                4.0,
                3.4,
                "Würth Elektronik 691214110002 datasheet",
            ),
        ] {
            let name = format!("TERM_Screw_1x{count:02}_P{pitch:.2}mm");
            let description =
                format!("Screw terminal block, {count} positions, {pitch} mm pitch. Per {source}");
            let fp = s.footprint(&name, &description);
            let first = -(count as f64 - 1.0) * pitch / 2.0;
            for i in 0..count {
                let x = first + i as f64 * pitch;
                s.pad(
                    &fp,
                    json!({
                        "number": (i + 1).to_string(),
                        "kind": "plated_through_hole",
                        "shape": if i == 0 { "rect" } else { "circle" },
                        "at": {"x": mm(x), "y": 0},
                        "width": mm(pad),
                        "height": mm(pad),
                        "hole": {"width": mm(drill)},
                    }),
                );
            }
            // The block runs from the wire entry (below) to the screws (above).
            let bx = (count as f64 - 1.0) * pitch / 2.0 + end;
            let (bottom, top) = (-entry, back);
            s.fp_draw(
                &fp,
                "assembly_top",
                rect(mm(-bx), mm(bottom), mm(bx), mm(top)),
                mm(0.1),
            );
            s.fp_draw(
                &fp,
                "silkscreen_top",
                rect(
                    mm(-bx - 0.06),
                    mm(bottom - 0.06),
                    mm(bx + 0.06),
                    mm(top + 0.06),
                ),
                mm(0.12),
            );
            let (cx, cy0, cy1) = (mm(bx + 0.37), mm(bottom - 0.37), mm(top + 0.37));
            s.fp_draw(&fp, "courtyard_top", rect(-cx, cy0, cx, cy1), mm(0.05));
            footprints.push(s.finish_footprint(&fp, cy1));
        }
        let name = format!("Screw_Terminal_1x{count:02}");
        s.part(PartSpec {
            name: &name,
            symbol: Some(&sym),
            footprints,
            fields: fields(
                &format!("Terminal 1x{count}"),
                &format!("Screw terminal block, {count} positions"),
                "J connector terminal screw",
            ),
            pin_map: None,
            kind: Kind::Standard,
        });
    }
}
