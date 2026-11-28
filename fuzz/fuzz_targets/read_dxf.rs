//! Fuzzes the DXF reader (`reda-import-dxf`): ASCII or binary DXF in, a drawing or an error
//! out, never a panic. What reads must read the same again and break into pieces. The check
//! itself is in `read_dxf_check.rs`, which a stable test also runs over its seeds, the DXF
//! fixtures in `crates/reda-import-dxf/tests/fixtures`.

#![no_main]

use libfuzzer_sys::fuzz_target;

include!("read_dxf_check.rs");

fuzz_target!(|data: &[u8]| {
    check(data);
});
