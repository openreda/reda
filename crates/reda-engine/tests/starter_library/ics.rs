//! A generic dual op-amp, a 555 timer, an LDO regulator and the STM32F042F6P6 USB
//! microcontroller.

use serde_json::json;

use crate::passives::fields;
use crate::script::{
    Kind, PartSpec, Script, Sym, bottom, left, line, mil, polygon, rect, right, text, top,
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

/// One amplifier: the triangle, centred on the origin, + and − inputs on the left, the
/// output on the right.
fn amplifier(s: &mut Script, sym: &Sym, plus: &str, minus: &str, out: &str) {
    s.pin(sym, left("+", plus, "input", -250, 50));
    s.pin(sym, left("-", minus, "input", -250, -50));
    s.pin(sym, right("~", out, "output", 250, 0));
    s.draw(
        sym,
        polygon(&[(mil(-150), mil(150)), (mil(-150), mil(-150)), (mil(150), 0)]),
        false,
    );
    s.draw(sym, line(mil(-135), mil(50), mil(-105), mil(50)), false);
    s.draw(sym, line(mil(-120), mil(35), mil(-120), mil(65)), false);
    s.draw(sym, line(mil(-135), mil(-50), mil(-105), mil(-50)), false);
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
        kind: Kind::Standard,
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
    s.origin_at_pin_1(&sym);
    let sym = s.finish_symbol(sym);
    s.part(PartSpec {
        name: "Timer_555",
        symbol: Some(&sym),
        footprints,
        fields: fields("NE555", "555 timer", "U timer 555 oscillator"),
        pin_map: None,
        kind: Kind::Standard,
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
    s.origin_at_pin_1(&sym);
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
        kind: Kind::Standard,
    });
}

/// The STMicroelectronics STM32F042F6P6: a Cortex-M0 with crystal-less USB in TSSOP-20,
/// so examples and the reference Projects can show a USB MCU board.
///
/// Pinout from the STM32F042x4/x6 datasheet (DocID025832), Figure 9 "TSSOP20 20-pin
/// package (top view)" and Table 13 "STM32F042xx pin definitions", TSSOP20 column. Pins
/// 17 and 18 are PA9 and PA10, or PA11 (USB_DM) and PA12 (USB_DP) once SYSCFG_CFGR1
/// remaps them; VSSA is bonded to VSS and VDDIO2 to VDD inside this package. The
/// footprint is the generated TSSOP-20 (JEDEC MO-153, 4.4 mm body, 0.65 mm pitch), as in
/// the datasheet's Figure 44 and Table 72.
fn mcu(s: &mut Script) {
    let tssop20 = s.generate("tssop", json!({"pins": 20}));
    let sym = s.symbol(
        "STM32F042F6P6",
        "U",
        "STM32F042F6P6 Cortex-M0 microcontroller with USB, TSSOP-20 pinout",
    );
    s.pin(&sym, top("VDD", "16", "power_in", -100, 650));
    s.pin(&sym, top("VDDA", "5", "power_in", 100, 650));
    s.pin(&sym, bottom("VSS", "15", "power_in", 0, -650));
    s.pin(&sym, left("PB8/BOOT0", "1", "bidirectional", -600, 300));
    s.pin(&sym, left("NRST", "4", "bidirectional", -600, 200));
    s.pin(&sym, left("PA13/SWDIO", "19", "bidirectional", -600, 0));
    s.pin(&sym, left("PA14/SWCLK", "20", "bidirectional", -600, -100));
    s.pin(&sym, left("PA10/PA12", "18", "bidirectional", -600, -300));
    s.pin(&sym, left("PA9/PA11", "17", "bidirectional", -600, -400));
    let io = [
        ("PA0", "6"),
        ("PA1", "7"),
        ("PA2", "8"),
        ("PA3", "9"),
        ("PA4", "10"),
        ("PA5", "11"),
        ("PA6", "12"),
        ("PA7", "13"),
        ("PB1", "14"),
        ("PF0/OSC_IN", "2"),
        ("PF1/OSC_OUT", "3"),
    ];
    for (i, (name, number)) in io.iter().enumerate() {
        let y = 500 - 100 * i as i64;
        s.pin(&sym, right(name, number, "bidirectional", 600, y));
    }
    s.draw(&sym, rect(mil(-500), mil(-550), mil(500), mil(550)), false);
    s.draw(&sym, text(0, mil(-450), "STM32F042", mil(50)), false);
    s.origin_at_pin_1(&sym);
    let sym = s.finish_symbol(sym);
    s.part(PartSpec {
        name: "STM32F042F6P6",
        symbol: Some(&sym),
        footprints: vec![tssop20],
        fields: json!({
            "value": "STM32F042F6P6",
            "description": "Cortex-M0 MCU, 32 KB Flash, crystal-less USB FS, TSSOP-20. \
                            Pins 17 and 18 are PA9/PA10 or, remapped, USB DM/DP (PA11/PA12)",
            "datasheet": "STM32F042x4/x6 datasheet, DocID025832",
            "manufacturer": "STMicroelectronics",
            "mpn": "STM32F042F6P6",
            "keywords": "U microcontroller MCU USB STM32 Cortex-M0",
        }),
        pin_map: None,
        kind: Kind::Standard,
    });
}
