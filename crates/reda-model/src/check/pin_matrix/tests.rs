use super::*;
use serde_json::json;

fn cell(a: &str, b: &str) -> Cell {
    Cell::new(
        MatrixType::from_id(a).unwrap(),
        MatrixType::from_id(b).unwrap(),
    )
    .unwrap()
}

/// The cells the spec's table sets, by name in either order; every other cell is Off.
const ERRORS: &[(&str, &str)] = &[
    ("output", "output"),
    ("output", "power_out"),
    ("power_out", "power_out"),
    ("input", "unconnected"),
    ("power_in", "unconnected"),
];
const WARNINGS: &[(&str, &str)] = &[
    ("tri_state", "output"),
    ("open_collector", "output"),
    ("open_emitter", "output"),
    ("tri_state", "power_out"),
    ("open_collector", "power_out"),
    ("open_emitter", "power_out"),
    ("bidirectional", "power_out"),
    ("open_collector", "open_emitter"),
    ("open_collector", "tri_state"),
    ("open_emitter", "tri_state"),
    ("output", "unconnected"),
    ("bidirectional", "unconnected"),
    ("tri_state", "unconnected"),
    ("passive", "unconnected"),
    ("power_out", "unconnected"),
    ("open_collector", "unconnected"),
    ("open_emitter", "unconnected"),
    ("output", "port_in"),
    ("power_out", "port_in"),
    ("bidirectional", "port_in"),
    ("port_out", "port_out"),
];

#[test]
fn every_cell_has_the_documented_default() {
    let named = |list: &[(&str, &str)], c: Cell| list.iter().any(|&(a, b)| cell(a, b) == c);
    let mut seen = 0;
    for c in Cell::all() {
        let expected = if named(ERRORS, c) {
            Severity::Error
        } else if named(WARNINGS, c) {
            Severity::Warning
        } else {
            Severity::Off
        };
        assert_eq!(c.default_severity(), expected, "{}", c.key());
        seen += 1;
    }
    // 13 types pair among themselves (13·14/2 = 91) and 10 Pin types with Unconnected.
    assert_eq!(seen, 101);
}

#[test]
fn the_port_cells_are_off_except_drivers_into_in_and_out_with_out() {
    let ports = ["port_in", "port_out", "port_bidirectional"];
    for p in ports {
        for q in ports {
            let expected = if (p, q) == ("port_out", "port_out") {
                Severity::Warning
            } else {
                Severity::Off
            };
            assert_eq!(cell(p, q).default_severity(), expected, "{p}|{q}");
        }
    }
    for t in [
        "input",
        "tri_state",
        "passive",
        "power_in",
        "open_collector",
        "open_emitter",
        "not_connected",
    ] {
        assert_eq!(cell(t, "port_in").default_severity(), Severity::Off, "{t}");
    }
    for t in ["output", "power_out", "bidirectional", "input", "passive"] {
        assert_eq!(cell(t, "port_out").default_severity(), Severity::Off, "{t}");
        assert_eq!(
            cell(t, "port_bidirectional").default_severity(),
            Severity::Off,
            "{t}"
        );
    }
}

#[test]
fn the_matrix_is_symmetric_and_unconnected_pairs_with_pin_types_only() {
    for &a in MatrixType::ALL {
        for &b in MatrixType::ALL {
            assert_eq!(Cell::new(a, b), Cell::new(b, a), "{a:?} / {b:?}");
        }
    }
    let u = MatrixType::Unconnected;
    assert!(Cell::new(u, u).is_none());
    assert!(Cell::new(MatrixType::Port(PortDirection::In), u).is_none());
    assert_eq!(
        Cell::unconnected(PinType::Passive).key(),
        "passive|unconnected"
    );
}

#[test]
fn keys_name_both_types_in_matrix_order_so_a_pair_has_one_spelling() {
    assert_eq!(cell("power_out", "output").key(), "output|power_out");
    assert_eq!(
        cell("port_in", "bidirectional").key(),
        "bidirectional|port_in"
    );
    assert_eq!(
        Cell::from_key("output|power_out"),
        Some(cell("output", "power_out"))
    );
    assert_eq!(Cell::from_key("power_out|output"), None);
    assert_eq!(Cell::from_key("output|bogus"), None);
    assert_eq!(Cell::from_key("port_in|unconnected"), None);
    assert_eq!(
        cell("input", "unconnected").check(),
        ErcCheck::UnconnectedPin
    );
    assert_eq!(cell("input", "port_in").check(), ErcCheck::PinConflict);
}

#[test]
fn only_cells_that_differ_from_the_default_are_stored() {
    let mut m = PinMatrix::DEFAULT;
    m.set(cell("output", "output"), Some(Severity::Warning));
    m.set(cell("passive", "unconnected"), Some(Severity::Off));
    m.set(cell("input", "input"), Some(Severity::Off)); // already the default
    assert_eq!(
        serde_json::to_value(&m).unwrap(),
        json!({"output|output": "warning", "passive|unconnected": "off"})
    );
    assert_eq!(m.get(cell("output", "output")), Severity::Warning);
    assert_eq!(m.get(cell("output", "power_out")), Severity::Error);
    m.set(cell("output", "output"), None);
    m.set(cell("passive", "unconnected"), Some(Severity::Warning));
    assert!(m.is_default());
}

#[test]
fn a_stored_key_that_is_no_cell_is_refused() {
    let bad: Result<PinMatrix, _> = serde_json::from_value(json!({"power_out|output": "off"}));
    assert!(
        bad.unwrap_err()
            .to_string()
            .contains("not a Pin Matrix cell")
    );
    let good: PinMatrix = serde_json::from_value(json!({"output|output": "off"})).unwrap();
    assert_eq!(good.get(cell("output", "output")), Severity::Off);
}

#[test]
fn erc_settings_keep_severities_and_overrides_in_one_object() {
    let mut s = ErcSettings::DEFAULT;
    assert!(s.is_default());
    s.severities.set(ErcCheck::SinglePinNet, Severity::Off);
    s.pin_matrix_overrides
        .set(cell("output", "output"), Some(Severity::Warning));
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v["single_pin_net"], "off");
    assert_eq!(
        v["pin_matrix_overrides"],
        json!({"output|output": "warning"})
    );
    assert!(v.get("unconnected_pin").is_none());
    assert!(v.get("pin_conflict").is_none());
    let back: ErcSettings = serde_json::from_value(v).unwrap();
    assert_eq!(back, s);
    let typo = serde_json::from_value::<ErcSettings>(json!({"single_pin_nett": "off"}));
    assert!(typo.is_err());
    let merged = serde_json::from_value::<ErcSettings>(json!({"unconnected_input": "off"}));
    assert!(
        merged.is_err(),
        "the merged checks have no severity of their own"
    );
}

#[test]
fn a_matrix_check_is_on_while_any_of_its_cells_is() {
    let mut s = ErcSettings::DEFAULT;
    assert!(s.is_on(ErcCheck::UnconnectedPin));
    assert_eq!(s.severities.get(ErcCheck::PinConflict), None);
    assert!(!s.severities.set(ErcCheck::PinConflict, Severity::Off));
    for c in Cell::all().filter(|c| c.check() == ErcCheck::UnconnectedPin) {
        s.pin_matrix_overrides.set(c, Some(Severity::Off));
    }
    assert!(!s.is_on(ErcCheck::UnconnectedPin));
    assert!(s.is_on(ErcCheck::PinConflict));
    assert!(!s.is_on(ErcCheck::NoDrivingSource));
}
