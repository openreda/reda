//! Fuzzes the STEP reader (`reda-step`): Part 21 text or gzip in, a typed model or an error
//! out, never a panic. What reads must write and read back as the same records. The check
//! itself is in `read_step_check.rs`, which a stable test also runs over
//! its seeds, the STEP fixtures in `crates/reda-step/tests/fixtures`.

#![no_main]

use libfuzzer_sys::fuzz_target;

include!("read_step_check.rs");

fuzz_target!(|data: &[u8]| {
    check(data);
});
