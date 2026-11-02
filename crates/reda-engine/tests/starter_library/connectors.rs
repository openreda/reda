//! Pin headers, sockets and screw terminals.

use serde_json::json;

use crate::passives::fields;
use crate::script::{PartSpec, Script, left, mil, mm, rect, right};

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
