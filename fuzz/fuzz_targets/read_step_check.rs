// The body of the `read_step` fuzz target, kept apart so a stable test
// (`crates/reda-step/tests/fuzz_seeds.rs`) runs the same check over the seeds.
//
// The STEP reader must refuse bad input with an error, never a panic, whether the input is
// Part 21 text or gzip. What reads must then write and read back as the same records, and
// converting it to millimetres must not panic either and must write and read back too.

/// Runs one fuzz input; returns the model if it read.
pub fn check(data: &[u8]) -> Option<reda_step::StepModel> {
    let first = reda_step::read(data).ok()?.model;

    let again = reda_step::read(reda_step::write(&first).as_bytes())
        .expect("written text reads back")
        .model;
    let mut want = first.clone();
    want.to_ap214();
    assert_eq!(again, want, "read → write → read changed the records");

    let mut mm = first.clone();
    mm.to_millimetres();
    let back = reda_step::read(reda_step::write(&mm).as_bytes())
        .expect("converted text reads back")
        .model;
    let mut want = mm;
    want.to_ap214();
    assert_eq!(back, want, "a converted model did not read back as written");
    Some(first)
}
