//! A generic dual op-amp, a 555 timer, an LDO regulator and a generic USB microcontroller.

use serde_json::json;

use crate::passives::fields;
use crate::script::{
    PartSpec, Script, Sym, bottom, left, line, mil, polygon, rect, right, text, top,
};

pub fn build(s: &mut Script) {
    let soic8 = s.generate("soic", json!({"pins": 8}));
    let tssop8 = s.generate("tssop", json!({"pins": 8}));
    let dip8 = s.generate("dip", json!({"pins": 8}));
    op_amp(s, vec![soic8.clone(), tssop8, dip8.clone()]);
    timer(s, vec![soic8, dip8]);
    ldo(s);
    mcu(s);
}

/// One amplifier: the triangle, + and − inputs on the left, the output on the right.
fn amplifier(s: &mut Script, sym: &Sym, plus: &str, minus: &str, out: &str) {
    s.pin(sym, left("+", plus, "input", -200, 50));
    s.pin(sym, left("-", minus, "input", -200, -50));
    s.pin(sym, right("~", out, "output", 250, 0));
    s.draw(
        sym,
        polygon(&[(mil(-100), mil(150)), (mil(-100), mil(-150)), (mil(150), 0)]),
        false,
    );
    s.draw(sym, line(mil(-85), mil(50), mil(-55), mil(50)), false);
    s.draw(sym, line(mil(-70), mil(35), mil(-70), mil(65)), false);
    s.draw(sym, line(mil(-85), mil(-50), mil(-55), mil(-50)), false);
}

/// A dual op-amp in the industry-standard 8-pin order: amplifiers A and B as Units, and
/// the supply Pins on their own Power Unit.
fn op_amp(s: &mut Script, footprints: Vec<String>) {
    let mut sym = s.symbol(
        "OpAmp_Dual",
        "U",
        "Dual operational amplifier, standard 8-pin pinout",
    );
    sym.names = false;
    amplifier(s, &sym, "3", "2", "1");
    s.unit(&mut sym, "B");
    amplifier(s, &sym, "5", "6", "7");
    s.unit(&mut sym, "Power");
    sym.names = true;
    s.pin(&sym, top("V+", "8", "power_in", 0, 200));
    s.pin(&sym, bottom("V-", "4", "power_in", 0, -200));
    s.draw(&sym, rect(mil(-50), mil(-100), mil(50), mil(100)), false);
    let sym = s.finish_symbol(sym);
    s.part(PartSpec {
        name: "OpAmp_Dual",
        symbol: Some(&sym),
        footprints,
        fields: fields(
            "LM358",
            "Dual operational amplifier",
            "U op-amp opamp operational amplifier dual",
        ),
        pin_map: None,
        mechanical: false,
    });
}

fn timer(s: &mut Script, footprints: Vec<String>) {
    let sym = s.symbol("Timer_555", "U", "555 timer");
    s.pin(&sym, top("VCC", "8", "power_in", 0, 350));
    s.pin(&sym, bottom("GND", "1", "power_in", 0, -350));
    s.pin(&sym, left("RESET", "4", "input", -300, 150));
    s.pin(&sym, left("TRIG", "2", "input", -300, 50));
    s.pin(&sym, left("THR", "6", "input", -300, -50));
    s.pin(&sym, left("CV", "5", "input", -300, -150));
    s.pin(&sym, right("OUT", "3", "output", 300, 50));
    s.pin(&sym, right("DIS", "7", "open_collector", 300, -50));
    s.draw(&sym, rect(mil(-200), mil(-250), mil(200), mil(250)), false);
    let sym = s.finish_symbol(sym);
    s.part(PartSpec {
        name: "Timer_555",
        symbol: Some(&sym),
        footprints,
        fields: fields("NE555", "555 timer", "U timer 555 oscillator"),
        pin_map: None,
        mechanical: false,
    });
}

/// A fixed-output LDO in SOT-223 with the common pinout: 1 GND, 2 and the tab VOUT,
/// 3 VIN.
fn ldo(s: &mut Script) {
    let sot223 = s.generate("sot", json!({"package": "sot223"}));
    let sym = s.symbol("LDO", "U", "Fixed-output low-dropout regulator");
    s.pin(&sym, left("VIN", "3", "power_in", -300, 0));
    s.pin(&sym, right("VOUT", "2", "power_out", 300, 0));
    s.pin(&sym, bottom("GND", "1", "power_in", 0, -200));
    s.draw(&sym, rect(mil(-200), mil(-100), mil(200), mil(100)), false);
    s.draw(&sym, text(0, mil(50), "LDO", mil(50)), false);
    let sym = s.finish_symbol(sym);
    s.part(PartSpec {
        name: "LDO_SOT-223",
        symbol: Some(&sym),
        footprints: vec![sot223],
        fields: fields(
            "3.3V",
            "Fixed-output LDO regulator, SOT-223, GND-VOUT-VIN with VOUT on the tab",
            "U regulator LDO linear voltage",
        ),
        pin_map: Some(json!({"pins": [{"pin": "2", "pads": ["2", "4"]}]})),
        mechanical: false,
    });
}

/// A vendor-neutral USB microcontroller in TSSOP-20, so examples and the reference
/// Projects can show an MCU board without shipping a vendor's Part. Its pinout is REDA's
/// own: supplies, reset, boot, SWD and USB on fixed Pins, the rest general-purpose I/O.
fn mcu(s: &mut Script) {
    let tssop20 = s.generate("tssop", json!({"pins": 20}));
    let sym = s.symbol(
        "MCU_USB",
        "U",
        "Generic microcontroller with USB device, SWD and 11 I/O Pins",
    );
    s.pin(&sym, top("VDD", "11", "power_in", -100, 650));
    s.pin(&sym, top("VDDA", "5", "power_in", 100, 650));
    s.pin(&sym, bottom("VSS", "8", "power_in", 0, -650));
    s.pin(&sym, left("BOOT0", "1", "input", -500, 300));
    s.pin(&sym, left("NRST", "4", "input", -500, 200));
    s.pin(&sym, left("SWDIO", "19", "bidirectional", -500, 0));
    s.pin(&sym, left("SWCLK", "18", "input", -500, -100));
    s.pin(&sym, left("USB_DP", "10", "bidirectional", -500, -300));
    s.pin(&sym, left("USB_DM", "9", "bidirectional", -500, -400));
    let io = ["2", "3", "6", "7", "12", "13", "14", "15", "16", "17", "20"];
    for (i, number) in io.iter().enumerate() {
        let name = format!("P{i}");
        let y = 500 - 100 * i as i64;
        s.pin(&sym, right(&name, number, "bidirectional", 500, y));
    }
    s.draw(&sym, rect(mil(-400), mil(-550), mil(400), mil(550)), false);
    s.draw(&sym, text(0, mil(-450), "MCU", mil(50)), false);
    let sym = s.finish_symbol(sym);
    s.part(PartSpec {
        name: "MCU_USB_TSSOP-20",
        symbol: Some(&sym),
        footprints: vec![tssop20],
        fields: fields(
            "MCU",
            "Generic USB microcontroller, TSSOP-20: a vendor-neutral stand-in for examples; \
             use your own Library or Import part for a real MCU",
            "U microcontroller MCU USB",
        ),
        pin_map: None,
        mechanical: false,
    });
}
