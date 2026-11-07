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

#[test]
fn a_pin_or_pad_number_made_only_of_letters_steps_through_the_alphabet() {
    use reda_model::auto_increment::step_number;
    assert_eq!(step_number("A", 1), "B");
    assert_eq!(step_number("Y", 1), "Z");
    assert_eq!(step_number("Z", 1), "AA");
    assert_eq!(step_number("AZ", 1), "BA");
    assert_eq!(step_number("ZZ", 1), "AAA");
    assert_eq!(step_number("A", 3), "D");
    assert_eq!(step_number("b", 1), "c", "case is kept");
    assert_eq!(step_number("z", 1), "aa");
}

#[test]
fn letter_numbers_count_down_and_stop_at_a() {
    use reda_model::auto_increment::step_number;
    assert_eq!(step_number("B", -1), "A");
    assert_eq!(step_number("AA", -1), "Z");
    assert_eq!(step_number("C", -5), "A");
}

#[test]
fn other_pin_and_pad_numbers_keep_the_trailing_number_rule() {
    use reda_model::auto_increment::step_number;
    assert_eq!(step_number("7", 1), "8");
    assert_eq!(step_number("A9", 1), "A10");
    assert_eq!(step_number("EP", 0), "EP");
    assert_eq!(step_number("+", 1), "+");
    assert_eq!(step_number("Ab", 1), "Ac", "mixed case letters still step");
}

#[test]
fn the_next_number_skips_ones_taken_and_has_none_when_it_cannot_step() {
    use reda_model::auto_increment::next_number;
    let taken = ["2", "3", "C"];
    let free = |n: &str| !taken.contains(&n);
    assert_eq!(next_number("1", 1, free).as_deref(), Some("4"));
    assert_eq!(next_number("B", 1, free).as_deref(), Some("D"));
    assert_eq!(next_number("A1", 2, free).as_deref(), Some("A3"));
    assert_eq!(next_number("+", 1, free), None);
    assert_eq!(next_number("1", 0, free), None);
    assert_eq!(
        next_number("A", -1, |_| true),
        None,
        "nothing comes before A"
    );
}
