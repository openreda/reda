//! Violation text: the Messages panel builds each line from the check id and its
//! arguments through the UI catalogue, never from text the engine sends.

use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::model::{DrcCheck, ErcCheck, LibraryCheck, Severity, Violation};
use reda_ui::i18n;
use reda_ui::messages::{check_title, violation_text};

#[test]
fn a_dangling_wire_names_its_end_in_millimetres() {
    let v = Violation::new("dangling_wire", Severity::Warning)
        .arg("x", Nm(5_080_000))
        .arg("y", Nm(-7_620_000));
    assert_eq!(
        violation_text(&v),
        "Wire end at (5.08mm, -7.62mm) touches nothing"
    );
}

#[test]
fn a_pin_conflict_names_both_pins_and_the_net() {
    let v = Violation::new("pin_conflict", Severity::Error)
        .arg("net", "VCC")
        .arg("pin_a", "U1-2")
        .arg("type_a", "output")
        .arg("pin_b", "U2-5")
        .arg("type_b", "power_out");
    assert_eq!(
        violation_text(&v),
        "Pin conflict on VCC: U1-2 (output) with U2-5 (power_out)"
    );
}

#[test]
fn an_unknown_check_falls_back_to_its_id() {
    let v = Violation::new("from_the_future", Severity::Warning);
    assert_eq!(violation_text(&v), "from_the_future");
}

#[test]
fn every_erc_drc_and_library_check_has_text_and_a_title_in_the_catalogue() {
    let ids = ErcCheck::ALL
        .iter()
        .map(|c| c.id())
        .chain(DrcCheck::ALL.iter().map(|c| c.id()))
        .chain(LibraryCheck::ALL.iter().map(|c| c.id()));
    let missing: Vec<String> = ids
        .flat_map(|id| {
            let id = id.replace('_', "-");
            [format!("violation-{id}"), format!("check-{id}")]
        })
        .filter(|key| !i18n::has_key(key))
        .collect();
    assert!(missing.is_empty(), "missing catalogue keys {missing:?}");
}

#[test]
fn a_check_title_comes_from_the_catalogue() {
    assert_eq!(check_title("dangling_wire"), "Dangling Wire");
    assert_eq!(check_title("from_the_future"), "from_the_future");
}

#[test]
fn library_check_lines_name_what_they_found() {
    let v = Violation::new("missing_pins_in_sequence", Severity::Warning).arg("missing", "3-4, 6");
    assert_eq!(violation_text(&v), "Pin numbers 3-4, 6 are missing");
    let v = Violation::new("origin_off_centroid", Severity::Warning)
        .arg("x", Nm(300_000))
        .arg("y", Nm(0));
    assert_eq!(
        violation_text(&v),
        "The origin is off the body centre at (0.3mm, 0mm)"
    );
    let v = Violation::new("generator_out_of_date", Severity::Warning)
        .arg("generator", "library.footprint.generate.chip");
    assert_eq!(
        violation_text(&v),
        "The Footprint differs from what its stored generator params make: \
         edited by hand, or made by an older generator"
    );
}

#[test]
fn a_pin_matrix_line_names_its_cell() {
    use reda_ui::messages::matrix_cell;
    let v = Violation::new("pin_conflict", Severity::Warning)
        .arg("type_a", "output")
        .arg("type_b", "port_in");
    assert_eq!(matrix_cell(&v), Some(("output".into(), "port_in".into())));
    let v = Violation::new("unconnected_pin", Severity::Error).arg("type", "input");
    assert_eq!(
        matrix_cell(&v),
        Some(("input".into(), "unconnected".into()))
    );
    let v = Violation::new("dangling_wire", Severity::Warning);
    assert_eq!(matrix_cell(&v), None);
}
