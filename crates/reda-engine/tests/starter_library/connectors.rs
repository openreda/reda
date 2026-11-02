//! Pin headers, sockets, screw terminals and a USB-C receptacle.

use serde_json::json;

use crate::passives::fields;
use crate::script::{PartSpec, Script, bottom, left, line, mil, mm, rect, right};

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
        "USB 2.0 Type-C receptacle, 16 pins, horizontal mid-mount, mating face at the board edge",
    );
    for (number, x, wide) in [
        ("A1", -3.25, true),
        ("A4", -2.45, true),
        ("B8", -1.75, false),
        ("A5", -1.25, false),
        ("B7", -0.75, false),
        ("A6", -0.25, false),
        ("A7", 0.25, false),
        ("B6", 0.75, false),
        ("A8", 1.25, false),
        ("B5", 1.75, false),
        ("B4", 2.45, true),
        ("B1", 3.25, true),
    ] {
        s.pad(
            &fp,
            json!({
                "number": number,
                "kind": "smd",
                "shape": "rect",
                "at": {"x": mm(x), "y": mm(4.045)},
                "width": mm(if wide { 0.6 } else { 0.3 }),
                "height": mm(1.45),
            }),
        );
    }
    // The shell's slotted legs: long ones beside the Pad row, short ones nearer the face.
    for (number, x, y, height, slot) in [
        ("S1", -4.32, 3.13, 2.1, 1.7),
        ("S2", 4.32, 3.13, 2.1, 1.7),
        ("S3", -4.32, -1.05, 1.6, 1.2),
        ("S4", 4.32, -1.05, 1.6, 1.2),
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
                "at": {"x": mm(x), "y": mm(2.6)},
                "width": mm(0.65),
                "height": mm(0.65),
                "hole": {"width": mm(0.65)},
            }),
        );
    }
    let (bx, face, rear) = (mm(4.47), mm(-3.65), mm(3.65));
    s.fp_draw(&fp, "assembly_top", rect(-bx, face, bx, rear), mm(0.1));
    // Silkscreen down the sides, clear of the shell legs.
    for x in [-bx, bx] {
        s.fp_draw(&fp, "silkscreen_top", line(x, face, x, mm(-2.05)), mm(0.12));
        s.fp_draw(&fp, "silkscreen_top", line(x, 0, x, mm(1.85)), mm(0.12));
    }
    let (cx, top) = (mm(5.1), mm(5.05));
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
        mechanical: false,
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
        mechanical: false,
    });
}

/// Rising-clamp screw terminal blocks at 5.08 and 3.50 mm pitch.
fn screw_terminals(s: &mut Script) {
    for count in [2usize, 3] {
        let sym = format!("Conn_1x{count:02}");
        let sym = s
            .symbol_named(&sym)
            .unwrap_or_else(|| panic!("{sym} is made with the headers"));
        let mut footprints = vec![];
        for (pitch, pad, drill, depth) in [(5.08, 2.6, 1.3, 8.0), (3.5, 2.0, 1.1, 7.0)] {
            let name = format!("TERM_Screw_1x{count:02}_P{pitch:.2}mm");
            let description = format!("Screw terminal block, {count} positions, {pitch} mm pitch");
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
            let bx = count as f64 * pitch / 2.0;
            let (bottom, top) = (-depth * 0.4, depth * 0.6);
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
            mechanical: false,
        });
    }
}
