//! Fuzzes Import part's KiCad reader: the S-expression parser and the symbol and footprint
//! mapping must refuse bad input with an error, never a panic. What imports must then
//! write and read back as the same elements. The check itself is in
//! `import_kicad_check.rs`, which a stable test also runs over `seeds/import_kicad`.
//!
//! Input layout: the first two bytes (little-endian) give the length of the `.kicad_sym`
//! text; the rest of the input is the `.kicad_mod` text. Both are read as lossy UTF-8.

#![no_main]

use libfuzzer_sys::fuzz_target;

include!("import_kicad_check.rs");

fuzz_target!(|data: &[u8]| {
    check(data);
});
