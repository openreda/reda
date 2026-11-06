//! Auto-increment: the next name in a repeat placement steps only a trailing number.

use reda_model::auto_increment::{step, step_designator};

#[test]
fn only_a_trailing_number_steps_and_its_padding_is_kept() {
    assert_eq!(step("D09", 1), "D10");
    assert_eq!(step("D01", 1), "D02");
    assert_eq!(step("D99", 1), "D100");
    assert_eq!(step("A1B2", 1), "A1B3");
    assert_eq!(step("DATA0", 2), "DATA2");
    assert_eq!(step("7", 1), "8");
}

#[test]
fn a_trailing_overbar_is_ignored_when_finding_the_number() {
    assert_eq!(step("CS1\\", 1), "CS2\\");
    assert_eq!(step("CS09\\", 1), "CS10\\");
}

#[test]
fn a_name_with_no_trailing_number_repeats_unchanged() {
    assert_eq!(step("RESET", 1), "RESET");
    assert_eq!(step("R\\E\\S\\E\\T\\", 1), "R\\E\\S\\E\\T\\");
    assert_eq!(step("", 1), "");
}

#[test]
fn a_negative_step_decrements_and_stops_at_zero() {
    assert_eq!(step("D10", -1), "D09");
    assert_eq!(step("D3", -2), "D1");
    assert_eq!(step("D1", -5), "D0");
    assert_eq!(step("D00", -1), "D00");
}

#[test]
fn designators_skip_numbers_that_are_taken() {
    let taken = ["R2", "R3", "R5"];
    let free = |d: &str| !taken.contains(&d);
    assert_eq!(step_designator("R1", 1, free).as_deref(), Some("R4"));
    assert_eq!(step_designator("R1", 2, free).as_deref(), Some("R7"));
    assert_eq!(step_designator("R6", -1, free).as_deref(), Some("R4"));
}

#[test]
fn a_designator_with_nowhere_left_to_go_has_no_next() {
    let free = |d: &str| d != "R0";
    assert_eq!(step_designator("R1", -1, free), None);
    assert_eq!(step_designator("R", 1, |_| true), None);
    assert_eq!(step_designator("R1", 0, |_| true), None);
}
